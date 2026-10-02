import type { ConfigurationModule } from "./contracts";

export interface SectionTemplateField {
  name: string;
  label: string;
  hint: string;
  widget: "text" | "password" | "number" | "select" | "reference";
  defaultValue?: string;
  required?: boolean;
  options?: readonly { value: string; label: string }[];
  reference?: "interface" | "radio" | "zone";
  placeholder?: string;
  min?: number;
  max?: number;
  hidden?: boolean;
}

export interface SectionTemplate {
  id: string;
  module: ConfigurationModule;
  type: string;
  label: string;
  hint: string;
  deleteImpact: string;
  fields: readonly SectionTemplateField[];
}

type FieldDetails = Omit<
  SectionTemplateField,
  "name" | "label" | "hint" | "widget"
>;

const text = (
  name: string,
  label: string,
  hint: string,
  details: FieldDetails = {},
): SectionTemplateField => ({ name, label, hint, widget: "text", ...details });

const select = (
  name: string,
  label: string,
  hint: string,
  choices: readonly (readonly [string, string])[],
  details: FieldDetails = {},
): SectionTemplateField => ({
  name,
  label,
  hint,
  widget: "select",
  options: choices.map(([value, optionLabel]) => ({
    value,
    label: optionLabel,
  })),
  ...details,
});

const reference = (
  name: string,
  label: string,
  hint: string,
  target: NonNullable<SectionTemplateField["reference"]>,
  details: FieldDetails = {},
): SectionTemplateField => ({
  name,
  label,
  hint,
  widget: "reference",
  reference: target,
  ...details,
});

const tcpUdp = [
  ["tcp", "TCP"],
  ["udp", "UDP"],
  ["tcp udp", "TCP 与 UDP"],
] as const;

// These forms describe new routine sections only. They neither write documents
// nor contact the router. Internal section IDs and reference choices are owned
// by the caller. Reference defaults are suggestions, not invented references.
const templates: Readonly<
  Record<ConfigurationModule, readonly SectionTemplate[]>
