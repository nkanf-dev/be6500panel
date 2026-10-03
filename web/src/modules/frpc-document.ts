import type { FrpcPlanInput, FrpcProxy } from "../lib/contracts";
import { compileFrpcConfig, quoteFrpcValue } from "./frpc-config";

export type FrpcTokenIntent = {
  mode: "preserve" | "replace" | "clear";
  value: string;
};
type SourceProxy = FrpcProxy & { sourceId?: string };
type Statement = {
  start: number;
  end: number;
  valueStart: number;
  valueEnd: number;
  path: string[];
  mapping?: number;
};
type Table = { start: number; end: number; path: string[]; mapping?: number };
export type SupportedFrpcDocument = {
  supported: true;
  source: string;
  input: FrpcPlanInput;
  hasToken: boolean;
  statements: Statement[];
  tables: Table[];
  mappings: { start: number; end: number; id: string }[];
};
export type FrpcDocument =
  | SupportedFrpcDocument
  | { supported: false; source: string; reason: string };
const equal = (a: unknown, b: unknown) =>
  JSON.stringify(a) === JSON.stringify(b);
export const frpcProxySourceId = (proxy: FrpcProxy) =>
  (proxy as SourceProxy).sourceId;
const key = (path: string[]) => JSON.stringify(path);
const rootKeys = [
  "serverAddr",
  "serverPort",
  "transport.protocol",
  "transport.tls.enable",
];
const proxyKeys = [
  "name",
  "type",
  "localIP",
  "localPort",
  "remotePort",
  "customDomains",
];

/** Scan accepted TOML, not a reserializer. Unknown values are opaque source spans.
 * Unsupported modeled shapes are read-only. Native frpc remains the syntax authority. */
