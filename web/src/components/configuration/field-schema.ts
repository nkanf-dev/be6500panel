import type { ConfigurationModule } from "./contracts";
import { fieldHelp } from "./field-help";
import type { FieldHelp } from "./field-help/types";

export interface FieldSchema {
  label: string;
  hint: string;
  widget: "text" | "password" | "boolean" | "number" | "select";
  options?: readonly { value: string; label: string }[];
  min?: number;
  max?: number;
  step?: number;
  placeholder?: string;
  /** Detailed source-backed explanation; never changes the input value or validation. */
  help?: FieldHelp;
}

interface SectionSchema {
  label: string;
  hint: string;
}

interface SectionDefinition extends SectionSchema {
  fields: Readonly<Record<string, FieldSchema>>;
}

const text = (
  label: string,
  hint: string,
  placeholder?: string,
): FieldSchema => ({
  label,
  hint,
  widget: "text",
  ...(placeholder === undefined ? {} : { placeholder }),
});
const password = (label: string, hint: string): FieldSchema => ({
  label,
  hint,
  widget: "password",
});
const boolean = (label: string, hint: string): FieldSchema => ({
  label,
  hint,
  widget: "boolean",
});
const number = (
  label: string,
  hint: string,
  min?: number,
  max?: number,
): FieldSchema => ({
  label,
  hint,
  widget: "number",
  step: 1,
  ...(min === undefined ? {} : { min }),
  ...(max === undefined ? {} : { max }),
});
const select = (
  label: string,
  hint: string,
  choices: readonly (readonly [string, string])[],
): FieldSchema => ({
  label,
  hint,
  widget: "select",
  options: choices.map(([value, optionLabel]) => ({
    value,
    label: optionLabel,
  })),
});
const section = (
  label: string,
  hint: string,
  fields: SectionDefinition["fields"],
): SectionDefinition => ({ label, hint, fields });

const disabled = boolean("停用", "停用此配置；开启表示不使用此接口或服务。");
const enabled = boolean("启用", "启用此配置；关闭表示不使用此规则或服务。");
const address = (label: string) =>
  text(label, "填写 IP 地址或原生 CIDR 地址；保留已有地址格式。", "192.0.2.1");
const mask = text(
  "子网掩码",
  "填写 IPv4 点分掩码或前缀长度。",
  "255.255.255.0",
);
const mac = text(
  "MAC 地址",
  "填写原生 MAC 地址；多个地址或列表项保持原有格式。",
  "02:00:00:00:00:01",
);
const network = text(
  "关联网络",
  "填写 /etc/config/network 中的逻辑接口名称；多个名称或列表项保持原样。",
  "lan",
);
const interfaceName = text(
  "逻辑接口",
  "填写 /etc/config/network 中的逻辑接口名称。",
  "lan",
);
const port = (label: string) =>
  number(label, "填写服务使用的端口号（1–65535）。", 1, 65535);
const seconds = (label: string, hint: string) => number(label, hint, 0);
const policy = (label: string) =>
  select(label, "选择默认处理动作：允许、拒绝并回应，或直接丢弃。", [
    ["ACCEPT", "允许（ACCEPT）"],
    ["REJECT", "拒绝（REJECT）"],
    ["DROP", "丢弃（DROP）"],
  ]);
const family = select("地址族", "选择 IPv4、IPv6 或两者。", [
  ["any", "IPv4 与 IPv6"],
  ["ipv4", "IPv4"],
  ["ipv6", "IPv6"],
]);
const firewallProto = select(
  "匹配协议",
  "选择 IP 协议；多个协议可使用原生列表或空格分隔值。",
  [
    ["tcp", "TCP"],
    ["udp", "UDP"],
    ["tcp udp", "TCP 与 UDP"],
    ["icmp", "ICMP"],
    ["icmpv6", "ICMPv6"],
    ["esp", "ESP"],
    ["ah", "AH"],
    ["all", "所有协议"],
  ],
);
const firewallPorts = (label: string) =>
  text(
    label,
    "填写端口、范围或多个端口；保留原生范围、否定和列表语法。",
    "80 443 1000-2000",
  );
const zoneSource = text(
  "来源区域",
  "填写流量来源的防火墙区域名称；* 表示任意区域。",
  "lan",
);
const zoneDestination = text(
  "目标区域",
  "填写流量目标的防火墙区域名称；* 表示任意区域。",
  "wan",
);
const ruleMatch = {
  name: text("规则名称", "用于识别此规则，不影响匹配行为。"),
  src: zoneSource,
  dest: zoneDestination,
  src_ip: address("来源地址"),
  dest_ip: address("目标地址"),
  src_mac: mac,
  src_port: firewallPorts("来源端口"),
  dest_port: firewallPorts("目标端口"),
  proto: firewallProto,
  family,
  enabled,
  ipset: text("IP 集合匹配", "填写已有 IP 集合名称及原生匹配方向。"),
  mark: text("数据包标记", "填写原生标记及可选掩码，例如 0x1/0xff。"),
  limit: text(
    "速率限制",
    "填写带单位的原生速率；不能仅填写普通数字。",
    "10/second",
  ),
  limit_burst: number("突发上限", "填写速率限制允许的突发包数量。", 0),
};
const routeFields = {
  interface: interfaceName,
  target: text(
    "目标网络",
    "填写目标 IP 地址、网络或 CIDR 前缀。",
    "192.0.2.0/24",
  ),
  gateway: address("下一跳网关"),
  metric: number("路由优先级", "填写非负路由度量值；较小值通常优先。", 0),
  table: text("路由表", "填写路由表数字编号或已注册名称。", "main"),
  type: select("路由类型", "选择原生路由类型。", [
    ["unicast", "单播"],
    ["local", "本地"],
    ["blackhole", "黑洞"],
    ["unreachable", "不可达"],
    ["prohibit", "禁止"],
    ["throw", "继续查表"],
  ]),
  source: address("来源地址"),
  mtu: number("路由 MTU", "填写此路由的最大传输单元，单位为字节。", 576, 65535),
  onlink: boolean("链路内网关", "即使网关不在本地子网，也将其视为直接连接。"),
  disabled,
};
const routingRuleFields = {
  in: text("入站接口", "填写入站逻辑接口名称。"),
  out: text("出站接口", "填写出站逻辑接口名称。"),
  src: address("来源网络"),
  dest: address("目标网络"),
  priority: number("规则优先级", "填写查表规则的优先级；较小值先处理。", 0),
  lookup: text("查询路由表", "填写路由表数字编号或已注册名称。", "main"),
  mark: text("数据包标记", "填写原生标记及可选掩码。", "0x1/0xff"),
  invert: boolean("反向匹配", "反转此策略路由规则的匹配结果。"),
  action: select(
    "规则动作",
    "选择策略路由的原生动作；查询路由表使用独立 lookup 字段。",
    [
      ["unicast", "单播"],
      ["blackhole", "黑洞"],
      ["unreachable", "不可达"],
      ["prohibit", "禁止"],
      ["throw", "继续查表"],
    ],
  ),
  goto: number("跳转优先级", "跳转到指定优先级的策略路由规则。", 0),
  suppress_prefixlength: number(
    "忽略前缀长度",
    "忽略不长于指定前缀长度的路由。",
    0,
    128,
  ),
  disabled,
};
const relayMode = (label: string) =>
  select(label, "选择此接口的 IPv6 服务方式；disabled 表示停用。", [
    ["disabled", "停用"],
    ["server", "服务器"],
    ["relay", "中继"],
    ["hybrid", "混合"],
  ]);