> = {
  dhcp: [
    {
      id: "dhcp-host",
      module: "dhcp",
      type: "host",
      label: "静态 DHCP 租约",
      hint: "为指定设备保留 IPv4 地址。新增内容先保留为本地更改，不会立即应用到路由器。",
      deleteImpact: "删除后，此设备不再获得这个保留地址，后续租约可能变化。",
      fields: [
        text(
          "name",
          "主机名称（可选）",
          "用于识别设备及本地域名解析；留空也可保留地址。",
          {
            placeholder: "printer",
          },
        ),
        text(
          "mac",
          "设备 MAC 地址",
          "填写一个六组十六进制 MAC 地址，例如 02:00:00:00:00:01。",
          {
            required: true,
            placeholder: "02:00:00:00:00:01",
          },
        ),
        text(
          "ip",
          "保留 IPv4 地址",
          "填写该设备使用的 IPv4 地址，应位于对应接口的网段内。",
          {
            required: true,
            placeholder: "192.168.31.10",
          },
        ),
      ],
    },
  ],
  firewall: [
    {
      id: "firewall-rule",
      module: "firewall",
      type: "rule",
      label: "防火墙流量规则",
      hint: "设置区域、协议和处理动作。新规则默认停用，确认匹配范围后再选择启用。",
      deleteImpact:
        "删除后不再执行此规则；匹配流量将由其他规则和区域策略处理。",
      fields: [
        text("name", "规则名称", "填写易识别的名称，不是内部章节标识。", {
          required: true,
        }),
        reference("src", "来源区域", "选择流量进入的已有防火墙区域。", "zone", {
          required: true,
          defaultValue: "lan",
        }),
        reference(
          "dest",
          "目标区域（可选）",
          "转发流量时选择目标区域；留空表示访问路由器本机。",
          "zone",
        ),
        select(
          "proto",
          "匹配协议",
          "选择此规则匹配的协议；所有协议会扩大匹配范围。",
          [...tcpUdp, ["icmp", "ICMP"], ["all", "所有协议"]],
          { required: true, defaultValue: "tcp" },
        ),
        text(
          "dest_port",
          "目标端口（可选）",
          "填写 TCP/UDP 端口、范围或空格分隔列表；留空不限制端口。",
          {
            placeholder: "80 443 1000-2000",
          },
        ),
        select(
          "target",
          "处理动作",
          "明确选择允许、丢弃或拒绝；默认拒绝，不会自动开放访问。",
          [
            ["ACCEPT", "允许（ACCEPT）"],
            ["DROP", "丢弃（DROP）"],
            ["REJECT", "拒绝（REJECT）"],
          ],
          { required: true, defaultValue: "REJECT" },
        ),
        select(
          "enabled",
          "规则状态",
          "默认停用。确认来源、目标和端口后，再选择启用此规则。",
          [
            ["0", "停用"],
            ["1", "启用"],
          ],
          { required: true, defaultValue: "0" },
        ),
      ],
    },
    {
      id: "firewall-redirect",
      module: "firewall",
      type: "redirect",
      label: "端口转发",
      hint: "将来源区域的指定端口转发到内网设备。确认服务需要对外开放，检查后再应用。",
      deleteImpact: "删除后，此端口不再转发到目标设备，外部访问可能中断。",
      fields: [
        text("name", "转发名称", "填写目标服务的名称，不是内部章节标识。", {
          required: true,
        }),
        reference(
          "src",
          "来源区域",
          "通常选择已有 WAN 区域；仅当 wan 存在时才使用建议值。",
          "zone",
          {
            required: true,
            defaultValue: "wan",
          },
        ),
        select(
          "proto",
          "转发协议",
          "选择服务使用的 TCP、UDP 或两者。",
          tcpUdp,
          {
            required: true,
            defaultValue: "tcp",
          },
        ),
        text(
          "src_dport",
          "外部端口",
          "填写 1–65535 内的端口或递增范围，例如 8080 或 8000-8010。",
          {
            required: true,
            placeholder: "8080",
          },
        ),
        reference(
          "dest",
          "目标区域（可选）",
          "可选择目标设备所在的已有防火墙区域。",
          "zone",
          {
            defaultValue: "lan",
          },
        ),
        text(
          "dest_ip",
          "目标 IPv4 地址",
          "填写接受转发的内网设备 IPv4 地址，不含子网前缀。",
          {
            required: true,
            placeholder: "192.168.31.10",
          },
        ),
        text(
          "dest_port",
          "内部端口",
          "填写目标服务的端口或递增范围，端口须在 1–65535 内。",
          {
            required: true,
            placeholder: "80",
          },
        ),
        select(
          "target",
          "转发动作（高级）",
          "常规端口转发固定使用目标地址转换 DNAT。",
          [["DNAT", "目标地址转换（DNAT）"]],
          {
            required: true,
            defaultValue: "DNAT",
            hidden: true,
          },
        ),
      ],
    },
  ],
  network: [
    {
      id: "network-interface",
      module: "network",
      type: "interface",
      label: "网络接口",
      hint: "建立逻辑接口，选择接入协议和底层设备。静态地址模式必须填写地址和掩码。",
      deleteImpact: "删除后，此逻辑接口及其地址、路由和关联服务可能不可用。",
      fields: [
        select(
          "proto",
          "接入协议",
          "DHCP 自动获取地址；静态地址手动配置；PPPoE 用于拨号。",
          [
            ["dhcp", "DHCP 客户端"],
            ["static", "静态 IPv4 地址"],
            ["pppoe", "PPPoE 拨号"],
            ["none", "不配置地址"],
          ],
          { required: true, defaultValue: "dhcp" },
        ),
        text(
          "device",
          "底层设备（可选）",
          "填写网卡或网桥名称；由设备和协议决定是否需要。",
          { placeholder: "eth1" },
        ),
        text(
          "ipaddr",
          "IPv4 地址",
          "静态地址模式必填，其他模式可留空；填写单个 IPv4 地址。",
          { placeholder: "192.0.2.2" },
        ),
        text(
          "netmask",
          "子网掩码",
          "静态地址模式必填；使用连续点分掩码或 0–32 前缀长度。",
          { placeholder: "255.255.255.0" },
        ),
        text(
          "username",
          "拨号用户名（可选）",
          "PPPoE 使用运营商提供的用户名；其他模式可留空。",
        ),
        {
          name: "password",
          label: "拨号密码（可选）",
          hint: "PPPoE 使用运营商提供的密码；输入内容隐藏，其他模式可留空。",
          widget: "password",
        },
      ],
    },
    {
      id: "network-route",
      module: "network",
      type: "route",
      label: "IPv4 静态路由",
      hint: "指定目标网络使用的逻辑接口和下一跳。不填写网关时用于直连路由。",
      deleteImpact:
        "删除后，目标网络将使用其他路由；没有其他可用路由时访问会中断。",
      fields: [
        reference(
          "interface",
          "出口接口",
          "选择已有逻辑接口，支持路由器上的厂商接口名称。",
          "interface",
          { required: true },
        ),
        text(
          "target",
          "目标地址或网段",
          "填写 IPv4 地址或 CIDR，例如 192.0.2.0/24；默认路由为 0.0.0.0/0。",
          {
            required: true,
            placeholder: "192.0.2.0/24",
          },
        ),
        text(
          "gateway",
          "下一跳网关（可选）",
          "填写单个 IPv4 网关地址；直连路由可留空。",
          { placeholder: "192.0.2.1" },
        ),
        {
          name: "metric",
          label: "路由度量（可选）",
          hint: "填写非负整数；数值较小的路由通常优先。",
          widget: "number",
          min: 0,
        },
      ],
    },
  ],
  wireless: [
    {
      id: "wireless-wifi-iface",
      module: "wireless",
      type: "wifi-iface",
      label: "无线网络（SSID）",
      hint: "在已有无线射频上新增接入点，选择关联网络、无线名称和加密方式。",
      deleteImpact:
        "删除后，该无线名称不再提供接入；连接此网络的客户端可能断开。",
      fields: [
        reference(
          "device",
          "无线射频",
          "选择同一无线文档中已有的射频章节，支持厂商射频名称。",
          "radio",
          { required: true },
        ),
        reference(
          "network",
          "关联网络",
          "选择已有逻辑接口，将无线客户端接入该网络。",
          "interface",
          {
            required: true,
            defaultValue: "lan",
          },
        ),
        select(
          "mode",
          "工作模式（高级）",
          "常规无线网络固定使用接入点 AP 模式。",
          [["ap", "接入点（AP）"]],
          {
            required: true,
            defaultValue: "ap",
            hidden: true,
          },
        ),
        text(
          "ssid",
          "无线名称（SSID）",
          "填写客户端看到的网络名称，最多 32 个 UTF-8 字节。",
          { required: true, placeholder: "家庭网络" },
        ),
        select(
          "encryption",
          "加密方式",
          "选择客户端支持的加密方式；开放网络无密码，任何人都可接入。",
          [
            ["psk2", "WPA2-PSK"],
            ["sae", "WPA3-SAE"],
            ["sae-mixed", "WPA2 / WPA3 混合"],
            ["none", "开放网络（无密码）"],
          ],
          { required: true, defaultValue: "psk2" },
        ),
        {
          name: "key",
          label: "无线密码",
          hint: "加密网络必须填写 8–63 字节密码，或 64 位十六进制密钥；开放网络可留空。",
          widget: "password",
        },
      ],
    },
  ],
  system: [],
  dropbear: [],
};