function scan(source: string) {
  if (source.length > 1024 * 1024)
    throw new Error("配置过大，不能安全读入表单");
  const statements: Statement[] = [];
  const tables: Table[] = [{ start: 0, end: source.length, path: [] }];
  const mappings: SupportedFrpcDocument["mappings"] = [];
  let position = 0;
  let table = tables[0];
  const seen = new Set<string>();
  const parsePath = (text: string) => {
    const parts: string[] = [];
    const pattern = /\s*("(?:[^"\\]|\\.)*"|'[^']*'|[A-Za-z0-9_-]+)\s*(\.|$)/gy;
    let match: RegExpExecArray | null;
    while ((match = pattern.exec(text))) {
      parts.push(
        match[1][0] === '"' || match[1][0] === "'"
          ? (scalar(match[1]) as string)
          : match[1],
      );
      if (match[2] === "") return parts;
    }
    throw new Error("键或表路径无法安全识别");
  };
  while (position < source.length) {
    const start = position;
    while (position < source.length && /[ \t\r]/.test(source[position]))
      position++;
    if (source[position] === "#" || source[position] === "\n") {
      const newline = source.indexOf("\n", position);
      position = newline < 0 ? source.length : newline + 1;
      continue;
    }
    if (position >= source.length) break;
    if (source[position] === "[") {
      const newline = source.indexOf("\n", position);
      const end = newline < 0 ? source.length : newline + 1;
      const text = source.slice(position, end).trim();
      const match = /^(\[\[|\[)(.+?)(\]\]|\])\s*(?:#.*)?$/.exec(text);
      if (!match || match[1].length !== match[3].length)
        throw new Error("表结构无法安全识别");
      const path = parsePath(match[2]);
      let mapping: number | undefined;
      if (path[0] === "proxies") {
        if (path.length === 1 && match[1] === "[[") {
          mapping = mappings.length;
          if (mapping >= 64) throw new Error("映射超过 64 项，请使用原生配置");
          mappings.push({
            start,
            end: source.length,
            id: `accepted-${mapping}`,
          });
        } else if (
          path.length > 1 &&
          match[1] === "[" &&
          table.mapping !== undefined
        )
          mapping = table.mapping;
        else throw new Error("映射表结构暂不支持安全表单编辑");
      } else if (match[1] === "[[" && ["auth", "transport"].includes(path[0]))
        throw new Error("连接或认证的数组表暂不支持表单编辑");
      table.end = start;
      if (table.mapping !== undefined && table.mapping !== mapping)
        mappings[table.mapping].end = start;
      table = { start, end: source.length, path, mapping };
      tables.push(table);
      position = end;
      continue;
    }
    let quoted = "";
    while (position < source.length) {
      const char = source[position];
      if (quoted) {
        if (char === "\\" && quoted === '"') position++;
        else if (char === quoted) quoted = "";
      } else if (char === '"' || char === "'") quoted = char;
      else if (char === "=") break;
      else if (char === "\n" || char === "#")
        throw new Error("配置语句缺少赋值");
      position++;
    }
    if (position >= source.length) throw new Error("配置语句缺少赋值");
    const path = [
      ...table.path,
      ...parsePath(source.slice(start, position).trim()),
    ];
    const identity = `${table.mapping ?? "root"}:${key(path)}`;
    if (seen.has(identity)) throw new Error("重复字段不能安全编辑");
    seen.add(identity);
    position++;
    while (/[ \t]/.test(source[position] ?? "") && position < source.length)
      position++;
    const valueStart = position;
    let valueEnd = position;
    let string = "",
      triple = false,
      depth = 0;
    while (position < source.length) {
      const char = source[position];
      if (string) {
        if (char === "\\" && string === '"') {
          position += 2;
          continue;
        }
        if (
          char === string &&
          (!triple || source.slice(position, position + 3) === string.repeat(3))
        ) {
          position += triple ? 3 : 1;
          string = "";
          valueEnd = position;
          continue;
        }
        if (!triple && (char === "\n" || char === "\r"))
          throw new Error("字符串未闭合");
      } else if (char === '"' || char === "'") {
        string = char;
        triple = source.slice(position, position + 3) === char.repeat(3);
        position += triple ? 3 : 1;
        continue;
      } else if (char === "[" || char === "{") depth++;
      else if (char === "]" || char === "}") {
        depth--;
        if (depth < 0) throw new Error("值结构未闭合");
      } else if (char === "#") {
        if (depth === 0) {
          valueEnd = position;
          const newline = source.indexOf("\n", position);
          position = newline < 0 ? source.length : newline + 1;
          break;
        }
        const newline = source.indexOf("\n", position);
        position = newline < 0 ? source.length : newline;
        continue;
      } else if (char === "\n" && depth === 0) {
        valueEnd = position;
        position++;
        break;
      }
      position++;
      valueEnd = position;
    }
    if (string || depth) throw new Error("字符串或值结构未闭合");
    while (valueEnd > valueStart && /\s/.test(source[valueEnd - 1])) valueEnd--;
    if (valueEnd === valueStart) throw new Error("字段值为空");
    statements.push({
      start,
      end: position,
      valueStart,
      valueEnd,
      path,
      mapping: table.mapping,
    });
  }
  return { statements, tables, mappings };
}
function scalar(text: string): unknown {
  text = text.trim();
  if (text.startsWith("'".repeat(3)) || text.startsWith('"'.repeat(3)))
    throw new Error("多行建模字段暂不支持表单编辑");
  if (text[0] === "'") {
    if (!/^'[^'\n\r]*'$/.test(text)) throw new Error("字符串无法安全读取");
    return text.slice(1, -1);
  }
  if (text[0] === '"') {
    const json = text.replace(/\\U([0-9a-fA-F]{8})/g, (_, hex: string) => {
      const point = parseInt(hex, 16);
      if (point > 0x10ffff || (point >= 0xd800 && point <= 0xdfff))
        throw new Error("Unicode 无效");
      return JSON.stringify(String.fromCodePoint(point)).slice(1, -1);
    });
    try {
      return JSON.parse(json);
    } catch {
      throw new Error("字符串无法安全读取");
    }
  }
  if (text === "true" || text === "false") return text === "true";
  if (/^[+-]?(?:[0-9][0-9_]*|0x[0-9a-fA-F_]+|0o[0-7_]+|0b[01_]+)$/.test(text))
    return Number(text.replaceAll("_", ""));
  if (text[0] === "[") {
    const values: string[] = [];
    let position = 1;
    while (position < text.length) {
      while (/[\s,]/.test(text[position] ?? "") && position < text.length)
        position++;
      if (text[position] === "#") {
        const newline = text.indexOf("\n", position);
        position = newline < 0 ? text.length : newline + 1;
        continue;
      }
      if (text[position] === "]" && !text.slice(position + 1).trim())
        return values;
      const match = /^("(?:[^"\\]|\\.)*"|'[^']*')/.exec(text.slice(position));
      if (!match) throw new Error("域名数组无法安全读取");
      values.push(scalar(match[1]) as string);
      position += match[1].length;
      if (!/^[\s,\]#]/.test(text.slice(position)))
        throw new Error("域名数组无法安全读取");
    }
  }
  throw new Error("建模字段的值类型暂不支持表单编辑");
}
export function readFrpcDocument(source: string): FrpcDocument {
  try {
    const scanned = scan(source);
    const values = (mapping?: number) => {
      const result = new Map<string, unknown>();
      for (const statement of scanned.statements) {
        if (statement.mapping !== mapping) continue;
        const components = statement.path.slice(mapping === undefined ? 0 : 1);
        const path = components.join(".");
        if (components.some((part) => part.includes("."))) continue;
        if (
          [
            "auth",
            "transport",
            "proxies",
            "auth.oidc",
            "transport.tls",
          ].includes(path)
        )
          throw new Error("内联连接、认证或映射暂不支持安全表单编辑");
        if (
          (mapping === undefined
            ? [...rootKeys, "auth.method", "auth.token"]
            : proxyKeys
          ).includes(path)
        )
          result.set(
            path,
            scalar(source.slice(statement.valueStart, statement.valueEnd)),
          );
      }
      return result;
    };
    const fields = values();
    const typed = <T>(
      fields: Map<string, unknown>,
      name: string,
      fallback: T,
    ): T => {
      const value = fields.get(name) ?? fallback;
      if (typeof value !== typeof fallback)
        throw new Error(`${name} 值类型暂不支持表单编辑`);
      return value as T;
    };
    const transport = typed(fields, "transport.protocol", "tcp");
    if (transport !== "tcp" && transport !== "quic")
      throw new Error("当前传输协议暂不支持表单编辑");
    if (typed(fields, "auth.method", "token") !== "token")
      throw new Error("当前认证方式暂不支持表单编辑");
    const token = typed(fields, "auth.token", "");
    const proxies: FrpcProxy[] = scanned.mappings.map((mapping, index) => {
      const fields = values(index);
      const type = typed(fields, "type", "tcp");
      if (!["tcp", "udp", "http", "https"].includes(type))
        throw new Error(`映射 ${index + 1} 类型暂不支持表单编辑`);
      const domains = fields.get("customDomains");
      if (
        domains !== undefined &&
        (!Array.isArray(domains) ||
          domains.some((item) => typeof item !== "string"))
      )
        throw new Error("映射域名暂不支持表单编辑");
      return {
        sourceId: mapping.id,
        name: typed(fields, "name", ""),
        type: type as FrpcProxy["type"],
        localAddress: typed(fields, "localIP", "127.0.0.1"),
        localPort: typed(fields, "localPort", 0),
        ...(type === "tcp" || type === "udp"
          ? { remotePort: typed(fields, "remotePort", 0) }
          : { domains: (domains ?? []) as string[] }),
      } as SourceProxy;
    });
    return {
      supported: true,
      source,
      ...scanned,
      hasToken: token !== "",
      input: {
        serverAddress: typed(fields, "serverAddr", ""),
        serverPort: typed(fields, "serverPort", 7000),
        transport,
        tls: typed(fields, "transport.tls.enable", true),
        proxies,
      },
    };
  } catch (cause) {
    return {
      supported: false,
      source,
      reason: cause instanceof Error ? cause.message : "配置无法安全读入表单",
    };
  }
}
function encoded(value: unknown): string {
  if (typeof value === "string") return quoteFrpcValue(value, "字段");
  if (Array.isArray(value)) return `[${value.map(encoded).join(", ")}]`;
  return String(value);
}
function modeled(
  input: FrpcPlanInput,
  index?: number,
): Record<string, unknown> {
  if (index === undefined)
    return {
      serverAddr: input.serverAddress,
      serverPort: input.serverPort,
      "transport.protocol": input.transport,
      "transport.tls.enable": input.tls,
    };
  const proxy = input.proxies[index];
  return {
    name: proxy.name,
    type: proxy.type,
    localIP: proxy.localAddress,
    localPort: proxy.localPort,
    ...(proxy.type === "tcp" || proxy.type === "udp"
      ? { remotePort: proxy.remotePort }
      : { customDomains: proxy.domains }),
  };
}
/** Validate edited modeled fields. Native special/default values remain untouched. */
function validateChanges(original: FrpcPlanInput, input: FrpcPlanInput) {
  const safe: FrpcPlanInput = {
    serverAddress: "validation.invalid",
    serverPort: 7000,
    tls: true,
    transport: "tcp",
    proxies: [
      {
        name: "validation",
        type: "tcp",
        localAddress: "127.0.0.1",
        localPort: 1,
        remotePort: 1,
      },
    ],
  };
  for (const field of [
    "serverAddress",
    "serverPort",
    "transport",
    "tls",
  ] as const)
    if (!equal(original[field], input[field]))
      Object.assign(safe, { [field]: input[field] });
  const names = new Set<string>();
  safe.proxies = input.proxies.map((proxy, index) => {
    if (names.has(proxy.name))
      throw new Error(`映射 ${index + 1} 名称与其他映射重复`);
    names.add(proxy.name);
    const previous = original.proxies.find(
      (item) =>
        frpcProxySourceId(item) === frpcProxySourceId(proxy) &&
        frpcProxySourceId(proxy),
    );
    if (!previous) return proxy;
    const next = {
      ...proxy,
      localAddress: "127.0.0.1",
      localPort: 1,
      remotePort: 1,
      domains: ["validation.invalid"],
    };
    for (const field of [
      "name",
      "type",
      "localAddress",
      "localPort",
      "remotePort",
      "domains",
    ] as const)
      if (!equal(previous[field], proxy[field]))
        Object.assign(next, { [field]: proxy[field] });
    return next;
  });
  if (!safe.proxies.length)
    safe.proxies = [
      {
        name: "validation",
        type: "tcp",
        localAddress: "127.0.0.1",
        localPort: 1,
        remotePort: 1,
      },
    ];
  compileFrpcConfig(safe, "");
}
export function patchFrpcDocument(
  document: SupportedFrpcDocument,
  input: FrpcPlanInput,
  token: FrpcTokenIntent,
): string {
  validateChanges(document.input, input);
  if (token.mode === "replace" && !token.value)
    throw new Error("请输入新密钥，或选择保留 / 清除");
  const edits: { start: number; end: number; text: string }[] = [];
  const inserts = new Map<number, string[]>();
  const newline = document.source.includes("\r\n") ? "\r\n" : "\n";
  const insert = (position: number, text: string) =>
    inserts.set(position, [...(inserts.get(position) ?? []), text]);
  function patch(path: string[], value: unknown, mapping?: number) {
    const statement = document.statements.find(
      (item) => item.mapping === mapping && key(item.path) === key(path),
    );
    if (statement) {
      if (
        value !== undefined &&
        equal(
          scalar(
            document.source.slice(statement.valueStart, statement.valueEnd),
          ),
          value,
        )
      )
        return;
      edits.push(
        value === undefined
          ? { start: statement.start, end: statement.end, text: "" }
          : {
              start: statement.valueStart,
              end: statement.valueEnd,
              text: encoded(value),
            },
      );
      return;
    }
    if (value === undefined) return;
    const candidates = document.tables.filter(
      (table) =>
        table.mapping === mapping &&
        table.path.length < path.length &&
        table.path.every((part, i) => part === path[i]),
    );
    const table = candidates.sort((a, b) => b.path.length - a.path.length)[0];
    if (!table) throw new Error("不能安全添加字段，请使用原生配置");
    insert(
      table.end,
      `${path
        .slice(table.path.length)
        .map((part) => (/^[A-Za-z0-9_-]+$/.test(part) ? part : encoded(part)))
        .join(".")} = ${encoded(value)}${newline}`,
    );
  }
  const root = modeled(input);
  const originalRoot = modeled(document.input);
  for (const [name, value] of Object.entries(root))
    if (!equal(value, originalRoot[name])) patch(name.split("."), value);
  if (token.mode === "replace") {
    patch(["auth", "method"], "token");
    patch(["auth", "token"], token.value);
  }
  if (token.mode === "clear") patch(["auth", "token"], undefined);
  for (const [index, mapping] of document.mappings.entries()) {
    const nextIndex = input.proxies.findIndex(
      (proxy) => frpcProxySourceId(proxy) === mapping.id,
    );
    if (nextIndex < 0) {
      edits.push({ start: mapping.start, end: mapping.end, text: "" });
      continue;
    }
    const original = modeled(document.input, index);
    const next = modeled(input, nextIndex);
    for (const name of new Set([
      ...Object.keys(original),
      ...Object.keys(next),
    ]))
      if (!equal(original[name], next[name]))
        patch(["proxies", name], next[name], index);
  }
  for (const proxy of input.proxies) {
    if (frpcProxySourceId(proxy)) continue;
    const config = compileFrpcConfig({ ...input, proxies: [proxy] }, "");
    insert(
      document.source.length,
      newline +
        config.slice(config.indexOf("[[proxies]]")).replaceAll("\n", newline),
    );
  }
  for (const [position, lines] of inserts)
    edits.push({
      start: position,
      end: position,
      text:
        (position > 0 && !document.source.slice(0, position).endsWith("\n")
          ? newline
          : "") + lines.join(""),
    });
  let source = document.source;
  for (const change of edits.sort((a, b) => b.start - a.start || b.end - a.end))
    source =
      source.slice(0, change.start) + change.text + source.slice(change.end);
  const verified = readFrpcDocument(source);
  if (!verified.supported) throw new Error(`无法安全保存：${verified.reason}`);
  return source;
}
export function redactFrpcDocument(source: string): string {
  const parsed = readFrpcDocument(source);
  if (!parsed.supported)
    return "当前文档只能在原生配置中查看。内容可能包含私密凭据。";
  // Opaque inline tables and multiline custom strings may contain plugin credentials.
  // Hide their full preview span. Never inspect or rewrite their accepted source.
  const sensitive = parsed.statements.filter((item) => {
    const value = source.slice(item.valueStart, item.valueEnd).trim();
    return (
      item.path.some((part) =>
        /token|password|secret|privatekey/i.test(part),
      ) ||
      value.includes("{") ||
      value.startsWith('"'.repeat(3)) ||
      value.startsWith("'".repeat(3))
    );
  });
  for (const statement of sensitive.sort((a, b) => b.valueStart - a.valueStart))
    source =
      source.slice(0, statement.valueStart) +
      '"[令牌已隐藏]"' +
      source.slice(statement.valueEnd);
  return source;
}
/** Three-way field merge. Never silently replace dirty fields or latest custom options. */
export function rebaseFrpcInput(
  original: SupportedFrpcDocument,
  input: FrpcPlanInput,
  latest: SupportedFrpcDocument,
): FrpcPlanInput {
  const merge = <T>(before: T, local: T, remote: T, label: string): T => {
    if (equal(before, local)) return remote;
    if (!equal(before, remote) && !equal(local, remote))
      throw new Error(
        `${label} 已被同时修改；当前输入已保留，请先对照最新配置`,
      );
    return local;
  };
  const result = structuredClone(latest.input);
  for (const field of [
    "serverAddress",
    "serverPort",
    "transport",
    "tls",
  ] as const)
    Object.assign(result, {
      [field]: merge(
        original.input[field],
        input[field],
        latest.input[field],
        "连接字段",
      ),
    });
  for (const [index, before] of original.input.proxies.entries()) {
    if (!frpcProxySourceId(before)) continue;
    const local = input.proxies.find(
      (proxy) => frpcProxySourceId(proxy) === frpcProxySourceId(before),
    );
    const remoteIndex = latest.input.proxies.findIndex(
      (proxy) => proxy.name === before.name,
    );
    const remote = latest.input.proxies[remoteIndex];
    if (!local) {
      if (
        remote &&
        (!equal(
          modeled(original.input, index),
          modeled(latest.input, remoteIndex),
        ) ||
          original.source.slice(
            original.mappings[index].start,
            original.mappings[index].end,
          ) !==
            latest.source.slice(
              latest.mappings[remoteIndex].start,
              latest.mappings[remoteIndex].end,
            ))
      )
        throw new Error("待删除映射已被更新；当前输入已保留");
      result.proxies = result.proxies.filter(
        (proxy) => frpcProxySourceId(proxy) !== frpcProxySourceId(remote),
      );
      continue;
    }
    if (!remote)
      throw new Error("映射名称或结构已变化；当前输入已保留，请对照最新配置");
    const next: SourceProxy = { ...remote };
    for (const field of [
      "name",
      "type",
      "localAddress",
      "localPort",
      "remotePort",
      "domains",
    ] as const)
      Object.assign(next, {
        [field]: merge(
          before[field],
          local[field],
          remote[field],
          `映射 ${before.name}`,
        ),
      });
    const resultIndex = result.proxies.findIndex(
      (proxy) => frpcProxySourceId(proxy) === frpcProxySourceId(remote),
    );
    result.proxies[resultIndex] = next;
  }
  for (const proxy of input.proxies)
    if (!frpcProxySourceId(proxy)) result.proxies.push(proxy);
  return result;
}