const zonePolicies = {
  input: policy("入站策略"),
  output: policy("出站策略"),
  forward: policy("转发策略"),
};

// Keys are exact UCI module/section/field names, not name-based type guesses.
const definitions: Record<
  ConfigurationModule,
  Readonly<Record<string, SectionDefinition>>
> = {
  network: {
    interface: section("网络接口", "设置逻辑接口的协议、设备、地址与 DNS。", {
      proto: select(
        "接入协议",
        "选择原生 netifd 协议；未列出的厂商协议保留现值。",
        [
          ["static", "静态地址"],
          ["dhcp", "DHCP 客户端"],
          ["dhcpv6", "DHCPv6 客户端"],
          ["pppoe", "PPPoE 拨号"],
          ["none", "不配置地址"],
          ["l2tp", "L2TP"],
          ["pptp", "PPTP"],
        ],
      ),
      device: text("底层设备", "填写此接口使用的网卡或桥设备名称。", "br-lan"),
      ifname: text(
        "网卡名称（旧版）",
        "填写旧版配置中的网卡名称；多个名称保持原有空格或列表格式。",
      ),
      type: select(
        "接口类型（旧版）",
        "旧版接口可使用 bridge 建立网桥；新配置通常在 device 章节中定义。",
        [["bridge", "网桥"]],
      ),
      ipaddr: address("IPv4 地址"),
      netmask: mask,
      gateway: address("IPv4 网关"),
      broadcast: address("广播地址"),
      ip6addr: address("IPv6 地址"),
      ip6gw: address("IPv6 网关"),
      ip6prefix: text(
        "IPv6 委派前缀",
        "填写原生 IPv6 前缀及前缀长度。",
        "2001:db8::/48",
      ),
      ip6assign: number(
        "IPv6 分配长度",
        "填写分配给下游网络的 IPv6 前缀长度。",
        0,
        128,
      ),
      ip6hint: text(
        "IPv6 子网提示",
        "填写十六进制子网标识；不要改为十进制数字。",
        "10",
      ),
      ip6class: text(
        "IPv6 前缀类别",
        "填写上游接口或 local 类别；多个列表项保持原样。",
      ),
      dns: text(
        "DNS 服务器",
        "填写 DNS 服务器地址；多个地址或列表项保持原样。",
        "192.0.2.53",
      ),
      dns_search: text(
        "DNS 搜索域",
        "填写解析时使用的搜索域；支持原生列表项。",
        "lan",
      ),
      peerdns: boolean("使用上游 DNS", "接受 DHCP 或拨号服务器提供的 DNS。"),
      defaultroute: boolean("安装默认路由", "允许此接口安装默认路由。"),
      delegate: boolean(
        "IPv6 前缀委派",
        "允许将获得的 IPv6 前缀分配给下游接口。",
      ),
      ipv6: select("IPv6 模式", "启用、停用或自动建立拨号连接的 IPv6 配置。", [
        ["0", "停用"],
        ["1", "启用"],
        ["auto", "自动"],
      ]),
      auto: boolean("自动启动", "在网络服务启动时自动启用此接口。"),
      force_link: boolean("忽略链路状态", "即使没有载波，也尝试启用此接口。"),
      disabled,
      mtu: number(
        "最大传输单元（MTU）",
        "填写最大传输单元，单位为字节。",
        576,
        65535,
      ),
      metric: number("路由度量", "填写此接口路由的非负度量值。", 0),
      macaddr: mac,
      username: text("拨号用户名", "填写 PPP 或隧道服务的认证用户名。"),
      password: password(
        "拨号密码",
        "填写 PPP 或隧道服务的认证密码；输入内容隐藏。",
      ),
      ac: text(
        "PPPoE 接入集中器",
        "填写需要连接的接入集中器名称；留空使用默认匹配。",
      ),
      service: text("PPPoE 服务名", "填写运营商要求的服务名称。"),
      keepalive: text(
        "连接保活",
        "填写原生失败次数与检测间隔，例如 5 10；这是组合值。",
        "5 10",
      ),
      demand: seconds(
        "空闲断线时间",
        "填写按需拨号的空闲断线时间，单位为秒；0 表示保持连接。",
      ),
      reqaddress: select("IPv6 地址请求", "选择 DHCPv6 客户端是否请求地址。", [
        ["try", "尝试请求"],
        ["force", "必须获取"],
        ["none", "不请求"],
      ]),
      reqprefix: text(
        "IPv6 前缀请求",
        "填写前缀长度，或原生 auto / no 值。",
        "auto",
      ),
    }),
    device: section("网络设备", "设置网卡、网桥和 VLAN 设备属性。", {
      name: text("设备名称", "填写网卡或虚拟设备名称。", "br-lan"),
      type: select(
        "设备类型",
        "选择 netifd 原生设备类型；未列出的设备类型保留现值。",
        [
          ["bridge", "网桥"],
          ["8021q", "802.1Q VLAN"],
          ["8021ad", "802.1ad VLAN"],
        ],
      ),
      ports: text(
        "桥接端口",
        "填写加入网桥的设备名称；每个原生列表项是一个端口。",
        "lan1",
      ),
      ifname: text("父设备", "填写 VLAN 使用的底层设备名称。", "eth0"),
      vid: number("VLAN 编号", "填写 VLAN 标签编号。", 1, 4094),
      mtu: number("设备 MTU", "填写最大传输单元，单位为字节。", 576, 65535),
      macaddr: mac,
      ipv6: boolean("启用 IPv6", "允许此设备使用 IPv6。"),
      stp: boolean("生成树协议", "在网桥上启用 STP，减少二层环路。"),
      igmp_snooping: boolean("IGMP 侦听", "根据组播成员关系优化网桥转发。"),
      multicast_querier: boolean("组播查询器", "在网桥上发送组播成员查询。"),
      bridge_empty: boolean("允许空网桥", "没有成员端口时仍创建网桥。"),
      vlan_filtering: boolean("VLAN 过滤", "根据桥接 VLAN 配置过滤流量。"),
      ageing_time: seconds(
        "MAC 老化时间",
        "填写网桥 MAC 表项的老化时间，单位为秒。",
      ),
      priority: number("STP 优先级", "填写网桥生成树优先级。", 0, 65535),
      disabled,
    }),
    "bridge-vlan": section("桥接 VLAN", "为网桥端口分配 VLAN 和标签方式。", {
      device: text("网桥设备", "填写已定义的网桥设备名称。", "br-lan"),
      vlan: number("VLAN 编号", "填写桥接 VLAN 标签编号。", 1, 4094),
      ports: text(
        "VLAN 端口",
        "填写端口及原生标签标记，保留 :t、:u 和 *。",
        "lan1:u*",
      ),
      local: boolean("本机参与", "允许路由器本机通过此 VLAN 收发流量。"),
    }),
    switch: section("交换芯片", "设置旧版 swconfig 交换芯片。", {
      name: text("交换芯片名称", "填写交换芯片名称。", "switch0"),
      reset: boolean("重置交换芯片", "加载配置时重置交换芯片。"),
      enable_vlan: boolean("启用 VLAN", "在交换芯片中启用 VLAN 隔离。"),
      enable_mirror_rx: boolean("镜像接收流量", "将接收流量复制到监控端口。"),
      enable_mirror_tx: boolean("镜像发送流量", "将发送流量复制到监控端口。"),
      mirror_source_port: number(
        "镜像来源端口",
        "填写交换芯片上的来源端口编号。",
        0,
      ),
      mirror_monitor_port: number(
        "镜像监控端口",
        "填写交换芯片上的监控端口编号。",
        0,
      ),
    }),
    switch_vlan: section("交换芯片 VLAN", "设置旧版交换芯片的 VLAN 成员。", {
      device: text("交换芯片", "填写交换芯片名称。", "switch0"),
      vlan: number("VLAN 表索引", "填写交换芯片 VLAN 表的条目编号。", 0),
      vid: number("VLAN 编号", "填写实际 VLAN 标签编号。", 1, 4094),
      ports: text(
        "成员端口",
        "填写端口编号和 t 等标签标记；保持原生空格格式。",
        "0 1 6t",
      ),
    }),
    route: section("IPv4 静态路由", "设置目标网络、下一跳和路由表。", {
      ...routeFields,
      netmask: mask,
    }),
    route6: section(
      "IPv6 静态路由",
      "设置 IPv6 目标前缀、下一跳和路由表。",
      routeFields,
    ),
    rule: section(
      "IPv4 策略路由",
      "根据来源、目标和标记选择 IPv4 路由表。",
      routingRuleFields,
    ),
    rule6: section(
      "IPv6 策略路由",
      "根据来源、目标和标记选择 IPv6 路由表。",
      routingRuleFields,
    ),
    globals: section("网络全局设置", "设置 IPv6 本地前缀与网络处理方式。", {
      ula_prefix: text(
        "IPv6 ULA 前缀",
        "填写 IPv6 唯一本地地址前缀。",
        "fd00:1234:5678::/48",
      ),
      packet_steering: select(
        "数据包分流",
        "选择是否将网络处理分散到多个 CPU。",
        [
          ["0", "停用"],
          ["1", "启用"],
          ["2", "使用所有 CPU"],
        ],
      ),
    }),
  },
  wireless: {
    "wifi-device": section("无线射频", "设置物理无线设备的频段、信道与功率。", {
      type: text(
        "无线驱动",
        "填写设备使用的原生驱动类型；厂商驱动名称保持原样。",
        "mac80211",
      ),
      path: text("硬件路径", "填写无线设备的原生硬件路径。"),
      phy: text("物理无线设备", "填写内核无线设备标识。", "phy0"),
      macaddr: mac,
      band: select("频段", "选择无线设备使用的频段。", [
        ["2g", "2.4 GHz"],
        ["5g", "5 GHz"],
        ["6g", "6 GHz"],
      ]),
      hwmode: select("无线模式（旧版）", "选择旧版驱动的无线硬件模式。", [
        ["11b", "802.11b"],
        ["11g", "802.11g"],
        ["11a", "802.11a"],
      ]),
      channel: text(
        "无线信道",
        "填写信道编号或 auto；不能将自动信道改为普通数字。",
        "auto",
      ),
      htmode: select(
        "信道宽度",
        "选择设备支持的原生带宽模式；不支持的现值仍保留。",
        [
          ["NOHT", "传统模式"],
          ["HT20", "HT 20 MHz"],
          ["HT40", "HT 40 MHz"],
          ["HT40+", "HT 40 MHz（上扩展）"],
          ["HT40-", "HT 40 MHz（下扩展）"],
          ["VHT20", "VHT 20 MHz"],
          ["VHT40", "VHT 40 MHz"],
          ["VHT80", "VHT 80 MHz"],
          ["VHT160", "VHT 160 MHz"],
          ["HE20", "HE 20 MHz"],
          ["HE40", "HE 40 MHz"],
          ["HE80", "HE 80 MHz"],
          ["HE160", "HE 160 MHz"],
          ["EHT20", "EHT 20 MHz"],
          ["EHT40", "EHT 40 MHz"],
          ["EHT80", "EHT 80 MHz"],
          ["EHT160", "EHT 160 MHz"],
          ["EHT320", "EHT 320 MHz"],
        ],
      ),
      country: text(
        "国家或地区",
        "填写监管域的两字母国家代码；信道和功率受当地规则限制。",
        "CN",
      ),
      txpower: number(
        "发射功率",
        "填写发射功率，单位为 dBm；实际可用范围由设备和监管域决定。",
        0,
        40,
      ),
      disabled,
      legacy_rates: boolean("兼容旧速率", "允许旧式低速无线传输速率。"),
      noscan: boolean(
        "跳过共存扫描",
        "跳过 40 MHz 共存扫描；仅在了解干扰影响时使用。",
      ),
      beacon_int: number(
        "信标间隔",
        "填写无线信标发送间隔，单位为 TU。",
        15,
        65535,
      ),
      distance: number(
        "链路距离",
        "填写用于 ACK 超时计算的链路距离，单位为米。",
        0,
      ),
    }),
    "wifi-iface": section("无线网络", "设置 SSID、加密、密码与关联网络。", {
      device: text(
        "无线设备",
        "填写同一文档中的 wifi-device 章节名称。",
        "radio0",
      ),
      network,
      ifname: text("无线接口名", "填写原生无线网卡名称；通常由驱动自动创建。"),
      mode: select("工作模式", "选择无线接口的原生运行模式。", [
        ["ap", "接入点（AP）"],
        ["sta", "客户端"],
        ["adhoc", "Ad-Hoc"],
        ["mesh", "Mesh"],
        ["monitor", "监听"],
      ]),
      ssid: text("无线名称（SSID）", "填写广播名称，最多 32 字节。"),
      encryption: select(
        "加密方式",
        "选择原生 UCI 加密值；未列出的算法或组合保留现值，直到实际编辑。",
        [
          ["none", "开放网络"],
          ["psk", "WPA-PSK"],
          ["psk2", "WPA2-PSK"],
          ["psk-mixed", "WPA / WPA2-PSK 混合"],
          ["psk+tkip", "WPA-PSK / TKIP"],
          ["psk+ccmp", "WPA-PSK / CCMP"],
          ["psk+tkip+ccmp", "WPA-PSK / TKIP + CCMP"],
          ["psk2+tkip", "WPA2-PSK / TKIP"],
          ["psk2+ccmp", "WPA2-PSK / CCMP"],
          ["psk2+tkip+ccmp", "WPA2-PSK / TKIP + CCMP"],
          ["psk-mixed+tkip", "混合 PSK / TKIP"],
          ["psk-mixed+ccmp", "混合 PSK / CCMP"],
          ["psk-mixed+tkip+ccmp", "混合 PSK / TKIP + CCMP"],
          ["sae", "WPA3-SAE"],
          ["sae-mixed", "WPA2 / WPA3 混合"],
          ["owe", "增强开放网络（OWE）"],
          ["wpa", "WPA 企业认证"],
          ["wpa2", "WPA2 企业认证"],
          ["wpa-mixed", "WPA / WPA2 企业混合"],
          ["wpa3", "WPA3 企业认证"],
          ["wpa3-mixed", "WPA2 / WPA3 企业混合"],
          ["wep-open", "WEP 开放认证（旧版）"],
          ["wep-shared", "WEP 共享认证（旧版）"],
        ],
      ),
      key: password(
        "无线密码 / 密钥",
        "填写与加密方式匹配的密码；WEP 也可填写密钥索引。内容隐藏，未编辑不改变原值。",
      ),
      key1: password("WEP 密钥 1", "填写第一组原生 WEP 密钥；内容隐藏。"),
      key2: password("WEP 密钥 2", "填写第二组原生 WEP 密钥；内容隐藏。"),
      key3: password("WEP 密钥 3", "填写第三组原生 WEP 密钥；内容隐藏。"),
      key4: password("WEP 密钥 4", "填写第四组原生 WEP 密钥；内容隐藏。"),
      disabled,
      hidden: boolean(
        "隐藏 SSID",
        "不在信标中广播无线名称；客户端仍可主动连接。",
      ),
      isolate: boolean("客户端隔离", "阻止同一无线网络的客户端直接互访。"),
      wds: boolean("启用 WDS", "使用四地址模式桥接无线客户端。"),
      wmm: boolean("启用 WMM", "启用无线多媒体流量优先级。"),
      ieee80211r: boolean("快速漫游", "启用 802.11r 快速 BSS 切换。"),
      ieee80211k: boolean("无线测量", "启用 802.11k 无线资源测量。"),
      ieee80211w: select("管理帧保护", "选择受保护管理帧（PMF）的要求。", [
        ["0", "停用"],
        ["1", "可选"],
        ["2", "必须"],
      ]),
      bssid: text(
        "目标 BSSID",
        "填写客户端需要连接的接入点 MAC 地址。",
        "02:00:00:00:00:01",
      ),
      macfilter: select("MAC 过滤", "选择客户端 MAC 地址列表的过滤方式。", [
        ["disable", "停用"],
        ["allow", "仅允许列表"],
        ["deny", "拒绝列表"],
      ]),
      maclist: mac,
      maxassoc: number("客户端上限", "填写最多允许关联的客户端数量。", 0),
      dtim_period: number(
        "DTIM 周期",
        "填写 DTIM 通知跨越的信标周期数。",
        1,
        255,
      ),
      auth_server: text("认证服务器", "填写 RADIUS 认证服务器地址或主机名。"),
      auth_port: port("认证端口"),
      auth_secret: password(
        "认证共享密钥",
        "填写 RADIUS 认证服务器的共享密钥；内容隐藏。",
      ),
      acct_server: text("计费服务器", "填写 RADIUS 计费服务器地址或主机名。"),
      acct_port: port("计费端口"),
      acct_secret: password(
        "计费共享密钥",
        "填写 RADIUS 计费服务器的共享密钥；内容隐藏。",
      ),
      mesh_id: text("Mesh 标识", "填写 Mesh 网络名称。"),
      mesh_fwding: boolean("Mesh 转发", "允许此节点为其他 Mesh 节点转发流量。"),
    }),
  },
  dhcp: {
    dnsmasq: section(
      "DNS 与 DHCP 服务",
      "设置 dnsmasq 的解析、缓存和监听行为。",
      {
        domainneeded: boolean(
          "拒绝无域名查询",
          "不向上游转发没有域名部分的查询。",
        ),
        boguspriv: boolean(
          "过滤私网反向查询",
          "不向上游转发无法在本地解析的私网反向查询。",
        ),
        filterwin2k: boolean(
          "过滤旧式 Windows 查询",
          "过滤特定旧式 Windows DNS 查询。",
        ),
        localise_queries: boolean(
          "本地化查询",
          "根据客户端所在子网返回本地地址。",
        ),
        rebind_protection: boolean(
          "DNS 重绑定保护",
          "阻止上游 DNS 返回不应接受的私网地址。",
        ),
        rebind_localhost: boolean(
          "允许回环重绑定",
          "允许 DNS 重绑定保护中的回环地址例外。",
        ),
        rebind_domain: text(
          "重绑定允许域",
          "填写允许上游返回私网地址的域名；支持原生列表。",
        ),
        local: text(
          "本地域转发规则",
          "填写 dnsmasq 原生本地域语法，例如 /lan/。",
          "/lan/",
        ),
        domain: text("本地域名", "填写 DHCP 与本地主机使用的域名。", "lan"),
        expandhosts: boolean(
          "扩展主机域名",
          "为 hosts 中的短名称追加本地域名。",
        ),
        authoritative: boolean(
          "权威 DHCP 服务",
          "声明本机为此网络的权威 DHCP 服务器。",
        ),
        readethers: boolean(
          "读取 ethers 文件",
          "读取 /etc/ethers 中的静态地址映射。",
        ),
        leasefile: text(
          "租约文件",
          "填写 dnsmasq 保存租约的文件路径。",
          "/tmp/dhcp.leases",
        ),
        resolvfile: text("上游解析文件", "填写上游 DNS 地址文件的路径。"),
        noresolv: boolean(
          "忽略解析文件",
          "仅使用显式 server 配置，不读取上游解析文件。",
        ),
        nohosts: boolean("忽略 hosts 文件", "不读取系统 hosts 文件。"),
        nonwildcard: boolean(
          "限制监听接口",
          "仅在配置允许的接口或地址上监听。",
        ),
        localservice: boolean(
          "仅服务本地网络",
          "仅接受来自本地子网的 DNS 查询。",
        ),
        strictorder: boolean("按顺序查询上游", "按配置顺序尝试 DNS 服务器。"),
        allservers: boolean(
          "同时查询上游",
          "同时向所有上游 DNS 服务器发送查询。",
        ),
        logqueries: boolean("记录 DNS 查询", "在系统日志中记录 DNS 查询。"),
        logdhcp: boolean("记录 DHCP 详情", "在系统日志中记录 DHCP 处理详情。"),
        port: number(
          "DNS 监听端口",
          "填写 DNS 端口；0 表示停用 DNS 功能但可保留 DHCP。",
          0,
          65535,
        ),
        queryport: number(
          "DNS 查询来源端口",
          "填写上游查询来源端口；0 使用随机端口。",
          0,
          65535,
        ),
        cachesize: number(
          "DNS 缓存条目",
          "填写 DNS 缓存条目上限；0 停用缓存。",
          0,
        ),
        dnsforwardmax: number(
          "并发查询上限",
          "填写允许的并发 DNS 转发查询数。",
          0,
        ),
        dhcpleasemax: number("租约数量上限", "填写 DHCP 租约最大数量。", 0),
        ednspacket_max: number(
          "EDNS 数据包上限",
          "填写 EDNS UDP 数据包大小上限，单位为字节。",
          512,
          65535,
        ),
        server: text(
          "上游 DNS",
          "填写地址或 /域名/服务器#端口 等原生转发语法；保留列表。",
          "192.0.2.53",
        ),
        address: text(
          "固定 DNS 解析",
          "填写 /域名/IP地址 等 dnsmasq 原生语法；保留列表。",
          "/example.test/192.0.2.1",
        ),
        interface: text(
          "监听接口",
          "填写服务监听的逻辑接口名称；支持列表。",
          "lan",
        ),
        notinterface: text(
          "排除接口",
          "填写不提供服务的接口名称；支持列表。",
          "wan",
        ),
        listen_address: text(
          "监听地址",
          "填写本机监听地址；支持原生列表项。",
          "127.0.0.1",
        ),
        addnhosts: text(
          "额外 hosts 文件",
          "填写额外 hosts 文件路径；支持原生列表。",
        ),
        dhcp_option: text(
          "DHCP 附加选项",
          "填写编号和值组成的原生 DHCP 选项；不是单个数字。",
          "6,192.0.2.53",
        ),
      },
    ),
    dhcp: section("接口地址分配", "设置每个接口的 DHCP 地址池和 IPv6 通告。", {
      interface: interfaceName,
      start: number(
        "地址池起点",
        "填写相对子网地址的起始偏移量，不是完整 IP 地址。",
        0,
        65535,
      ),
      limit: number("地址池数量", "填写地址池最多分配的地址数量。", 0, 65535),
      leasetime: text(
        "租约时间",
        "填写带单位的时间，例如 12h、30m 或 infinite。",
        "12h",
      ),
      ignore: boolean("停用此地址池", "不在此接口提供 DHCP 地址分配。"),
      force: boolean(
        "强制提供 DHCP",
        "即使检测到其他 DHCP 服务器，也在此接口提供服务。",
      ),
      dynamicdhcp: boolean(
        "动态地址分配",
        "允许为没有静态租约的客户端分配地址。",
      ),
      netmask: mask,
      dhcpv4: select("DHCPv4 模式", "选择此接口是否提供 DHCPv4 服务。", [
        ["disabled", "停用"],
        ["server", "服务器"],
      ]),
      dhcpv6: relayMode("DHCPv6 模式"),
      ra: relayMode("路由器通告模式"),
      ndp: relayMode("NDP 代理模式"),
      master: boolean("IPv6 中继上游", "将此接口作为 IPv6 中继的上游接口。"),
      ra_management: select(
        "RA 管理标志（旧版）",
        "选择旧版路由器通告的 DHCPv6 地址管理方式。",
        [
          ["0", "无状态"],
          ["1", "混合"],
          ["2", "有状态"],
        ],
      ),
      ra_default: select(
        "通告默认路由",
        "选择何时向客户端通告本机为默认路由。",
        [
          ["0", "自动"],
          ["1", "无公网地址也通告"],
          ["2", "始终通告"],
        ],
      ),
      ra_flags: select(
        "RA 标志",
        "选择原生路由器通告标志；每个列表项保持独立。",
        [
          ["managed-config", "受管理地址"],
          ["other-config", "其他配置"],
          ["home-agent", "归属代理"],
          ["none", "无标志"],
        ],
      ),
      ra_slaac: boolean(
        "允许 SLAAC",
        "允许客户端通过通告前缀自动生成 IPv6 地址。",
      ),
      ra_mininterval: seconds(
        "最短通告间隔",
        "填写路由器通告的最短间隔，单位为秒。",
      ),
      ra_maxinterval: seconds(
        "最长通告间隔",
        "填写路由器通告的最长间隔，单位为秒。",
      ),
      ra_lifetime: seconds(
        "默认路由有效期",
        "填写默认路由通告的有效时间，单位为秒。",
      ),
      ra_mtu: number(
        "通告 MTU",
        "填写路由器通告中的链路 MTU，单位为字节。",
        1280,
        65535,
      ),
      dns: text(
        "IPv6 DNS 通告",
        "填写向客户端提供的 DNS 地址；支持原生列表。",
        "2001:db8::53",
      ),
      domain: text(
        "搜索域通告",
        "填写向客户端提供的搜索域；支持原生列表。",
        "lan",
      ),
      dhcp_option: text(
        "DHCP 附加选项",
        "填写编号和值组成的原生 DHCP 选项；保留逗号和列表格式。",
        "6,192.0.2.53",
      ),
      dhcp_option_force: text(
        "强制 DHCP 选项",
        "即使客户端没有请求，也提供此原生 DHCP 选项。",
      ),
    }),
    host: section("静态 DHCP 租约", "按客户端 MAC 或 DUID 固定分配地址。", {
      name: text("客户端名称", "填写此静态租约的主机名称。"),
      mac,
      ip: text(
        "固定 IPv4 地址",
        "填写固定 IPv4 地址，或 ignore 以忽略此客户端。",
        "192.0.2.10",
      ),
      duid: text(
        "客户端 DUID",
        "填写原生 DHCPv6 客户端标识，保留十六进制格式。",
      ),
      hostid: text(
        "IPv6 主机标识",
        "填写十六进制 IPv6 主机标识，不是十进制数字。",
        "10",
      ),
      leasetime: text(
        "专用租约时间",
        "填写此客户端的带单位租约时间或 infinite。",
        "12h",
      ),
      dns: boolean("生成 DNS 记录", "为此静态租约生成本地 DNS 记录。"),
      broadcast: boolean(
        "广播 DHCP 应答",
        "通过广播发送此客户端的 DHCP 应答。",
      ),
      tag: text("客户端标签", "填写关联的 dnsmasq 标签；保留原生格式。"),
    }),
    domain: section("本地 DNS 记录", "将域名映射到指定 IP 地址。", {
      name: text("域名", "填写需要在本地解析的完整域名。", "router.lan"),
      ip: address("解析地址"),
    }),
    odhcpd: section("IPv6 地址服务", "设置 odhcpd 服务与租约文件。", {
      maindhcp: boolean(
        "主要 DHCP 服务",
        "由 odhcpd 作为主要 DHCP 服务提供地址分配。",
      ),
      leasefile: text("租约文件", "填写 odhcpd 保存租约的文件路径。"),
      leasetrigger: text(
        "租约通知路径",
        "已有原生租约通知路径；保留厂商配置并遵循后端校验。",
      ),
      loglevel: number(
        "日志级别",
        "填写 syslog 日志级别编号，0 最紧急、7 最详细。",
        0,
        7,
      ),
    }),
    cname: section("DNS 别名", "将别名映射到另一个域名。", {
      cname: text("别名", "填写 DNS 别名。", "panel.lan"),
      target: text("目标域名", "填写别名指向的域名。", "router.lan"),
      ttl: seconds("记录有效期", "填写 DNS 记录的缓存时间，单位为秒。"),
    }),
    boot: section("网络启动", "设置 DHCP 网络启动文件与服务器。", {
      filename: text("启动文件", "填写客户端请求的网络启动文件名称。"),
      serveraddress: address("启动服务器地址"),
      servername: text("启动服务器名称", "填写网络启动服务器的名称。"),
      networkid: text("客户端标签", "填写匹配此启动配置的 dnsmasq 标签。"),
    }),
    relay: section("DHCP 中继", "将 DHCP 请求转发到指定服务器。", {
      interface: interfaceName,
      local_addr: address("本地地址"),
      server_addr: address("服务器地址"),
    }),
    srvhost: section("DNS SRV 记录", "公布服务的目标主机、端口和优先级。", {
      srv: text(
        "服务域名",
        "填写原生 SRV 服务名称。",
        "_sip._tcp.example.test",
      ),
      target: text("目标主机", "填写提供此服务的主机名。"),
      port: port("服务端口"),
      class: number("优先级", "填写 SRV 记录的优先级；较小值优先。", 0, 65535),
      weight: number("权重", "填写同优先级目标的相对权重。", 0, 65535),
    }),
    mxhost: section("DNS MX 记录", "公布邮件服务器与优先级。", {
      domain: text("邮件域名", "填写此 MX 记录所属的域名。"),
      relay: text("邮件服务器", "填写邮件服务器主机名。"),
      pref: number("优先级", "填写邮件服务器优先级；较小值优先。", 0, 65535),
    }),
  },
  firewall: {
    defaults: section("防火墙默认策略", "设置全局处理策略与连接防护。", {
      ...zonePolicies,
      synflood_protect: boolean("SYN 洪泛保护", "启用 TCP SYN 洪泛防护。"),
      drop_invalid: boolean(
        "丢弃无效连接",
        "丢弃连接跟踪无法识别的无效数据包。",
      ),
      flow_offloading: boolean(
        "软件流量卸载",
        "使用软件流量卸载加速已建立连接。",
      ),
      flow_offloading_hw: boolean(
        "硬件流量卸载",
        "在支持的设备上启用硬件流量卸载。",
      ),
      disable_ipv6: boolean(
        "停用 IPv6 防火墙",
        "不为 IPv6 安装防火墙规则；这不是停用 IPv6 网络。",
      ),
    }),
    zone: section("防火墙区域", "为逻辑网络分组并设置区域策略与 NAT。", {
      name: text("区域名称", "填写此防火墙区域的名称。", "lan"),
      network,
      device: text("匹配设备", "填写区域匹配的网卡名称；支持原生列表。"),
      subnet: text("匹配子网", "填写区域匹配的 IP 子网；支持原生列表。"),
      ...zonePolicies,
      family,
      masq: boolean(
        "IPv4 地址伪装",
        "对离开此区域的 IPv4 流量进行源地址转换。",
      ),
      masq6: boolean(
        "IPv6 地址伪装",
        "对离开此区域的 IPv6 流量进行源地址转换。",
      ),
      masq_src: text(
        "伪装来源限制",
        "填写允许或排除的来源网络；保留原生否定和列表语法。",
      ),
      masq_dest: text(
        "伪装目标限制",
        "填写允许或排除的目标网络；保留原生否定和列表语法。",
      ),
      mtu_fix: boolean("修正 TCP MSS", "根据路径 MTU 修正 TCP 最大分段大小。"),
      log: boolean("记录区域拒绝", "记录被区域规则丢弃或拒绝的数据包。"),
      log_limit: text("日志速率限制", "填写原生带单位日志速率。", "10/minute"),
      enabled,
    }),
    forwarding: section("区域间转发", "允许从一个防火墙区域转发到另一区域。", {
      src: zoneSource,
      dest: zoneDestination,
      enabled,
    }),
    rule: section("流量规则", "按地址、协议和端口匹配并处理数据包。", {
      ...ruleMatch,
      target: select("处理动作", "选择匹配流量后的原生规则动作。", [
        ["ACCEPT", "允许"],
        ["REJECT", "拒绝"],
        ["DROP", "丢弃"],
        ["MARK", "设置标记"],
        ["NOTRACK", "不跟踪连接"],
        ["HELPER", "指定连接助手"],
        ["DSCP", "设置 DSCP"],
      ]),
      icmp_type: text(
        "ICMP 类型",
        "填写 ICMP 类型名称或编号；支持列表。",
        "echo-request",
      ),
      set_mark: text("设置标记", "填写原生数据包标记及可选掩码。", "0x1/0xff"),
      set_xmark: text("扩展标记", "填写异或设置的数据包标记及可选掩码。"),
      helper: text("连接助手", "填写需要匹配或启用的连接跟踪助手名称。"),
      start_time: text("每日开始时间", "填写原生时间字符串。", "08:00:00"),
      stop_time: text("每日结束时间", "填写原生时间字符串。", "18:00:00"),
      weekdays: text(
        "星期限制",
        "填写原生星期名称或列表。",
        "Mon Tue Wed Thu Fri",
      ),
      utc_time: boolean("使用 UTC 时间", "按 UTC 而不是本地时间匹配时间规则。"),
    }),
    redirect: section(
      "端口转发 / 地址转换",
      "将匹配的地址与端口重定向到指定目标。",
      {
        ...ruleMatch,
        target: select(
          "转换类型",
          "选择目标地址转换（DNAT）或源地址转换（SNAT）。",
          [
            ["DNAT", "目标地址转换（DNAT）"],
            ["SNAT", "源地址转换（SNAT）"],
          ],
        ),
        src_dip: address("外部目标地址"),
        src_dport: firewallPorts("外部目标端口"),
        reflection: boolean(
          "NAT 回环",
          "允许内部客户端通过外部地址访问转发服务。",
        ),
        reflection_src: select(
          "回环来源地址",
          "选择 NAT 回环使用的来源地址。",
          [
            ["internal", "内部地址"],
            ["external", "外部地址"],
          ],
        ),
        reflection_zone: text(
          "回环区域",
          "填写启用 NAT 回环的防火墙区域；支持列表。",
        ),
      },
    ),
    nat: section("源地址转换", "为匹配的出站流量设置源地址转换。", {
      ...ruleMatch,
      target: select("转换动作", "选择源地址转换或动态地址伪装。", [
        ["SNAT", "源地址转换（SNAT）"],
        ["MASQUERADE", "动态地址伪装"],
      ]),
      snat_ip: address("转换后来源地址"),
      snat_port: firewallPorts("转换后来源端口"),
    }),
    include: section(
      "防火墙附加配置",
      "保留已有原生附加配置；是否可修改由后端校验决定。",
      {
        path: text(
          "附加文件路径",
          "已有附加文件的原生路径；新增执行脚本不属于普通字段配置。",
        ),
        type: select("附加配置类型", "选择原生附加配置的文件类型。", [
          ["script", "已有脚本"],
          ["nftables", "nftables 规则"],
        ]),
        enabled,
        reload: boolean("重载时读取", "在防火墙重载时读取已有附加配置。"),
        fw4_compatible: boolean("兼容 fw4", "声明已有附加配置兼容 fw4。"),
      },
    ),
    ipset: section("IP 集合", "集中管理地址、网段或端口的匹配集合。", {
      name: text("集合名称", "填写规则引用的 IP 集合名称。"),
      family,
      match: text("匹配内容", "填写原生匹配类型和方向；支持列表。", "src_net"),
      storage: select("存储类型", "选择 IP 集合的原生存储类型。", [
        ["hash", "哈希"],
        ["bitmap", "位图"],
        ["list", "列表"],
      ]),
      entry: text(
        "集合成员",
        "填写原生地址、网段或端口组合；每个列表项保持独立。",
      ),
      maxelem: number("成员数量上限", "填写集合最多可保存的成员数量。", 1),
      timeout: seconds(
        "成员超时",
        "填写动态成员的有效时间，单位为秒；0 表示不超时。",
      ),
      enabled,
    }),
  },
  system: {
    system: section("系统设置", "设置主机名称、时区和系统日志。", {
      hostname: text("主机名称", "填写路由器的主机名。", "router"),
      timezone: text(
        "POSIX 时区",
        "填写原生 POSIX 时区字符串，不是时区显示名称。",
        "CST-8",
      ),
      zonename: text(
        "时区名称",
        "填写 IANA 时区名称；与 timezone 分别保存。",
        "Asia/Shanghai",
      ),
      description: text("系统描述", "填写此设备的描述信息。"),
      notes: text("备注", "填写设备备注，保留原生内容。"),
      log_size: number(
        "日志缓冲区大小",
        "填写内存日志缓冲区大小，单位为 KiB。",
        0,
      ),
      log_ip: text(
        "远程日志地址",
        "填写远程 syslog 服务器地址或主机名。",
        "192.0.2.20",
      ),
      log_port: port("远程日志端口"),
      log_proto: select("远程日志协议", "选择远程 syslog 的传输协议。", [
        ["udp", "UDP"],
        ["tcp", "TCP"],
      ]),
      log_remote: boolean("启用远程日志", "将系统日志发送到远程日志服务器。"),
      log_file: text("日志文件", "填写本地日志文件的路径。"),
      conloglevel: number(
        "控制台日志级别",
        "填写内核控制台日志级别，0 最少、8 最详细。",
        0,
        8,
      ),
      cronloglevel: number(
        "定时任务日志级别",
        "填写 cron 的原生日志级别，数值越小越详细。",
        0,
        8,
      ),
    }),
    timeserver: section("网络时间同步", "设置 NTP 客户端与服务器。", {
      enabled,
      enable_server: boolean("提供 NTP 服务", "允许其他设备从本机同步时间。"),
      server: text(
        "NTP 服务器",
        "填写时间服务器主机名或地址；每个原生列表项是一个服务器。",
        "time.example.test",
      ),
      use_dhcp: boolean(
        "使用 DHCP 时间服务器",
        "接受 DHCP 提供的 NTP 服务器。",
      ),
      interface: interfaceName,
    }),
    led: section("指示灯", "设置 LED 设备、默认状态与触发条件。", {
      name: text("指示灯名称", "填写此指示灯配置的显示名称。"),
      sysfs: text("LED 设备", "填写 /sys/class/leds 下的设备名称。"),
      default: boolean("默认点亮", "在触发器接管前使用点亮状态。"),
      trigger: text(
        "触发器",
        "填写内核支持的 LED 触发器名称；厂商触发器保持原样。",
        "netdev",
      ),
      dev: text("关联设备", "填写网络触发器监控的网卡名称。", "eth0"),
      mode: text(
        "网络触发模式",
        "填写 link、tx、rx 等原生组合或列表。",
        "link tx rx",
      ),
      delayon: number("点亮时长", "填写定时闪烁的点亮时间，单位为毫秒。", 0),
      delayoff: number("熄灭时长", "填写定时闪烁的熄灭时间，单位为毫秒。", 0),
      interval: number("检查间隔", "填写触发器检查间隔，单位为毫秒。", 0),
    }),
  },
  dropbear: {
    dropbear: section("SSH 服务", "设置 Dropbear 监听端口、接口与登录认证。", {
      Port: port("SSH 端口"),
      Interface: text(
        "监听逻辑接口",
        "填写监听的逻辑接口名称；留空通常监听所有接口。",
        "lan",
      ),
      PasswordAuth: boolean("允许密码认证", "允许 SSH 用户使用密码认证。"),
      RootPasswordAuth: boolean(
        "允许 root 密码认证",
        "允许 root 使用密码认证；不影响公钥认证。",
      ),
      RootLogin: boolean("允许 root 登录", "允许 root 用户通过 SSH 登录。"),
      GatewayPorts: boolean(
        "允许远程转发外部监听",
        "允许远程端口转发监听非回环地址。",
      ),
      IdleTimeout: seconds(
        "空闲超时",
        "填写 SSH 会话空闲超时，单位为秒；0 表示不限制。",
      ),
      SSHKeepAlive: seconds(
        "SSH 保活间隔",
        "填写 SSH 保活消息间隔，单位为秒；0 表示停用。",
      ),
      MaxAuthTries: number(
        "认证尝试上限",
        "填写每个连接允许的认证尝试次数；0 使用服务默认值。",
        0,
      ),
      BannerFile: text("登录提示文件", "填写认证前展示的登录提示文件路径。"),
      keyfile: text(
        "主机密钥文件",
        "填写已有 SSH 主机密钥文件路径；列表项保持原样，不是密钥内容。",
      ),
      enable: boolean("启用 SSH 服务", "启用此 Dropbear 服务实例。"),
      mdns: boolean("公布 mDNS 服务", "通过 mDNS 公布此 SSH 服务。"),
    }),
  },
};