export function sectionTemplates(
  module: ConfigurationModule,
): readonly SectionTemplate[] {
  return templates[module];
}

function ipv4(value: string): boolean {
  const octets = value.split(".");
  return (
    octets.length === 4 &&
    octets.every(
      (octet) => /^(0|[1-9]\d{0,2})$/.test(octet) && Number(octet) <= 255,
    )
  );
}

function ipv4Cidr(value: string): boolean {
  const parts = value.split("/");
  return (
    ipv4(parts[0]) &&
    (parts.length === 1 ||
      (parts.length === 2 &&
        /^\d{1,2}$/.test(parts[1]) &&
        Number(parts[1]) <= 32))
  );
}

function netmask(value: string): boolean {
  if (/^\d{1,2}$/.test(value)) return Number(value) <= 32;
  if (!ipv4(value)) return false;
  const bits = value
    .split(".")
    .map((octet) => Number(octet).toString(2).padStart(8, "0"))
    .join("");
  return /^1*0*$/.test(bits);
}

function portRange(value: string): boolean {
  const match = value.match(/^(\d+)(?:[-:](\d+))?$/);
  if (!match) return false;
  const first = Number(match[1]);
  const last = Number(match[2] ?? match[1]);
  return (
    Number.isInteger(first) &&
    Number.isInteger(last) &&
    first >= 1 &&
    last <= 65535 &&
    first <= last
  );
}

function portList(value: string): boolean {
  const ports = value.split(/[\s,]+/).filter(Boolean);
  return ports.length > 0 && ports.every(portRange);
}

