import { describe, expect, it } from "vitest";
import type { FrpcPlanInput, FrpcProxy } from "../lib/contracts";
import { compileFrpcConfig } from "./frpc-config";

const tcp: FrpcProxy = {
  name: "test-service",
  type: "tcp",
  localAddress: "127.0.0.1",
  localPort: 8080,
  remotePort: 18080,
};
const input = (change: Partial<FrpcPlanInput> = {}): FrpcPlanInput => ({
  serverAddress: "frps.example.test",
  serverPort: 7000,
  transport: "tcp",
  tls: true,
  proxies: [{ ...tcp }],
  ...change,
});

describe("compileFrpcConfig", () => {
  it("accepts port boundaries and exactly 64 named mappings", () => {
    const proxies = Array.from({ length: 64 }, (_, index) => ({
      ...tcp,
      name: `svc-${index}`,
      localPort: 1,
      remotePort: 65535,
    }));
    const config = compileFrpcConfig(input({ proxies, serverPort: 65535 }), "");
    expect(config.match(/\[\[proxies\]\]/g)).toHaveLength(64);
    expect(config).toContain("serverPort = 65535");
    expect(config).toContain("localPort = 1");
  });
  it("preserves valid Unicode while escaping all control characters", () => {
    const controls =
      Array.from({ length: 32 }, (_, point) =>
        String.fromCodePoint(point),
      ).join("") + "\u007f\u0085\u2028\u2029";
    const config = compileFrpcConfig(
      input({ proxies: [{ ...tcp, name: "服务😀" }] }),
      `synthetic${controls}`,
    );
    expect(config).toContain('name = "服务😀"');
    expect(config).not.toMatch(
      /[\u0000-\u0009\u000b-\u001f\u007f-\u009f\u2028\u2029]/u,
    );
    expect(config).toContain(String.raw`\u001F`);
  });
  it.each(["https://frps.example.test", "server\nport=1", "host name"])(
    "rejects server addresses with schemes, whitespace or controls: %s",
    (serverAddress) => {
      expect(() => compileFrpcConfig(input({ serverAddress }), "")).toThrow(
        /服务器地址/,
      );
    },
  );
  it("compiles native TOML with the actual user server, TLS and TCP mapping", () => {
    expect(compileFrpcConfig(input(), "synthetic-token")).toBe(
      'serverAddr = "frps.example.test"\n' +
        "serverPort = 7000\n" +
        'transport.protocol = "tcp"\n' +
        "transport.tls.enable = true\n" +
        'auth.method = "token"\n' +
        'auth.token = "synthetic-token"\n' +
        "\n[[proxies]]\n" +
        'name = "test-service"\n' +
        'type = "tcp"\n' +
        'localIP = "127.0.0.1"\n' +
        "localPort = 8080\n" +
        "remotePort = 18080\n",
    );
  });
  it("omits authentication when the user supplies no token", () => {
    const config = compileFrpcConfig(input(), "");
    expect(config).not.toContain("auth.");
    expect(config).not.toContain("example.com");
  });
  it("escapes quotes, backslashes and TOML controls without creating keys", () => {
    const token = 'synthetic"\\\n\r\t\b\f\u0000\u001b\u007f\u0085';
    const config = compileFrpcConfig(
      input({ proxies: [{ ...tcp, name: 'svc"\\\n[next]\nkey = "value' }] }),
      token,
    );
    expect(config).toContain(
      String.raw`auth.token = "synthetic\"\\\n\r\t\b\f\u0000\u001B\u007F\u0085"`,
    );
    expect(config).toContain(
      String.raw`name = "svc\"\\\n[next]\nkey = \"value"`,
    );
    expect(config).not.toContain("\n[next]");
    expect(config).not.toMatch(/[\u0000-\u0009\u000b-\u001f\u007f]/u);
  });
  it("compiles UDP and HTTP(S) with only their matching native fields", () => {
    const proxies: FrpcProxy[] = [
      { ...tcp, name: "udp", type: "udp", remotePort: 1 },
      {
        ...tcp,
        name: "http",
        type: "http",
        domains: ["app.example.test", "*.example.test"],
      },
      {
        ...tcp,
        name: "https",
        type: "https",
        domains: ["secure.example.test"],
      },
    ];
    const config = compileFrpcConfig(
      input({ proxies, transport: "quic", tls: false }),
      "",
    );
    expect(config).toContain('transport.protocol = "quic"');
    expect(config).toContain("transport.tls.enable = false");
    expect(config).toContain(
      'customDomains = ["app.example.test", "*.example.test"]',
    );
    expect(config).toContain('customDomains = ["secure.example.test"]');
    expect(config.match(/remotePort =/g)).toHaveLength(1);
  });
  it.each([0, 65536, -1, 1.5, NaN, Infinity])(
    "rejects invalid server port %s",
    (serverPort) => {
      expect(() => compileFrpcConfig(input({ serverPort }), "")).toThrow(
        /服务器端口.*1.*65535/,
      );
    },
  );
  it.each(["localPort", "remotePort"] as const)(
    "rejects invalid mapping %s",
    (key) => {
      expect(() =>
        compileFrpcConfig(input({ proxies: [{ ...tcp, [key]: 0 }] }), ""),
      ).toThrow(/映射 1.*端口/);
    },
  );
  it("rejects a missing server and invalid transport or mapping type", () => {
    expect(() => compileFrpcConfig(input({ serverAddress: "" }), "")).toThrow(
      /服务器地址/,
    );
    expect(() =>
      compileFrpcConfig(input({ transport: "invalid" as "tcp" }), ""),
    ).toThrow(/传输协议/);
    expect(() =>
      compileFrpcConfig(
        input({ proxies: [{ ...tcp, type: "invalid" as "tcp" }] }),
        "",
      ),
    ).toThrow(/映射 1.*类型/);
  });
  it.each([0, 65])("rejects invalid mapping count %s", (count) => {
    const proxies = Array.from({ length: count }, (_, i) => ({
      ...tcp,
      name: `svc-${i}`,
    }));
    expect(() => compileFrpcConfig(input({ proxies }), "")).toThrow(
      /映射数量.*1.*64/,
    );
  });
  it.each([
    undefined,
    [],
    [""],
    ["https://example.test"],
    ["a..test"],
    ["bad_domain.test"],
    ["a.*.test"],
    ["-app.test"],
  ])("rejects missing or invalid HTTP domains %s", (domains) => {
    expect(() =>
      compileFrpcConfig(
        input({ proxies: [{ ...tcp, type: "http", domains }] }),
        "",
      ),
    ).toThrow(/映射 1.*域名/);
  });
  it("rejects missing remote port, duplicate names and malformed Unicode", () => {
    expect(() =>
      compileFrpcConfig(
        input({ proxies: [{ ...tcp, remotePort: undefined }] }),
        "",
      ),
    ).toThrow(/映射 1.*远程端口/);
    expect(() => compileFrpcConfig(input({ proxies: [tcp, tcp] }), "")).toThrow(
      /名称.*重复/,
    );
    expect(() => compileFrpcConfig(input(), "\ud800")).toThrow(/令牌.*Unicode/);
  });
});
