import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { SectionControls, type SectionControlsProps } from "./SectionControls";
import { nativeSections } from "./native-document";
import { sectionTemplates } from "./section-templates";

function controls(props: Partial<SectionControlsProps> = {}) {
  const onChange = vi.fn();
  const values: SectionControlsProps = {
    module: "dhcp",
    content: "",
    documents: {},
    onChange,
    ...props,
  };
  return {
    ...render(<SectionControls {...values} />),
    onChange,
    props: values,
  };
}

function add(template: RegExp) {
  fireEvent.keyDown(screen.getByRole("button", { name: "添加配置" }), {
    key: "ArrowDown",
  });
  fireEvent.click(screen.getByRole("menuitem", { name: template }));
  return screen.getByRole("dialog");
}

function submit(dialog: HTMLElement) {
  fireEvent.click(
    within(dialog).getByRole("button", { name: "添加到待应用更改" }),
  );
}

function suggestions(input: HTMLElement) {
  const list = document.getElementById(input.getAttribute("list")!);
  return [...list!.querySelectorAll("option")].map((option) => ({
    value: option.value,
    label: option.label,
  }));
}

describe("SectionControls local UCI changes", () => {
  it("validates a static lease inline, omits an empty optional name and generates a collision-free identifier", () => {
    const fetchMock = vi.spyOn(globalThis, "fetch");
    const source =
      "# retain source\nconfig host 'panel_host_1'\n option mac '02:00:00:00:00:01' # existing\n option ip '192.0.2.2'\n";
    const { onChange } = controls({ content: source });
    const dialog = add(/静态/);
    expect(dialog).toHaveAccessibleName("添加静态 DHCP 租约");
    expect(dialog).toHaveTextContent(
      "添加后先检查更改，再应用；不会立即影响路由器",
    );
    const advanced = within(dialog).getByText("高级设置").closest("details")!;
    expect(advanced).not.toHaveAttribute("open");
    const hostname = within(dialog).getByLabelText("主机名称（可选）");
    const internalName = within(dialog).getByLabelText("内部配置标识（可选）");
    expect(hostname.id).not.toBe(internalName.id);
    expect(hostname).toHaveValue("");
    expect(internalName).toHaveValue("panel_host_2");
    submit(dialog);
    const mac = within(dialog).getByLabelText("设备 MAC 地址");
    expect(mac).toHaveAttribute("aria-invalid", "true");
    expect(within(dialog).getAllByRole("alert")).toHaveLength(2);
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.change(mac, { target: { value: "02:00:00:00:00:02" } });
    fireEvent.change(within(dialog).getByLabelText("保留 IPv4 地址"), {
      target: { value: "192.0.2.3" },
    });
    submit(dialog);
    const [updated, selected] = onChange.mock.calls[0];
    expect(updated.startsWith(source)).toBe(true);
    expect(selected).toBe("panel_host_2");
    const created = nativeSections(updated).at(-1)!;
    expect(created.name).toBe(selected);
    expect(created.fields.map(({ name, value }) => [name, value])).toEqual([
      ["mac", "02:00:00:00:00:02"],
      ["ip", "192.0.2.3"],
    ]);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("canceling an add keeps local text unchanged and reopening starts a clean form", () => {
    const { onChange } = controls();
    let dialog = add(/静态/);
    fireEvent.change(within(dialog).getByLabelText("设备 MAC 地址"), {
      target: { value: "02:00:00:00:00:04" },
    });
    fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));
    expect(onChange).not.toHaveBeenCalled();
    dialog = add(/静态/);
    expect(within(dialog).getByLabelText("设备 MAC 地址")).toHaveValue("");
  });

  it("checks conditional static interface fields, masks PPPoE password and permits an advanced custom identifier", () => {
    const source = "config interface 'existing'\n option proto 'dhcp'\n";
    const { onChange } = controls({ module: "network", content: source });
    const dialog = add(/^网络接口$/);
    fireEvent.change(within(dialog).getByLabelText("接入协议"), {
      target: { value: "static" },
    });
    submit(dialog);
    expect(within(dialog).getByLabelText("IPv4 地址")).toHaveAttribute(
      "aria-invalid",
      "true",
    );
    expect(within(dialog).getByLabelText("子网掩码")).toHaveAttribute(
      "aria-invalid",
      "true",
    );
    expect(within(dialog).getByLabelText("拨号密码（可选）")).toHaveAttribute(
      "type",
      "password",
    );
    fireEvent.change(within(dialog).getByLabelText("IPv4 地址"), {
      target: { value: "192.0.2.1" },
    });
    fireEvent.change(within(dialog).getByLabelText("子网掩码"), {
      target: { value: "24" },
    });
    fireEvent.click(within(dialog).getByText("高级设置"));
    const id = within(dialog).getByLabelText("内部配置标识（可选）");
    fireEvent.change(id, { target: { value: "existing" } });
    submit(dialog);
    expect(id).toHaveAttribute("aria-invalid", "true");
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.change(id, { target: { value: "vendor_guest" } });
    submit(dialog);
    const [updated, selected] = onChange.mock.calls[0];
    expect(selected).toBe("vendor_guest");
    expect(
      nativeSections(updated)
        .at(-1)!
        .fields.map(({ name, value }) => [name, value]),
    ).toEqual([
      ["proto", "static"],
      ["ipaddr", "192.0.2.1"],
      ["netmask", "24"],
    ]);
  });

  it("uses observed interface and current radio names, keeps vendor references editable and includes hidden wireless defaults", () => {
    const source =
      "config wifi-device 'vendor_radio'\n option name '客厅射频'\n";
    const { onChange } = controls({
      module: "wireless",
      content: source,
      documents: {
        network:
          "config interface 'lan'\n option name '家庭网络'\nconfig interface 'vendor_net'\nconfig interface\n option name 'anonymous'\n",
        wireless: "config wifi-device 'stale_radio'\n",
      },
    });
    const dialog = add(/无线网络/);
    const radio = within(dialog).getByLabelText("无线射频");
    const network = within(dialog).getByLabelText("关联网络");
    expect(suggestions(radio)).toEqual([
      { value: "vendor_radio", label: "客厅射频" },
    ]);
    expect(suggestions(network)).toEqual([
      { value: "lan", label: "家庭网络" },
      { value: "vendor_net", label: "vendor_net" },
    ]);
    expect(network).toHaveValue("lan");
    expect(radio).toHaveValue("");
    const key = within(dialog).getByLabelText("无线密码");
    expect(key).toHaveAttribute("type", "password");
    fireEvent.change(radio, { target: { value: "vendor_radio" } });
    fireEvent.change(network, { target: { value: "new_vendor_ref" } });
    fireEvent.change(within(dialog).getByLabelText("无线名称（SSID）"), {
      target: { value: "Guest's network" },
    });
    fireEvent.change(key, { target: { value: "short" } });
    submit(dialog);
    expect(key).toHaveAttribute("aria-invalid", "true");
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.change(key, { target: { value: "valid-test-password" } });
    submit(dialog);
    const created = nativeSections(onChange.mock.calls[0][0]).at(-1)!;
    expect(
      Object.fromEntries(
        created.fields.map(({ name, value }) => [name, value]),
      ),
    ).toEqual({
      device: "vendor_radio",
      network: "new_vendor_ref",
      mode: "ap",
      ssid: "Guest's network",
      encryption: "psk2",
      key: "valid-test-password",
    });
  });

  it("never invents missing suggested zones and reads zone names from option name, including anonymous zones", () => {
    const source =
      "config zone 'not_the_zone_name'\n option name 'vendor_wan'\nconfig zone\n option name 'guest'\n";
    const { onChange } = controls({ module: "firewall", content: source });
    const dialog = add(/^端口转发$/);
    const sourceZone = within(dialog).getByLabelText("来源区域");
    const destinationZone = within(dialog).getByLabelText("目标区域（可选）");
    expect(sourceZone).toHaveValue("");
    expect(destinationZone).toHaveValue("");
    expect(suggestions(sourceZone).map(({ value }) => value)).toEqual([
      "vendor_wan",
      "guest",
    ]);
    fireEvent.change(within(dialog).getByLabelText("转发名称"), {
      target: { value: "test service" },
    });
    fireEvent.change(sourceZone, { target: { value: "vendor_wan" } });
    fireEvent.change(within(dialog).getByLabelText("外部端口"), {
      target: { value: "8080" },
    });
    fireEvent.change(within(dialog).getByLabelText("目标 IPv4 地址"), {
      target: { value: "192.0.2.3" },
    });
    fireEvent.change(within(dialog).getByLabelText("内部端口"), {
      target: { value: "80" },
    });
    submit(dialog);
    const created = nativeSections(onChange.mock.calls[0][0]).at(-1)!;
    expect(
      Object.fromEntries(
        created.fields.map(({ name, value }) => [name, value]),
      ),
    ).toEqual({
      name: "test service",
      src: "vendor_wan",
      proto: "tcp",
      src_dport: "8080",
      dest_ip: "192.0.2.3",
      dest_port: "80",
      target: "DNAT",
    });
  });

  it("requires route interface selection even without observed interfaces, while accepting explicit vendor names", () => {
    const { onChange } = controls({ module: "network" });
    const dialog = add(/静态路由/);
    const input = within(dialog).getByLabelText("出口接口");
    expect(suggestions(input)).toEqual([]);
    fireEvent.change(within(dialog).getByLabelText("目标地址或网段"), {
      target: { value: "192.0.2.0/24" },
    });
    submit(dialog);
    expect(input).toHaveAttribute("aria-invalid", "true");
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.change(input, { target: { value: "vendor_tunnel" } });
    submit(dialog);
    expect(
      nativeSections(onChange.mock.calls[0][0]).at(-1)!.fields[0].value,
    ).toBe("vendor_tunnel");
  });

  it("shows the affected friendly name and impact, cancels unchanged, then removes only the local selected section", () => {
    const fetchMock = vi.spyOn(globalThis, "fetch");
    const source =
      "# retain\nconfig host 'panel_host_1' # selected comment\n option name 'printer' # host comment\n option mac '02:00:00:00:00:01'\n option ip '192.0.2.2'\nconfig host 'neighbor'\n option ip '192.0.2.3'\n";
    const section = nativeSections(source)[0];
    const { onChange } = controls({ content: source, section });
    fireEvent.click(screen.getByRole("button", { name: "删除当前配置" }));
    let dialog = screen.getByRole("dialog", { name: "删除配置？" });
    expect(dialog).toHaveTextContent("静态 DHCP 租约 · printer");
    expect(dialog).not.toHaveTextContent("panel_host_1");
    expect(dialog).toHaveTextContent(sectionTemplates("dhcp")[0].deleteImpact);
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: "取消" }));
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "删除当前配置" }));
    dialog = screen.getByRole("dialog", { name: "删除配置？" });
    fireEvent.click(
      within(dialog).getByRole("button", { name: "删除并保留为待应用更改" }),
    );
    expect(onChange.mock.calls).toEqual([
      [
        "# retain\n# selected comment\n # host comment\nconfig host 'neighbor'\n option ip '192.0.2.3'\n",
      ],
    ]);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("allows deleting ordinary sections without templates, but blocks unknown directives even in known template sections", () => {
    const source =
      "config system 'main'\n option hostname 'router'\n option vendor_option 'preserved'\n";
    const result = controls({
      module: "system",
      content: source,
      section: nativeSections(source)[0],
    });
    expect(
      screen.queryByRole("button", { name: "添加配置" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "删除当前配置" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "删除当前配置" }));
    fireEvent.click(
      screen.getByRole("button", { name: "删除并保留为待应用更改" }),
    );
    expect(result.onChange).toHaveBeenCalledWith("");
    const unsafe =
      "config host 'printer'\n option ip '192.0.2.2'\n vendor_directive 'retain'\n";
    result.rerender(
      <SectionControls
        {...result.props}
        module="dhcp"
        content={unsafe}
        section={nativeSections(unsafe)[0]}
      />,
    );
    expect(screen.getByRole("button", { name: "删除当前配置" })).toBeDisabled();
    expect(screen.getByText(/无法识别的厂商指令/)).toBeInTheDocument();
  });

  it("disables add/delete while busy and blocks a deletion if the local source changes with its dialog open", () => {
    const source = "config host 'printer'\n option ip '192.0.2.2'\n";
    const result = controls({
      content: source,
      section: nativeSections(source)[0],
      disabled: true,
    });
    expect(screen.getByRole("button", { name: "添加配置" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "删除当前配置" })).toBeDisabled();
    result.rerender(<SectionControls {...result.props} disabled={false} />);
    fireEvent.click(screen.getByRole("button", { name: "删除当前配置" }));
    result.rerender(
      <SectionControls
        {...result.props}
        disabled={false}
        content={`${source}# changed\n`}
      />,
    );
    expect(
      screen.getByRole("button", { name: "删除并保留为待应用更改" }),
    ).toBeDisabled();
    expect(screen.getByRole("alert")).toHaveTextContent("本地配置已改变");
    expect(result.onChange).not.toHaveBeenCalled();
  });
});