function definition(
  module: ConfigurationModule,
  sectionType: string,
): SectionDefinition | undefined {
  const sections = definitions[module];
  return Object.hasOwn(sections, sectionType)
    ? sections[sectionType]
    : undefined;
}

/** Presentation metadata only. Callers preserve existing values, including unknown select tokens, until edited. */
export function fieldSchema(
  module: ConfigurationModule,
  sectionType: string,
  fieldName: string,
): FieldSchema {
  const fields = definition(module, sectionType)?.fields;
  if (fields && Object.hasOwn(fields, fieldName)) {
    const help = fieldHelp(module, sectionType, fieldName);
    return {
      ...fields[fieldName],
      ...(help
        ? { help, ...(help.summary ? { hint: help.summary } : {}) }
        : {}),
    };
  }
  return text(
    fieldName,
    "未登记的原生字段；可直接编辑文本，不推断布尔值或数字类型，未编辑时保留原值。",
  );
}

export function sectionSchema(
  module: ConfigurationModule,
  sectionType: string,
): SectionSchema {
  const known = definition(module, sectionType);
  return known
    ? { label: known.label, hint: known.hint }
    : {
        label: sectionType,
        hint: "未登记的原生章节；字段仍可作为文本编辑，保留厂商配置与原生名称。",
      };
}

/** Canonical case-sensitive UCI keys for the add-field selector; custom keys remain allowed. */
export function sectionFields(
  module: ConfigurationModule,
  sectionType: string,
): readonly string[] {
  const fields = definition(module, sectionType)?.fields;
  return fields ? Object.keys(fields) : [];
}

/** Complete inventory for catalog review and deterministic coverage tests. */
export function fieldInventory(): readonly {
  module: ConfigurationModule;
  section: string;
  field: string;
}[] {
  return (Object.keys(definitions) as ConfigurationModule[]).flatMap((module) =>
    Object.entries(definitions[module]).flatMap(([section, definition]) =>
      Object.keys(definition.fields).map((field) => ({
        module,
        section,
        field,
      })),
    ),
  );
}
