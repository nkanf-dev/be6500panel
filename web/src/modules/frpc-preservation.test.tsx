import { afterEach, describe, expect, it, vi } from "vitest";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FrpcPage } from "./frpc";
import { clearFrpcFormSession, readFrpcFormSession } from "./frpc-session";
import { commitButton, frpcStatus, mockRuntime } from "./frpc-test-support";

vi.mock("../app/console-context", () => ({
  useConsole: () => ({ health: { mode: "host" } }),
}));
afterEach(() => vi.unstubAllGlobals());
const savedToml = `# preserved
serverAddr = "saved.example.test"
serverPort = 7443
transport.protocol = "quic"
transport.tls.enable = false
transport.poolCount = 4
auth.token = "synthetic-accepted-token"
[[proxies]]
name = "ssh"
type = "tcp"
localIP = "192.168.31.9"
localPort = 22
remotePort = 0 # native assigned port
transport.useCompression = true
[[proxies]]
name = "web"
type = "http"
localIP = "127.0.0.1"
localPort = 8081
customDomains = ["home.example.test"]
healthCheck.type = "http"
healthCheck.path = "/ready"
`;
const tokenInput = () => screen.getByLabelText("认证令牌", { exact: true });
async function loaded() {
  await waitFor(
    () =>
      expect(screen.getByRole("textbox", { name: "服务器地址" })).toHaveValue(
        "saved.example.test",
      ),
    { timeout: 10000 },
  );
}
const writes = (fetch: ReturnType<typeof mockRuntime>) =>
  fetch.mock.calls.filter(([url]) => url === "/api/runtime/configure");