function hostname(value: string): boolean {
  return (
    value.length <= 253 &&
    value
      .replace(/\.$/, "")
      .split(".")
      .every(
        (part) =>
          part.length >= 1 &&
          part.length <= 63 &&
          /^[A-Za-z0-9](?:[A-Za-z0-9-]*[A-Za-z0-9])?$/.test(part),
      )
  );
}

const utf8Bytes = (value: string) => new TextEncoder().encode(value).length;

/** Validate exactly the submitted values. Defaults live in field metadata only.
 * Reference existence is checked against candidate documents by the caller and
 * backend, not against a hard-coded set of lan/wan/radio names here.
 */
export function templateErrors(
  template: SectionTemplate,
  values: Record<string, string>,
): Record<string, string> {
  const errors: Record<string, string> = {};
  const value = (name: string) => (values[name] ?? "").trim();
  for (const field of template.fields) {
    const current = value(field.name);
    if (!current) {
      if (field.required) errors[field.name] = `请填写${field.label}。`;
      continue;
    }
    if (
      field.widget === "select" &&
      !field.options?.some((option) => option.value === current)
    ) {
      errors[field.name] = `请选择支持的${field.label}。`;
    }
    if (field.widget === "number") {
      const number = Number(current);
      if (
        !/^\d+$/.test(current) ||
        !Number.isSafeInteger(number) ||
        (field.min !== undefined && number < field.min) ||
        (field.max !== undefined && number > field.max)
      ) {
        errors[field.name] =
          `请填写有效的${field.label}整数${field.min === 0 ? "（不小于 0）" : ""}。`;
      }
    }
  }
  const check = (
    name: string,
    valid: (current: string) => boolean,
    message: string,
  ) => {
    const current = value(name);
    if (current && !valid(current)) errors[name] = message;
  };
  const checkIpv4 = (name: string) =>
    check(
      name,
      ipv4,
      "请填写有效的 IPv4 地址，例如 192.0.2.1。不要填写 CIDR 前缀。",
    );

  if (template.module === "dhcp" && template.type === "host") {
    checkIpv4("ip");
    check(
      "mac",
      (current) =>
        /^(?:[0-9a-f]{2}:){5}[0-9a-f]{2}$/i.test(current) ||
        /^(?:[0-9a-f]{2}-){5}[0-9a-f]{2}$/i.test(current),
      "请填写一个完整 MAC 地址，例如 02:00:00:00:00:01。",
    );
    check(
      "name",
      hostname,
      "主机名称每段须为 1–63 个字母、数字或连字符，且不能以连字符开头或结尾。",
    );
  }
  if (template.module === "firewall" && template.type === "rule") {
    check(
      "dest_port",
      portList,
      "请填写 1–65535 内的端口或递增范围；多个端口用空格或逗号分隔。",
    );
  }
  if (template.module === "firewall" && template.type === "redirect") {
    for (const name of ["src_dport", "dest_port"]) {
      check(
        name,
        portRange,
        "请填写一个 1–65535 内的端口或递增范围，例如 8080 或 8000-8010。",
      );
    }
    checkIpv4("dest_ip");
  }
  if (template.module === "network" && template.type === "interface") {
    if (value("proto") === "static") {
      if (!value("ipaddr")) errors.ipaddr = "静态地址模式必须填写 IPv4 地址。";
      if (!value("netmask")) errors.netmask = "静态地址模式必须填写子网掩码。";
    }
    checkIpv4("ipaddr");
    check("netmask", netmask, "请填写连续的 IPv4 子网掩码或 0–32 前缀长度。");
  }
  if (template.module === "network" && template.type === "route") {
    check(
      "target",
      ipv4Cidr,
      "请填写有效的 IPv4 地址或 CIDR，前缀长度为 0–32。",
    );
    checkIpv4("gateway");
  }
  if (template.module === "wireless" && template.type === "wifi-iface") {
    const ssid = values.ssid ?? "";
    if (utf8Bytes(ssid) > 32 || /[\r\n]/.test(ssid))
      errors.ssid = "无线名称最多 32 个 UTF-8 字节，不能包含换行。";
    const encryption = value("encryption");
    if (encryption && encryption !== "none") {
      const key = values.key ?? "";
      const length = utf8Bytes(key);
      if (
        length < 8 ||
        !(length <= 63 || (length === 64 && /^[0-9a-f]{64}$/i.test(key)))
      ) {
        errors.key = "加密网络必须填写 8–63 字节密码，或 64 位十六进制密钥。";
      }
    }
  }
  return errors;
}
