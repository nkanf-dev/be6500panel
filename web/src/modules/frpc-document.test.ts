import { describe, expect, it } from "vitest";
import {
  readFrpcDocument,
  patchFrpcDocument,
  rebaseFrpcInput,
  redactFrpcDocument,
} from "./frpc-document";

export const acceptedToml = `# accepted private configuration
serverAddr = 'saved.example.test' # keep comment
serverPort = 7443
transport.protocol = "quic"
transport.tls.enable = false
transport.poolCount = 3 # custom transport
[auth]
method = "token"
token = "synthetic-saved-token" # private
additionalScopes = ["HeartBeats", "NewWorkConns"]
[[proxies]]
name = "ssh"
type = "tcp"
localIP = "192.168.31.9"
localPort = 22
remotePort = 0 # server assigns this port
transport.useEncryption = true
metadata.owner = "keep me"
[[proxies]]
name = "web"
type = "https"
localPort = 443
customDomains = [
  "home.example.test", # preserve domains comment
]
[proxies.transport]
useCompression = true
`;
function doc(source = acceptedToml) {
  const result = readFrpcDocument(source);
  if (!result.supported) throw new Error(result.reason);
  return result;
}
describe("accepted frpc TOML source preservation", () => {
  it("reads connection, native defaults, token status and all supported mapping fields", () => {
    const result = doc();
    expect(result.hasToken).toBe(true);
    expect(result.input).toMatchObject({
      serverAddress: "saved.example.test",
      serverPort: 7443,
      transport: "quic",
      tls: false,
      proxies: [
        {
          name: "ssh",
          localAddress: "192.168.31.9",
          localPort: 22,
          remotePort: 0,
        },
        {
          name: "web",
          localAddress: "127.0.0.1",
          localPort: 443,
          domains: ["home.example.test"],
        },
      ],
    });
    expect(
      patchFrpcDocument(result, result.input, { mode: "preserve", value: "" }),
    ).toBe(acceptedToml);
  });
  it("patches only changed values while blank credentials and all custom source survive", () => {
    const result = doc();
    const input = structuredClone(result.input);
    input.serverPort = 7001;
    input.proxies[0].localPort = 2222;
    const patched = patchFrpcDocument(result, input, {
      mode: "preserve",
      value: "",
    });
    expect(patched).toBe(
      acceptedToml
        .replace("serverPort = 7443", "serverPort = 7001")
        .replace("localPort = 22", "localPort = 2222"),
    );
    expect(redactFrpcDocument(patched)).not.toContain("synthetic-saved-token");
    expect(redactFrpcDocument(patched)).toContain("[令牌已隐藏]");
  });
  it("replaces or clears only explicitly requested token and preserves auth extensions", () => {
    const result = doc();
    expect(
      patchFrpcDocument(result, result.input, {
        mode: "replace",
        value: "new-token",
      }),
    ).toContain('token = "new-token" # private');
    const cleared = patchFrpcDocument(result, result.input, {
      mode: "clear",
      value: "",
    });
    expect(cleared).not.toContain("token =");
    expect(cleared).toContain(
      'additionalScopes = ["HeartBeats", "NewWorkConns"]',
    );
    expect(() =>
      patchFrpcDocument(result, result.input, { mode: "replace", value: "" }),
    ).toThrow(/密钥/);
  });
  it("removes an entire mapping only with removal intent, including its unmodeled child fields", () => {
    const result = doc();
    const input = { ...result.input, proxies: [result.input.proxies[0]] };
    const patched = patchFrpcDocument(result, input, {
      mode: "preserve",
      value: "",
    });
    expect(patched).not.toContain('name = "web"');
    expect(patched).not.toContain("useCompression");
    expect(patched).toContain('metadata.owner = "keep me"');
  });
  it("inserts missing modeled values in their existing table without moving custom tables", () => {
    const result = doc(
      'serverAddr = "saved.test"\n[transport]\nprotocol = "tcp"\npoolCount = 4\n[[proxies]]\nname = "svc"\ntype = "tcp"\nlocalPort = 80\nremotePort = 8080\n',
    );
    const input = { ...result.input, transport: "quic" as const, tls: false };
    const patched = patchFrpcDocument(result, input, {
      mode: "replace",
      value: "test",
    });
    expect(doc(patched).input).toMatchObject({ transport: "quic", tls: false });
    expect(patched).toContain("poolCount = 4");
    expect(doc(patched).hasToken).toBe(true);
  });
  it.each([
    ['serverAddr = "x"\nauth = { token = "private" }\n', /内联/],
    ['serverAddr = "x"\ntransport.protocol = "kcp"\n', /传输/],
    ['serverAddr = "x"\n[[proxies]]\nname = "x"\ntype = "stcp"\n', /映射/],
    ['serverAddr = "x"\nproxies = [{name = "x"}]\n', /内联/],
    ['serverAddr = "x"\nserverAddr = "y"\n', /重复/],
    ['serverAddr = "unterminated', /字符串/],
  ])(
    "refuses unsupported shapes with a reason instead of destructive normalization",
    (source, reason) => {
      const result = readFrpcDocument(source);
      expect(result.supported).toBe(false);
      if (!result.supported) expect(result.reason).toMatch(reason);
    },
  );
  it("keeps opaque multiline strings, inline tables, arrays, comments and quoted custom keys byte-for-byte", () => {
    const source =
      acceptedToml +
      `
[custom]
"key.with.dots" = { options = [1, 2] }
message = """first
# not a comment
[[proxies]]
last"""
`;
    const result = doc(source);
    expect(
      patchFrpcDocument(
        result,
        { ...result.input, serverPort: 8000 },
        { mode: "preserve", value: "" },
      ),
    ).toBe(source.replace("serverPort = 7443", "serverPort = 8000"));
  });
  it("reads accepted quoted values without a final newline and keeps CRLF source intact", () => {
    expect(doc('serverAddr = "saved.test"').input.serverAddress).toBe(
      "saved.test",
    );
    const source = acceptedToml.replaceAll("\n", "\r\n");
    const result = doc(source);
    expect(
      patchFrpcDocument(
        result,
        { ...result.input, serverPort: 7001 },
        { mode: "preserve", value: "" },
      ),
    ).toBe(source.replace("serverPort = 7443", "serverPort = 7001"));
  });
  it("preserves literal dotted custom keys instead of mistaking them for modeled fields", () => {
    const source =
      acceptedToml + '\n[custom]\n"auth.token" = "private-extension"\n';
    const result = doc(source);
    expect(
      patchFrpcDocument(result, result.input, { mode: "preserve", value: "" }),
    ).toBe(source);
    expect(redactFrpcDocument(source)).not.toContain("private-extension");
  });
  it("blocks deleting mappings whose unknown fields were updated remotely", () => {
    const original = doc();
    const latest = doc(
      acceptedToml.replace("useCompression = true", "useCompression = false"),
    );
    expect(() =>
      rebaseFrpcInput(
        original,
        { ...original.input, proxies: [original.input.proxies[0]] },
        latest,
      ),
    ).toThrow(/待删除映射已被更新/);
  });
  it("hides opaque plugin inline credentials and nested quoted secrets without changing accepted source", () => {
    const source =
      acceptedToml +
      '\n[proxies.plugin]\nsettings = { type = "http_proxy", httpPassword = "synthetic-inline" }\n"quoted.secret" = "synthetic-quoted"\n';
    const result = doc(source);
    expect(
      patchFrpcDocument(result, result.input, { mode: "preserve", value: "" }),
    ).toBe(source);
    const preview = redactFrpcDocument(source);
    expect(preview).not.toContain("synthetic-inline");
    expect(preview).not.toContain("synthetic-quoted");
  });
  it("keeps explicit local deletion on rebase when another field changed remotely", () => {
    const original = doc();
    const latest = doc(acceptedToml.replace("poolCount = 3", "poolCount = 5"));
    const rebased = rebaseFrpcInput(
      original,
      { ...original.input, proxies: [original.input.proxies[0]] },
      latest,
    );
    expect(rebased.proxies.map((proxy) => proxy.name)).toEqual(["ssh"]);
    const patched = patchFrpcDocument(latest, rebased, {
      mode: "preserve",
      value: "",
    });
    expect(patched).not.toContain('name = "web"');
    expect(patched).toContain("poolCount = 5");
  });
  it("rebases only local field intent onto latest readback, preserving new credentials and custom options", () => {
    const original = doc();
    const latest = doc(
      acceptedToml
        .replace("synthetic-saved-token", "rotated-token")
        .replace("poolCount = 3", "poolCount = 5"),
    );
    const input = structuredClone(original.input);
    input.proxies[0].localPort = 2222;
    const rebased = rebaseFrpcInput(original, input, latest);
    const patched = patchFrpcDocument(latest, rebased, {
      mode: "preserve",
      value: "",
    });
    expect(patched).toContain("rotated-token");
    expect(patched).toContain("poolCount = 5");
    expect(patched).toContain("localPort = 2222");
    expect(() =>
      rebaseFrpcInput(
        original,
        input,
        doc(acceptedToml.replace("localPort = 22", "localPort = 23")),
      ),
    ).toThrow(/同时/);
  });
});