describe("frpc mature form readback", () => {
  it("loads all modeled fields, hides the saved token, and does not POST on load, filter or tab switches", async () => {
    const fetch = mockRuntime({ configured: true, config: savedToml });
    const user = userEvent.setup();
    render(<FrpcPage />);
    await loaded();
    expect(screen.getByRole("spinbutton", { name: "服务器端口" })).toHaveValue(
      7443,
    );
    expect(
      screen.getByRole("combobox", { name: "传输协议" }),
    ).toHaveTextContent("QUIC");
    expect(
      screen.getByRole("checkbox", { name: /启用 TLS/ }),
    ).not.toBeChecked();
    const ssh = within(screen.getByRole("region", { name: "服务映射 1" }));
    expect(ssh.getByRole("textbox", { name: "名称" })).toHaveValue("ssh");
    expect(ssh.getByRole("textbox", { name: "本地地址" })).toHaveValue(
      "192.168.31.9",
    );
    expect(ssh.getByRole("spinbutton", { name: "本地端口" })).toHaveValue(22);
    expect(ssh.getByRole("spinbutton", { name: "远程端口" })).toHaveValue(0);
    const web = within(screen.getByRole("region", { name: "服务映射 2" }));
    expect(
      web.getByRole("combobox", { name: "映射 2 类型" }),
    ).toHaveTextContent("HTTP");
    expect(web.getByRole("textbox", { name: /^域名/ })).toHaveValue(
      "home.example.test",
    );
    expect(tokenInput()).toHaveValue("");
    expect(tokenInput()).toHaveAttribute(
      "placeholder",
      "已配置密钥（留空保持不变）",
    );
    expect(screen.getByRole("radio", { name: "保留已保存密钥" })).toBeChecked();
    expect(screen.getByLabelText("frpc TOML 预览")).not.toHaveTextContent(
      "synthetic-accepted-token",
    );
    await user.click(screen.getByRole("tab", { name: "原生配置" }));
    await user.click(screen.getByRole("tab", { name: "连接与映射" }));
    expect(writes(fetch)).toHaveLength(0);
  });
  it("preserves token, special port policy and unknown options on save and again after reopening", async () => {
    const fetch = mockRuntime({ configured: true, config: savedToml });
    const user = userEvent.setup();
    const first = render(<FrpcPage />);
    await loaded();
    fireEvent.change(screen.getByRole("spinbutton", { name: "服务器端口" }), {
      target: { value: "7001" },
    });
    await user.click(commitButton());
    await screen.findByText(/再次编辑默认保留已保存密钥/);
    expect(JSON.parse(writes(fetch)[0][1]?.body as string).config).toBe(
      savedToml.replace("serverPort = 7443", "serverPort = 7001"),
    );
    first.unmount();
    render(<FrpcPage />);
    await loaded();
    expect(tokenInput()).toHaveValue("");
    fireEvent.change(
      within(screen.getByRole("region", { name: "服务映射 1" })).getByRole(
        "spinbutton",
        { name: "本地端口" },
      ),
      { target: { value: "2222" } },
    );
    await waitFor(() => expect(commitButton()).toBeEnabled());
    await user.click(commitButton());
    await waitFor(() => expect(writes(fetch)).toHaveLength(2));
    const config = JSON.parse(writes(fetch)[1][1]?.body as string).config;
    expect(config).toContain('auth.token = "synthetic-accepted-token"');
    expect(config).toContain("remotePort = 0 # native assigned port");
    expect(config).toContain('healthCheck.path = "/ready"');
    expect(config).toContain("localPort = 2222");
  });
  it("requires explicit replacement or clear for saved credentials", async () => {
    const fetch = mockRuntime({ configured: true, config: savedToml });
    const user = userEvent.setup();
    render(<FrpcPage />);
    await loaded();
    await user.click(screen.getByRole("radio", { name: "替换密钥" }));
    expect(commitButton()).toBeDisabled();
    await user.type(tokenInput(), "synthetic-new-token");
    await user.click(commitButton());
    await screen.findByText(/再次编辑默认保留已保存密钥/);
    expect(JSON.parse(writes(fetch)[0][1]?.body as string).config).toContain(
      'auth.token = "synthetic-new-token"',
    );
    expect(tokenInput()).toHaveValue("");
    await user.click(screen.getByRole("radio", { name: "清除密钥" }));
    await waitFor(() => expect(commitButton()).toBeEnabled());
    await user.click(commitButton());
    await waitFor(() => expect(writes(fetch)).toHaveLength(2));
    expect(
      JSON.parse(writes(fetch)[1][1]?.body as string).config,
    ).not.toContain("auth.token");
    expect(JSON.parse(writes(fetch)[1][1]?.body as string).config).toContain(
      'healthCheck.path = "/ready"',
    );
  });
  it("retains dirty input across page unmount, blocks changed generation, and reads latest source before explicit merge", async () => {
    let observed = { ...frpcStatus, configured: true };
    const fetch = mockRuntime({
      observedStatus: () => observed,
      config: savedToml,
    });
    const user = userEvent.setup();
    const first = render(<FrpcPage />);
    await loaded();
    fireEvent.change(screen.getByRole("spinbutton", { name: "服务器端口" }), {
      target: { value: "7001" },
    });
    await user.type(tokenInput(), "synthetic-dirty-token");
    first.unmount();
    observed = { ...observed, generation: 9 };
    render(<FrpcPage />);
    await screen.findByText(/配置已被更新/);
    expect(tokenInput()).toHaveValue("synthetic-dirty-token");
    expect(screen.getByRole("spinbutton", { name: "服务器端口" })).toHaveValue(
      7001,
    );
    expect(commitButton()).toBeDisabled();
    expect(writes(fetch)).toHaveLength(0);
    const reads = fetch.mock.calls.filter(
      ([url]) => url === "/api/runtime/config?service=frpc",
    ).length;
    await user.click(
      screen.getByRole("button", { name: "读取最新配置并合并更改" }),
    );
    await waitFor(() => expect(commitButton()).toBeEnabled());
    expect(
      fetch.mock.calls.filter(
        ([url]) => url === "/api/runtime/config?service=frpc",
      ),
    ).toHaveLength(reads + 1);
    expect(writes(fetch)).toHaveLength(0);
    await user.click(commitButton());
    await waitFor(() => expect(writes(fetch)).toHaveLength(1));
    expect(JSON.parse(writes(fetch)[0][1]?.body as string)).toMatchObject({
      generation: 9,
    });
    expect(JSON.parse(writes(fetch)[0][1]?.body as string).config).toContain(
      'auth.token = "synthetic-dirty-token"',
    );
  });
  it("shows unsupported shapes read-only and offers the secondary native editor without destructive save", async () => {
    const fetch = mockRuntime({
      configured: true,
      config: 'serverAddr = "saved.test"\ntransport.protocol = "kcp"\n',
    });
    const user = userEvent.setup();
    render(<FrpcPage />);
    expect(
      await screen.findByRole("alert", {}, { timeout: 10000 }),
    ).toHaveTextContent("不能安全使用表单编辑");
    expect(screen.getByRole("textbox", { name: "服务器地址" })).toBeDisabled();
    expect(commitButton()).toBeDisabled();
    fireEvent.submit(screen.getByRole("tabpanel"));
    expect(writes(fetch)).toHaveLength(0);
    await user.click(
      screen.getByRole("button", { name: "打开原生配置编辑器" }),
    );
    expect(screen.getByRole("tab", { name: "原生配置" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });
  it("removes saved mapping extensions only after explicit confirmation, then save", async () => {
    const fetch = mockRuntime({ configured: true, config: savedToml });
    const user = userEvent.setup();
    render(<FrpcPage />);
    await loaded();
    await user.click(screen.getByRole("button", { name: "删除映射 2" }));
    expect(
      screen.getByRole("region", { name: "服务映射 2" }),
    ).toBeInTheDocument();
    expect(writes(fetch)).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "确认删除映射" }));
    expect(
      screen.queryByRole("region", { name: "服务映射 2" }),
    ).not.toBeInTheDocument();
    expect(writes(fetch)).toHaveLength(0);
    await user.click(commitButton());
    await waitFor(() => expect(writes(fetch)).toHaveLength(1));
    const config = JSON.parse(writes(fetch)[0][1]?.body as string).config;
    expect(config).not.toContain("healthCheck.path");
    expect(config).toContain("transport.useCompression");
    expect(config).toContain("synthetic-accepted-token");
  });
  it("does not discard dirty intent on reload without confirmation", async () => {
    const fetch = mockRuntime({ configured: true, config: savedToml });
    const user = userEvent.setup();
    render(<FrpcPage />);
    await loaded();
    await user.type(tokenInput(), "synthetic-dirty-token");
    await user.click(screen.getByRole("button", { name: "重新读取配置" }));
    expect(tokenInput()).toHaveValue("synthetic-dirty-token");
    expect(writes(fetch)).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "保留编辑" }));
    expect(tokenInput()).toHaveValue("synthetic-dirty-token");
    await user.click(screen.getByRole("button", { name: "重新读取配置" }));
    await user.click(screen.getByRole("button", { name: "放弃并重新读取" }));
    await waitFor(() => expect(tokenInput()).toHaveValue(""));
    expect(writes(fetch)).toHaveLength(0);
  });
  it("preserves a replacement token when the server rejects the saved document", async () => {
    mockRuntime({ configured: true, config: savedToml, failure: true });
    const user = userEvent.setup();
    render(<FrpcPage />);
    await loaded();
    await user.type(tokenInput(), "synthetic-rejected-token");
    await user.click(commitButton());
    expect(
      await screen.findByRole("alert", {}, { timeout: 10000 }),
    ).toHaveTextContent("generation_conflict");
    expect(tokenInput()).toHaveValue("synthetic-rejected-token");
    expect(readFrpcFormSession().token.value).toBe("synthetic-rejected-token");
  });
  it("keeps later remounted edits when an older save response arrives", async () => {
    const fetch = mockRuntime({ configured: true, config: savedToml });
    const original = fetch.getMockImplementation()!;
    let finish!: () => void;
    fetch.mockImplementation((url, init) => {
      if (url === "/api/runtime/configure")
        return new Promise<Response>((resolve) => {
          finish = () => {
            void original(url, init).then(resolve);
          };
        });
      return original(url, init);
    });
    const user = userEvent.setup();
    const first = render(<FrpcPage />);
    await loaded();
    fireEvent.change(screen.getByRole("spinbutton", { name: "服务器端口" }), {
      target: { value: "7001" },
    });
    await user.click(commitButton());
    await waitFor(() => expect(finish).toBeTypeOf("function"));
    first.unmount();
    const second = render(<FrpcPage />);
    await loaded();
    await user.type(tokenInput(), "synthetic-later-token");
    fireEvent.change(
      within(screen.getByRole("region", { name: "服务映射 1" })).getByRole(
        "spinbutton",
        { name: "本地端口" },
      ),
      { target: { value: "2222" } },
    );
    await act(async () => {
      finish();
    });
    expect(readFrpcFormSession().token.value).toBe("synthetic-later-token");
    expect(readFrpcFormSession().input.proxies[0].localPort).toBe(2222);
    second.unmount();
    render(<FrpcPage />);
    expect(tokenInput()).toHaveValue("synthetic-later-token");
    expect(
      within(screen.getByRole("region", { name: "服务映射 1" })).getByRole(
        "spinbutton",
        { name: "本地端口" },
      ),
    ).toHaveValue(2222);
    await screen.findByText(/配置已被更新/);
    expect(commitButton()).toBeDisabled();
  });
  it("logout clears mounted private draft and old asynchronous readback cannot repopulate it", async () => {
    const fetch = mockRuntime({ configured: true, config: savedToml });
    let resolve!: (response: Response) => void;
    fetch.mockImplementation((url) =>
      url === "/api/runtime/config?service=frpc"
        ? new Promise<Response>((done) => {
            resolve = done;
          })
        : Promise.resolve(
            new Response(
              JSON.stringify({
                enabled: true,
                services: [{ ...frpcStatus, configured: true }],
              }),
              { headers: { "Content-Type": "application/json" } },
            ),
          ),
    );
    const view = render(<FrpcPage />);
    await waitFor(() => expect(resolve).toBeTypeOf("function"));
    clearFrpcFormSession();
    resolve(
      new Response(
        JSON.stringify({ service: "frpc", generation: 7, config: savedToml }),
        { headers: { "Content-Type": "application/json" } },
      ),
    );
    await waitFor(() => expect(readFrpcFormSession().document).toBeUndefined());
    expect(tokenInput()).toHaveValue("");
    view.unmount();
  });
});
