import type { FrpcPlanInput } from "../lib/contracts";

const maximumMappings = 64;
const validTypes = new Set(["tcp", "udp", "http", "https"]);

function fail(message: string): never {
  throw new Error(message);
}
function port(value: number | undefined, label: string): number {
  if (
    !Number.isInteger(value) ||
    value === undefined ||
    value < 1 ||
    value > 65535
  )
    fail(`${label}必须为 1–65535 的整数`);
  return value;
}
function address(value: string, label: string): string {
  if (
    typeof value !== "string" ||
    !value.trim() ||
    value.length > 253 ||
    /[\s/\\"'?#@\u0000-\u001f\u007f-\u009f]/u.test(value)
  )
    fail(`${label}必须为主机名或 IP 地址，不含协议、路径或空白`);
  return value;
}
function domain(value: string, label: string): string {
  const host = typeof value === "string" ? value.replace(/^\*\./, "") : "";
  if (
    !host ||
    value.length > 253 ||
    !host
      .split(".")
      .every((part) => /^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$/i.test(part))
  )
    fail(`${label}必须为有效域名（可使用 *. 前缀），不能包含 URL 或端口`);
  return value;
}
/** TOML basic strings cannot contain raw controls or invalid Unicode scalars. */
export function quoteFrpcValue(value: string, label: string): string {
  if (typeof value !== "string") fail(`${label}必须为字符串`);
  const escapes: Record<string, string> = {
    '"': '\\"',
    "\\": "\\\\",
    "\b": "\\b",
    "\t": "\\t",
    "\n": "\\n",
    "\f": "\\f",
    "\r": "\\r",
  };
  let encoded = "";
  for (const char of value) {
    const point = char.codePointAt(0)!;
    if (point >= 0xd800 && point <= 0xdfff)
      fail(`${label}包含无效 Unicode 字符`);
    encoded +=
      escapes[char] ??
      (point < 0x20 ||
      (point >= 0x7f && point <= 0x9f) ||
      point === 0x2028 ||
      point === 0x2029
        ? `\\u${point.toString(16).toUpperCase().padStart(4, "0")}`
        : char);
  }
  return `"${encoded}"`;
}

/** Compile only supplied credentials; native frpc verification runs on the server. */
export function compileFrpcConfig(input: FrpcPlanInput, token: string): string {
  const server = address(input.serverAddress, "服务器地址");
  const serverPort = port(input.serverPort, "服务器端口");
  if (input.transport !== "tcp" && input.transport !== "quic")
    fail("传输协议必须为 TCP 或 QUIC");
  if (typeof input.tls !== "boolean") fail("TLS 必须为布尔值");
  if (
    !Array.isArray(input.proxies) ||
    input.proxies.length < 1 ||
    input.proxies.length > maximumMappings
  )
    fail(`映射数量必须为 1–${maximumMappings}`);
  const quotedToken = quoteFrpcValue(token, "令牌");
  const lines = [
    `serverAddr = ${quoteFrpcValue(server, "服务器地址")}`,
    `serverPort = ${serverPort}`,
    `transport.protocol = ${quoteFrpcValue(input.transport, "传输协议")}`,
    `transport.tls.enable = ${input.tls}`,
  ];
  if (token !== "")
    lines.push('auth.method = "token"', `auth.token = ${quotedToken}`);
  const names = new Set<string>();
  input.proxies.forEach((proxy, index) => {
    const label = `映射 ${index + 1}`;
    if (
      typeof proxy.name !== "string" ||
      !proxy.name.trim() ||
      proxy.name.length > 64
    )
      fail(`${label}名称必须为 1–64 个字符`);
    if (names.has(proxy.name)) fail(`${label}名称与其他映射重复`);
    names.add(proxy.name);
    if (!validTypes.has(proxy.type))
      fail(`${label}类型必须为 TCP、UDP、HTTP 或 HTTPS`);
    lines.push(
      "",
      "[[proxies]]",
      `name = ${quoteFrpcValue(proxy.name, `${label}名称`)}`,
      `type = ${quoteFrpcValue(proxy.type, `${label}类型`)}`,
      `localIP = ${quoteFrpcValue(address(proxy.localAddress, `${label}本地地址`), `${label}本地地址`)}`,
      `localPort = ${port(proxy.localPort, `${label}本地端口`)}`,
    );
    if (proxy.type === "tcp" || proxy.type === "udp") {
      lines.push(`remotePort = ${port(proxy.remotePort, `${label}远程端口`)}`);
    } else {
      if (
        !Array.isArray(proxy.domains) ||
        !proxy.domains.length ||
        proxy.domains.length > 64
      )
        fail(`${label}域名数量必须为 1–64`);
      const domains = proxy.domains.map((value) =>
        quoteFrpcValue(domain(value, `${label}域名`), `${label}域名`),
      );
      lines.push(`customDomains = [${domains.join(", ")}]`);
    }
  });
  return `${lines.join("\n")}\n`;
}
