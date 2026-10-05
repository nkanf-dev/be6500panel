//! Typed RN02 factory network controls. No caller-selected Lua route or raw reply.
//!
//! Mapping evidence: factory xqnetwork/misystem index() registrations and the
//! final CamelCase assignments in the 1.0.43 decompile, XQLanWanUtil,
//! XQPortServiceUtil and XQMultiWanPolicy, plus RN02 port_map/ipv6 defaults.
//! Native invocation, device locking, checkpoints and reconnect jobs belong to
//! the shared feature owner. These setters commit/restart internally.
use crate::features::{
    Action, Domain, Error, Field, FieldKind, Impact, Read, choice, field, integer, invalid,
};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeSet,
    net::{Ipv4Addr, Ipv6Addr},
};

const WAN_NAMES: &[&str] = &["WAN1", "WAN2"];
const WAN6_NAMES: &[&str] = &["wan6", "wan6_2"];
// RN02 /etc/config/ipv6 support_modes, not every mode in the shared helper.
const IPV6_MODES: &[&str] = &["off", "native", "dhcpv6", "pppoev6", "static", "pi_relay"];
const PS_SERVICES: &[&str] = &["wandt", "lag", "iptv", "wan", "multiwan", "game", "wantag"];
const WAN: Field = choice("wan_name", "逻辑 WAN", false, WAN_NAMES);
const WAN6: Field = choice("wan6_name", "逻辑 IPv6 WAN", false, WAN6_NAMES);
const PS_READ: &[Field] = &[
    choice("service", "端口服务", true, PS_SERVICES),
    choice("interface", "Internet VLAN 接口", false, &["wan", "wan_2"]),
];
const WAN_FIELDS: &[Field] = &[
    WAN,
    choice(
        "wanType",
        "IPv4 连接方式",
        true,
        &["dhcp", "pppoe", "static"],
    ),
    field("pppoeName", "PPPoE 账号", FieldKind::Secret, false),
    field("pppoePwd", "PPPoE 密码", FieldKind::Secret, false),
    field("staticIp", "静态 IPv4 地址", FieldKind::Ipv4, false),
    field("staticMask", "静态 IPv4 掩码", FieldKind::Ipv4, false),
    field("staticGateway", "静态 IPv4 网关", FieldKind::Ipv4, false),
    field("dns1", "WAN 上游 DNS 1", FieldKind::Ipv4, false),
    field("dns2", "WAN 上游 DNS 2", FieldKind::Ipv4, false),
    choice("autoset", "自动 DNS", false, &["0", "1"]),
    choice("special", "PPPoE 特殊拨号", false, &["0", "1"]),
    integer("mtu", "PPPoE MRU", false, 576, 1492),
    field("service", "PPPoE 服务名", FieldKind::Text, false),
];
const DHCP_FIELDS: &[Field] = &[
    choice("ignore", "禁用 DHCP", true, &["0", "1"]),
    integer("start", "地址池起始主机号", false, 2, 254),
    integer("end", "地址池结束主机号", false, 2, 254),
    field("startip", "地址池起始 IPv4", FieldKind::Ipv4, false),
    field("endip", "地址池结束 IPv4", FieldKind::Ipv4, false),
    field(
        "leasetime",
        "租期（2–2880m / 1–48h）",
        FieldKind::Text,
        false,
    ),
    field("router", "向客户端下发网关", FieldKind::Ipv4, false),
    field("dns1", "向客户端下发 DNS 1", FieldKind::Ipv4, false),
    field("dns2", "向客户端下发 DNS 2", FieldKind::Ipv4, false),
];
const IPV6_FIELDS: &[Field] = &[
    WAN6,
    choice("ipv6_mode", "IPv6 方式", true, IPV6_MODES),
    integer("automode", "自动检测", false, 0, 1),
    field("dns1", "IPv6 上游 DNS 1", FieldKind::Ipv6, false),
    field("dns2", "IPv6 上游 DNS 2", FieldKind::Ipv6, false),
    integer("nat6_enabled", "NAT6", false, 0, 1),
    field("ip6prefix", "LAN IPv6 前缀", FieldKind::Ipv6, false),
    integer("ip6prefixlen", "LAN 前缀长度", false, 1, 128),
    integer("use_pppoev4", "复用 IPv4 PPPoE", false, 0, 1),
    field(
        "ipv6DialAccount",
        "IPv6 PPPoE 账号",
        FieldKind::Secret,
        false,
    ),
    field(
        "ipv6DialPassword",
        "IPv6 PPPoE 密码",
        FieldKind::Secret,
        false,
    ),
    field("ip6addr", "静态 IPv6 地址", FieldKind::Text, false),
    field("ip6gw", "静态 IPv6 网关", FieldKind::Ipv6, false),
];
const PS_MULTI_FIELDS: &[Field] = &[
    choice("service", "端口服务", true, &["multiwan"]),
    integer("enable", "双 WAN", true, 0, 1),
    // These are literal formvalue names in the registered PS analyzer.
    choice(
        "port_map%5B0%5D%5Bname%5D",
        "第一个逻辑出口",
        false,
        &["WAN1"],
    ),
    field(
        "port_map%5B0%5D%5Bport%5D",
        "WAN1 物理口",
        FieldKind::Text,
        false,
    ),
    choice(
        "port_map%5B1%5D%5Bname%5D",
        "第二个逻辑出口",
        false,
        &["WAN2"],
    ),
    field(
        "port_map%5B1%5D%5Bport%5D",
        "WAN2 物理口",
        FieldKind::Text,
        false,
    ),
    integer(
        "policy%5Bmode%5D",
        "出口策略（0均衡/1主备/2备主/3仅WAN1/4仅WAN2）",
        false,
        0,
        4,
    ),
    integer("policy%5Bbandwidth_wan1%5D", "WAN1 带宽", false, 1, 2500),
    integer("policy%5Bbandwidth_wan2%5D", "WAN2 带宽", false, 1, 2500),
];
// A complete port-service recovery scope includes generated network/IPv6 and
// multiWAN's miqos force-disabled setting, not only the visible port config.
const PORT_CONFIGS: &[&str] = &[
    "port_service",
    "port_map",
    "network",
    "dhcp",
    "firewall",
    "ipv6",
    "mwan3",
    "miqos",
];
const WAN_CONFIGS: &[&str] = &[
    "network",
    "ipv6",
    "dhcp",
    "firewall",
    "port_service",
    "port_map",
    "mwan3",
];
const MODE_CONFIGS: &[&str] = &[
    "xiaoqiang",
    "backup",
    "network",
    "wireless",
    "dhcp",
    "firewall",
    "ipv6",
    "port_service",
    "port_map",
    "mwan3",
    "miqos",
    "macfilter",
    "wifiblist",
    "wifiwlist",
];

pub const DOMAIN: Domain = Domain {
    id: "network",
    title: "网络、地址与端口",
    reads: &[
        Read {
            id: "wan_info",
            title: "WAN IPv4 / 上游 DNS",
            controller: "xqnetwork",
            handler: "getWanInfo",
            fields: &[WAN],
        },
        Read {
            id: "wan_status",
            title: "WAN IPv4 / IPv6 运行状态",
            controller: "xqnetwork",
            handler: "getWanStatus",
            fields: &[],
        },
        Read {
            id: "pppoe_status",
            title: "PPPoE 连接状态",
            controller: "xqnetwork",
            handler: "pppoeStatus",
            fields: &[WAN],
        },
        Read {
            id: "wan_speed",
            title: "WAN1 保存的协商速率",
            controller: "xqnetwork",
            handler: "getWanSpeed",
            fields: &[],
        },
        Read {
            id: "wan_link",
            title: "WAN 物理链路",
            controller: "xqnetwork",
            handler: "getWanLinkStatus",
            fields: &[choice("wan_sec", "逻辑 WAN 接口", false, &["wan", "wan_2"])],
        },
        Read {
            id: "lan_info",
            title: "LAN 地址与链路",
            controller: "xqnetwork",
            handler: "getLanInfo",
            fields: &[],
        },
        Read {
            id: "lan_dhcp",
            title: "DHCP 地址池 / 下发 DNS",
            controller: "xqnetwork",
            handler: "getLanDhcp",
            fields: &[],
        },
        Read {
            id: "macbind_info",
            title: "静态地址与 MAC 绑定",
            controller: "xqnetwork",
            handler: "getMacBindInfo",
            fields: &[field("mac", "筛选 MAC", FieldKind::Mac, false)],
        },
        Read {
            id: "ipmac_check",
            title: "IP / MAC 安全绑定",
            controller: "xqnetwork",
            handler: "getIPMACCheckStatus",
            fields: &[],
        },
        Read {
            id: "wan6_switch",
            title: "IPv6 总开关",
            controller: "xqnetwork",
            handler: "getWan6SwitchV2",
            fields: &[],
        },
        Read {
            id: "wan6_config",
            title: "IPv6 WAN v2 配置",
            controller: "xqnetwork",
            handler: "getWan6V2",
            fields: &[WAN6],
        },
        Read {
            id: "wan6_status",
            title: "IPv6 WAN / PD 运行状态",
            controller: "xqnetwork",
            handler: "getWan6InfoV2",
            fields: &[WAN6],
        },
        Read {
            id: "lan6_config",
            title: "IPv6 LAN / RA / DHCPv6",
            controller: "xqnetwork",
            handler: "getLan6V2",
            fields: &[],
        },
        Read {
            id: "ipv6_firewall",
            title: "IPv6 入站防火墙",
            controller: "xqnetwork",
            handler: "getIpv6Firewall",
            fields: &[],
        },
        Read {
            id: "multiwan_info",
            title: "双 WAN 策略与权重",
            controller: "xqnetwork",
            handler: "getMultiwanBasicInfo",
            fields: &[],
        },
        Read {
            id: "multiwan_devices",
            title: "可分配出口的设备",
            controller: "xqnetwork",
            handler: "getMultiwanDevList",
            fields: &[],
        },
        Read {
            id: "multiwan_policies",
            title: "设备固定出口",
            controller: "xqnetwork",
            handler: "getMultiwanDevPolicies",
            fields: &[],
        },
        Read {
            id: "port_map",
            title: "RN02 四口映射",
            controller: "misystem",
            handler: "getPSMap",
            fields: &[],
        },
        Read {
            id: "port_service",
            title: "端口角色 / IPTV / VLAN / LAG",
            controller: "misystem",
            handler: "getPSService",
            fields: PS_READ,
        },
        Read {
            id: "vlan_internet",
            title: "Internet VLAN",
            controller: "misystem",
            handler: "getVlanInternet",
            fields: &[],
        },
        Read {
            id: "vlan_iptv",
            title: "IPTV 与 LAN 口",
            controller: "misystem",
            handler: "getVlanIPTV",
            fields: &[],
        },
        Read {
            id: "mode",
            title: "路由 / 有线 AP / 无线中继模式",
            controller: "xqnetwork",
            handler: "getMode",
            fields: &[],
        },
        Read {
            id: "netmode",
            title: "CAP / RE 网络模式",
            controller: "xqnetwork",
            handler: "getNetMode",
            fields: &[],
        },
        Read {
            id: "bridge_lan",
            title: "AP / 中继上联地址",
            controller: "xqnetwork",
            handler: "getBridgeLanStatus",
            fields: &[],
        },
    ],
    actions: &[
        Action {
            id: "set_wan",
            title: "设置 WAN IPv4 / DNS",
            controller: "xqnetwork",
            handler: "setWan",
            fields: WAN_FIELDS,
            impact: Impact::Network,
            configs: WAN_CONFIGS,
            readback: "wan_info",
        },
        Action {
            id: "pppoe_start",
            title: "连接 PPPoE",
            controller: "xqnetwork",
            handler: "pppoeStart",
            fields: &[WAN],
            impact: Impact::Network,
            configs: &["network"],
            readback: "pppoe_status",
        },
        Action {
            id: "pppoe_stop",
            title: "断开 PPPoE",
            controller: "xqnetwork",
            handler: "pppoeStop",
            fields: &[WAN],
            impact: Impact::Network,
            configs: &["network"],
            readback: "pppoe_status",
        },
        Action {
            id: "wan_up",
            title: "连接 WAN1",
            controller: "xqnetwork",
            handler: "wanUp",
            fields: &[],
            impact: Impact::Network,
            configs: &["network"],
            readback: "pppoe_status",
        },
        Action {
            id: "wan_down",
            title: "断开 WAN1",
            controller: "xqnetwork",
            handler: "wanDown",
            fields: &[],
            impact: Impact::Network,
            configs: &["network"],
            readback: "pppoe_status",
        },
        Action {
            id: "mac_clone",
            title: "设置 WAN MAC",
            controller: "xqnetwork",
            handler: "setWanMac",
            fields: &[WAN, field("mac", "WAN MAC", FieldKind::Mac, true)],
            impact: Impact::Network,
            configs: WAN_CONFIGS,
            readback: "wan_info",
        },
        // The stock getWanSpeed getter reads WAN_SPEED (WAN1) only. WAN2 speed
        // cannot be claimed verified from this getter, so it is not offered.
        Action {
            id: "set_wan_speed",
            title: "设置 WAN1 协商速率",
            controller: "xqnetwork",
            handler: "setWanSpeed",
            fields: &[
                choice("wan_name", "逻辑 WAN", false, &["WAN1"]),
                integer("speed", "速率（0自动/100/1000/2500）", true, 0, 2500),
            ],
            impact: Impact::Network,
            configs: &["xiaoqiang", "port_service", "port_map", "network"],
            readback: "wan_speed",
        },
        Action {
            id: "set_lan_ip",
            title: "更改 LAN 管理地址",
            controller: "xqnetwork",
            handler: "setLanIp",
            fields: &[
                field("ip", "LAN IPv4", FieldKind::Ipv4, true),
                field("mask", "LAN 掩码", FieldKind::Ipv4, true),
            ],
            impact: Impact::Maintenance,
            configs: &["network", "dhcp", "macbind", "firewall", "xiaoqiang"],
            readback: "lan_info",
        },
        Action {
            id: "set_lan_dhcp",
            title: "设置 DHCP / 客户端 DNS",
            controller: "xqnetwork",
            handler: "setLanDhcp",
            fields: DHCP_FIELDS,
            impact: Impact::Network,
            configs: &[
                "dhcp",
                "network",
                "macbind",
                "firewall",
                "port_service",
                "port_map",
            ],
            readback: "lan_dhcp",
        },
        Action {
            id: "mac_bind",
            title: "添加或更新静态地址绑定",
            controller: "xqnetwork",
            handler: "macBind",
            fields: &[field(
                "data",
                "绑定列表（mac/ip/name/instance）",
                FieldKind::Json,
                true,
            )],
            impact: Impact::Network,
            configs: &["macbind", "dhcp", "firewall", "devicelist"],
            readback: "macbind_info",
        },
        Action {
            id: "mac_unbind",
            title: "删除静态地址绑定",
            controller: "xqnetwork",
            handler: "macUnbind",
            fields: &[field("mac", "MAC（多个用逗号分隔）", FieldKind::Text, true)],
            impact: Impact::Network,
            configs: &["macbind", "dhcp", "firewall"],
            readback: "macbind_info",
        },
        Action {
            id: "ipmac_check_enable",
            title: "IP / MAC 安全检查",
            controller: "xqnetwork",
            handler: "setIPMACCheckEnable",
            fields: &[integer("enable", "安全检查", true, 0, 1)],
            impact: Impact::Network,
            configs: &["firewall", "macbind", "dhcp"],
            readback: "ipmac_check",
        },
        Action {
            id: "set_wan6_switch",
            title: "IPv6 总开关",
            controller: "xqnetwork",
            handler: "setWan6SwitchV2",
            fields: &[choice("enabled", "IPv6", true, &["0", "1"])],
            impact: Impact::Network,
            configs: &["ipv6", "network", "dhcp", "firewall"],
            readback: "wan6_switch",
        },
        Action {
            id: "set_wan6",
            title: "设置 IPv6 WAN v2 / DNS",
            controller: "xqnetwork",
            handler: "setWan6V2",
            fields: IPV6_FIELDS,
            impact: Impact::Network,
            configs: &["ipv6", "network", "dhcp", "firewall"],
            readback: "wan6_config",
        },
        Action {
            id: "set_lan6",
            title: "设置 IPv6 LAN",
            controller: "xqnetwork",
            handler: "setLan6V2",
            fields: &[
                integer("mode", "IPv6 LAN 原厂模式（0–3）", true, 0, 3),
                integer("ip6assign", "LAN IPv6 前缀长度", true, 1, 128),
            ],
            impact: Impact::Network,
            configs: &["ipv6", "network", "dhcp"],
            readback: "lan6_config",
        },
        Action {
            id: "set_ipv6_firewall",
            title: "IPv6 入站防火墙",
            controller: "xqnetwork",
            handler: "setIpv6Firewall",
            fields: &[choice("mode", "防火墙", true, &["0", "1"])],
            impact: Impact::Network,
            configs: &["ipv6", "firewall"],
            readback: "ipv6_firewall",
        },
        Action {
            id: "set_multiwan_enable",
            title: "切换双 WAN 策略",
            controller: "xqnetwork",
            handler: "setMultiwanEnable",
            fields: &[choice("enable", "双 WAN", true, &["0", "1"])],
            impact: Impact::Network,
            configs: PORT_CONFIGS,
            readback: "multiwan_info",
        },
        Action {
            id: "set_multiwan_policy",
            title: "双 WAN 出口策略",
            controller: "xqnetwork",
            handler: "setMultiwanPolicy",
            fields: &[integer(
                "policy",
                "策略（0均衡/1主备/2备主/3仅WAN1/4仅WAN2）",
                true,
                0,
                4,
            )],
            impact: Impact::Network,
            configs: PORT_CONFIGS,
            readback: "multiwan_info",
        },
        Action {
            id: "set_multiwan_weight",
            title: "按出口带宽分配权重",
            controller: "xqnetwork",
            handler: "setMultiwanWeight",
            fields: &[
                integer("bandwidth_wan1", "WAN1 带宽", true, 1, 2500),
                integer("bandwidth_wan2", "WAN2 带宽", true, 1, 2500),
            ],
            impact: Impact::Network,
            configs: &["mwan3"],
            readback: "multiwan_info",
        },
        Action {
            id: "set_multiwan_dev_policy",
            title: "设备固定 WAN 出口",
            controller: "xqnetwork",
            handler: "setMultiwanDevPolicy",
            fields: &[
                field("mac", "设备 MAC（多个用逗号分隔）", FieldKind::Text, true),
                choice("wan", "出口", true, WAN_NAMES),
                field("oname", "设备名称", FieldKind::Text, false),
                choice("manual", "手动设备", true, &["0", "1"]),
                choice("opt", "操作（0设置/1删除）", true, &["0", "1"]),
            ],
            impact: Impact::Network,
            configs: &["mwan3"],
            readback: "multiwan_policies",
        },
        Action {
            id: "ps_wan",
            title: "WAN 物理口 / 自动识别 / LAN",
            controller: "misystem",
            handler: "setPSService",
            fields: &[
                choice("service", "端口服务", true, &["wan"]),
                integer("mode", "角色（1固定WAN/2自动/3全LAN）", true, 1, 3),
                field("wan_port", "物理口", FieldKind::Text, false),
            ],
            impact: Impact::Network,
            configs: PORT_CONFIGS,
            readback: "port_service",
        },
        Action {
            id: "ps_wandt",
            title: "自动识别 WAN 口",
            controller: "misystem",
            handler: "setPSService",
            fields: &[
                choice("service", "端口服务", true, &["wandt"]),
                integer("enable", "自动识别", true, 0, 1),
                field("wan_port", "固定 WAN 物理口", FieldKind::Text, false),
            ],
            impact: Impact::Network,
            configs: PORT_CONFIGS,
            readback: "port_service",
        },
        Action {
            id: "ps_multiwan",
            title: "双 WAN 物理口与策略",
            controller: "misystem",
            handler: "setPSService",
            fields: PS_MULTI_FIELDS,
            impact: Impact::Network,
            configs: PORT_CONFIGS,
            readback: "port_service",
        },
        Action {
            id: "ps_lag",
            title: "LAN 链路聚合",
            controller: "misystem",
            handler: "setPSService",
            fields: &[
                choice("service", "端口服务", true, &["lag"]),
                integer("enable", "聚合", true, 0, 1),
                integer("mode", "原厂 LAG 模式", true, 0, 65535),
                field("ports", "两个物理口（空格分隔）", FieldKind::Text, false),
            ],
            impact: Impact::Network,
            configs: PORT_CONFIGS,
            readback: "port_service",
        },
        Action {
            id: "ps_game",
            title: "游戏专用 LAN 口",
            controller: "misystem",
            handler: "setPSService",
            fields: &[
                choice("service", "端口服务", true, &["game"]),
                integer("enable", "游戏口", true, 0, 1),
                field("ports", "物理口", FieldKind::Text, false),
            ],
            impact: Impact::Network,
            configs: PORT_CONFIGS,
            readback: "port_service",
        },
        Action {
            id: "ps_iptv",
            title: "IPTV 口 / VLAN / 802.1p",
            controller: "misystem",
            handler: "setPSService",
            fields: &[
                choice("service", "端口服务", true, &["iptv"]),
                integer("enable", "IPTV", true, 0, 1),
                integer("profile", "IPTV 档案（0自定义/1桥接）", false, 0, 1),
                integer("vid", "VLAN ID（桥接为0）", false, 0, 4094),
                integer("priority", "802.1p", false, 0, 7),
                field("ports", "IPTV 物理口", FieldKind::Text, false),
            ],
            impact: Impact::Network,
            configs: PORT_CONFIGS,
            readback: "port_service",
        },
        Action {
            id: "ps_wantag",
            title: "Internet VLAN / 802.1p",
            controller: "misystem",
            handler: "setPSService",
            fields: &[
                choice("service", "端口服务", true, &["wantag"]),
                choice("interface", "逻辑 WAN 接口", true, &["wan", "wan_2"]),
                integer("profile", "Internet 档案（0禁用/1自定义）", true, 0, 1),
                integer("vid", "Internet VLAN ID", false, 1, 4094),
                integer("priority", "802.1p", false, 0, 7),
            ],
            impact: Impact::Network,
            configs: PORT_CONFIGS,
            readback: "port_service",
        },
        Action {
            id: "set_vlan_internet",
            title: "设置 Internet VLAN",
            controller: "misystem",
            handler: "setVlanService",
            fields: &[
                choice("opt", "操作", true, &["init", "set", "clean"]),
                integer(
                    "internet_profile",
                    "Internet 档案（0禁用/1自定义）",
                    false,
                    0,
                    1,
                ),
                integer("internet_vid", "Internet VLAN ID", false, 1, 4094),
                integer("internet_priority", "802.1p", false, 0, 7),
            ],
            impact: Impact::Network,
            configs: PORT_CONFIGS,
            readback: "vlan_internet",
        },
        // Deliberately omit initialization/admin-password branches. Mode
        // changes are separate reconnect jobs, never normal draft saves.
        Action {
            id: "set_lan_ap",
            title: "切换到有线 AP",
            controller: "xqnetwork",
            handler: "setLanAP",
            fields: &[],
            impact: Impact::Maintenance,
            configs: MODE_CONFIGS,
            readback: "mode",
        },
        Action {
            id: "disable_lan_ap",
            title: "有线 AP 恢复路由模式",
            controller: "xqnetwork",
            handler: "disableLanAP",
            fields: &[],
            impact: Impact::Maintenance,
            configs: MODE_CONFIGS,
            readback: "mode",
        },
        Action {
            id: "set_wifi_ap",
            title: "切换到无线中继",
            controller: "xqnetwork",
            handler: "setWifiApMode",
            fields: &[
                field("ssid", "上联 SSID", FieldKind::Text, true),
                field(
                    "password",
                    "上联密码（开放网络填空字符串）",
                    FieldKind::Secret,
                    false,
                ),
            ],
            impact: Impact::Maintenance,
            configs: MODE_CONFIGS,
            readback: "mode",
        },
        Action {
            id: "disable_wifi_ap",
            title: "无线中继恢复路由模式",
            controller: "xqnetwork",
            handler: "disableap",
            fields: &[],
            impact: Impact::Maintenance,
            configs: MODE_CONFIGS,
            readback: "mode",
        },
    ],
};

fn response_error() -> Error {
    Error {
        status: 502,
        code: "vendor_invalid_response",
        message: "原厂网络服务响应无效。",
    }
}
fn not_found() -> Error {
    Error {
        status: 404,
        code: "feature_read_not_found",
        message: "没有该网络读取功能。",
    }
}
fn object(value: &Value) -> Result<&Map<String, Value>, Error> {
    value.as_object().ok_or_else(response_error)
}
/// Scalars only. A selected key may not smuggle an arbitrary nested response.
fn scalars(value: &Value, keys: &[&str]) -> Result<Value, Error> {
    let source = object(value)?;
    let mut out = Map::new();
    for key in keys {
        if let Some(v) = source.get(*key) {
            match v {
                Value::Null | Value::Bool(_) | Value::Number(_) => {}
                Value::String(s) if s.len() <= 4096 && !s.chars().any(char::is_control) => {}
                _ => return Err(response_error()),
            }
            out.insert((*key).into(), v.clone());
        }
    }
    Ok(Value::Object(out))
}
fn nested(
    out: &mut Value,
    source: &Value,
    key: &str,
    f: impl FnOnce(&Value) -> Result<Value, Error>,
) -> Result<(), Error> {
    if let Some(v) = source.get(key).filter(|v| !v.is_null()) {
        out[key] = f(v)?;
    }
    Ok(())
}
fn records(value: &Value, keys: &[&str]) -> Result<Value, Error> {
    if value.as_object().is_some_and(|m| m.is_empty()) {
        return Ok(json!([]));
    }
    let rows = value.as_array().ok_or_else(response_error)?;
    if rows.len() > 512 {
        return Err(response_error());
    }
    rows.iter()
        .map(|row| scalars(row, keys))
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}
/// Lua sparse numeric tables can encode as JSON arrays or index-keyed maps.
fn indexed(value: &Value, keys: &[&str]) -> Result<Value, Error> {
    if value.is_array() {
        return records(value, keys);
    }
    let map = object(value)?;
    if map.len() > 16 {
        return Err(response_error());
    }
    let mut out = Map::new();
    for (key, v) in map {
        if key.len() > 2 || key.parse::<u8>().ok().is_none_or(|n| n > 16) {
            return Err(response_error());
        }
        out.insert(key.clone(), scalars(v, keys)?);
    }
    Ok(Value::Object(out))
}
fn strings(value: &Value, depth: usize) -> Result<Value, Error> {
    match value {
        Value::Object(m) if m.is_empty() => Ok(json!([])),
        Value::Null => Ok(Value::Null),
        Value::String(s) if s.len() <= 4096 && !s.chars().any(char::is_control) => {
            Ok(value.clone())
        }
        Value::Array(rows) if rows.len() <= 64 && depth < 2 => rows
            .iter()
            .map(|v| strings(v, depth + 1))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        _ => Err(response_error()),
    }
}
fn secret_flags(out: &mut Value, source: &Value, keys: &[&str]) {
    for key in keys {
        if let Some(v) = source.get(*key) {
            out[format!("{key}Configured")] = json!(v.as_str().is_some_and(|s| !s.is_empty()));
        }
    }
}
fn v6_state(value: &Value) -> Result<Value, Error> {
    let mut out = scalars(
        value,
        &["up", "ipv6_mode", "wanType", "ip6gw", "ifname", "peerdns"],
    )?;
    for key in ["dns", "ip6addr", "lan_ip6addr", "lan_ip6prefix"] {
        nested(&mut out, value, key, |v| strings(v, 0))?;
    }
    Ok(out)
}
fn ppp_state(value: &Value) -> Result<Value, Error> {
    // errcode can be a vendor error message; only numeric error codes are public.
    let mut out = scalars(
        value,
        &[
            "code", "proto", "wanType", "status", "errtype", "perror", "peerdns", "gw", "wanSpeed",
            "uptime",
        ],
    )?;
    if let Some(v) = value.get("errcode")
        && (v.is_number()
            || v.as_str()
                .is_some_and(|s| s.len() <= 16 && s.bytes().all(|c| c.is_ascii_digit())))
    {
        out["errcode"] = v.clone();
    }
    nested(&mut out, value, "ip", |v| scalars(v, &["address", "mask"]))?;
    for key in ["dns", "cdns"] {
        nested(&mut out, value, key, |v| strings(v, 0))?;
    }
    secret_flags(&mut out, value, &["pppoename", "password"]);
    Ok(out)
}
fn port_service(value: &Value) -> Result<Value, Error> {
    object(value)?;
    let mut out = json!({});
    for service in PS_SERVICES {
        if let Some(v) = value.get(*service) {
            let mut cfg = match *service {
                "wandt" => scalars(v, &["enable", "wan_port", "index"]),
                "wan" => scalars(v, &["mode", "wan_port", "wan_label"]),
                "game" => scalars(v, &["enable", "ports"]),
                "lag" => scalars(v, &["enable", "ports", "mode", "status"]),
                "iptv" => scalars(
                    v,
                    &[
                        "enable",
                        "ports",
                        "profile",
                        "vid",
                        "priority",
                        "forbid_vid",
                        "permit_vid",
                    ],
                ),
                "wantag" => scalars(
                    v,
                    &[
                        "interface",
                        "profile",
                        "vid",
                        "priority",
                        "forbid_vid",
                        "permit_vid",
                    ],
                ),
                "multiwan" => scalars(v, &["enable"]),
                _ => unreachable!(),
            }?;
            if *service == "lag" {
                nested(&mut cfg, v, "info", |v| {
                    records(v, &["port", "link", "speed"])
                })?;
            }
            if *service == "multiwan" {
                nested(&mut cfg, v, "port_map", |v| records(v, &["name", "port"]))?;
                nested(&mut cfg, v, "policy", |v| {
                    scalars(
                        v,
                        &[
                            "mode",
                            "currwan",
                            "weight1",
                            "weight2",
                            "bandwidth_wan1",
                            "bandwidth_wan2",
                        ],
                    )
                })?;
            }
            out[*service] = cfg;
        }
    }
    Ok(out)
}
/// Strict public DTOs. Unknown keys are discarded at every nested boundary;
/// native credentials/messages are never returned, even under selected keys.
pub fn project(read_id: &str, value: Value) -> Result<Value, Error> {
    if !DOMAIN.reads.iter().any(|r| r.id == read_id) {
        return Err(not_found());
    }
    if serde_json::to_vec(&value)
        .map_err(|_| response_error())?
        .len()
        > 256 << 10
    {
        return Err(response_error());
    }
    object(&value)?;
    let out = match read_id {
        "wan_info" => {
            let mut out = json!({});
            nested(&mut out, &value, "info", |v| {
                let mut info = scalars(
                    v,
                    &[
                        "link",
                        "mtu",
                        "special",
                        "mac",
                        "gateWay",
                        "dnsAddrs",
                        "dnsAddrs1",
                        "uptime",
                        "status",
                        "ipv6_show",
                    ],
                )?;
                nested(&mut info, v, "ipv4", |v| records(v, &["ip", "mask"]))?;
                nested(&mut info, v, "details", |v| {
                    let mut details = scalars(
                        v,
                        &[
                            "wanType",
                            "baseWanType",
                            "ifname",
                            "mtu",
                            "special",
                            "ipaddr",
                            "netmask",
                            "gateway",
                            "peerdns",
                            "service",
                        ],
                    )?;
                    nested(&mut details, v, "dns", |v| strings(v, 0))?;
                    secret_flags(&mut details, v, &["username", "password"]);
                    Ok(details)
                })?;
                nested(&mut info, v, "ipv6_info", v6_state)?;
                Ok(info)
            })?;
            out
        }
        "wan_status" => {
            let mut out = json!({});
            nested(&mut out, &value, "ipv4", ppp_state)?;
            nested(&mut out, &value, "ipv6", |v| {
                let mut sub = json!({});
                nested(&mut sub, v, "wan6_info", v6_state)?;
                Ok(sub)
            })?;
            out
        }
        "pppoe_status" => ppp_state(&value)?,
        "wan_speed" => scalars(&value, &["speed"])?,
        "wan_link" => scalars(&value, &["link"])?,
        "lan_info" => {
            let mut out = json!({});
            nested(&mut out, &value, "info", |v| {
                let mut info = scalars(v, &["uptime", "status", "mac"])?;
                nested(&mut info, v, "ipv4", |v| records(v, &["ip", "mask"]))?;
                Ok(info)
            })?;
            if let Some(v) = value.get("linkList") {
                match v {
                    Value::Array(a)
                        if a.len() <= 16
                            && a.iter().all(|v| {
                                v.is_null() || v.as_i64().is_some_and(|n| n == 0 || n == 1)
                            }) =>
                    {
                        out["linkList"] = v.clone()
                    }
                    Value::Object(m)
                        if m.len() <= 16
                            && m.iter().all(|(k, v)| {
                                k.parse::<u8>().ok().is_some_and(|n| n <= 16)
                                    && v.as_i64().is_some_and(|n| n == 0 || n == 1)
                            }) =>
                    {
                        out["linkList"] = v.clone()
                    }
                    _ => return Err(response_error()),
                }
            }
            out
        }
        "lan_dhcp" => {
            let mut out = json!({});
            nested(&mut out, &value, "info", |v| {
                let mut dto = scalars(
                    v,
                    &[
                        "start",
                        "limit",
                        "startip",
                        "endip",
                        "leasetime",
                        "leasetimeNum",
                        "leasetimeUnit",
                        "ignore",
                        "router",
                        "dns1",
                        "dns2",
                    ],
                )?;
                nested(&mut dto, v, "lanIp", |value| {
                    records(value, &["ip", "mask"])
                })?;
                Ok(dto)
            })?;
            out
        }
        "macbind_info" => {
            let mut out = scalars(&value, &["lanmask"])?;
            nested(&mut out, &value, "list", |v| {
                records(v, &["mac", "ip", "name", "tag", "instance"])
            })?;
            nested(&mut out, &value, "devicelist", |v| {
                records(
                    v,
                    &[
                        "mac", "ip", "name", "oname", "port", "isap", "tag", "online",
                    ],
                )
            })?;
            out
        }
        "ipmac_check" => scalars(&value, &["enable"])?,
        "wan6_switch" => scalars(&value, &["enabled"])?,
        "wan6_config" => {
            let mut out = json!({});
            nested(&mut out, &value, "wan6_cfg", |v| {
                let mut cfg = scalars(
                    v,
                    &[
                        "ipv6_mode",
                        "peerdns",
                        "nat6_enabled",
                        "use_pppoev4",
                        "ip6prefix",
                        "ip6prefixlen",
                        "ip6addr",
                        "ip6gw",
                    ],
                )?;
                nested(&mut cfg, v, "dns", |v| strings(v, 0))?;
                secret_flags(&mut cfg, v, &["username", "password"]);
                Ok(cfg)
            })?;
            out
        }
        "wan6_status" => {
            let mut out = json!({});
            nested(&mut out, &value, "wan6_info", v6_state)?;
            out
        }
        "lan6_config" => {
            let mut out = json!({});
            nested(&mut out, &value, "lan6_cfg", |v| {
                scalars(v, &["mode", "ip6assign"])
            })?;
            out
        }
        // This unusual stock getter returns the firewall setting in code,
        // normally a UCI string. It is not a success/error wrapper.
        "ipv6_firewall" => {
            let mode = value
                .get("code")
                .and_then(number)
                .filter(|n| *n == 0 || *n == 1)
                .ok_or_else(response_error)?;
            json!({"mode":mode})
        }
        "multiwan_info" => {
            let mut out = json!({});
            nested(&mut out, &value, "info", |v| {
                scalars(
                    v,
                    &[
                        "enable",
                        "policy",
                        "currwan",
                        "weight1",
                        "weight2",
                        "bandwidth_wan1",
                        "bandwidth_wan2",
                    ],
                )
            })?;
            out
        }
        "multiwan_devices" => {
            let mut out = json!({});
            nested(&mut out, &value, "info", |v| {
                records(v, &["mac", "ip", "oname"])
            })?;
            out
        }
        "multiwan_policies" => {
            let mut out = json!({});
            nested(&mut out, &value, "info", |v| {
                let mut info = json!({});
                nested(&mut info, v, "dev_policies", |v| {
                    records(v, &["mac", "oname", "manual", "wan"])
                })?;
                Ok(info)
            })?;
            out
        }
        "port_map" => {
            let mut out = scalars(&value, &["description"])?;
            nested(&mut out, &value, "ports", |v| {
                indexed(v, &["port", "index", "speed", "service", "label"])
            })?;
            out
        }
        "port_service" => port_service(&value)?,
        "vlan_internet" => scalars(
            &value,
            &[
                "internet_tag",
                "internet_vid",
                "internet_profile",
                "internet_priority",
            ],
        )?,
        "vlan_iptv" => {
            let mut out = json!({});
            nested(&mut out, &value, "IPTV", |v| {
                scalars(
                    v,
                    &[
                        "enable",
                        "profile",
                        "vid",
                        "priority",
                        "wan_egress_tag",
                        "lan_egress_tag",
                    ],
                )
            })?;
            nested(&mut out, &value, "interfaces", |v| {
                records(v, &["name", "type"])
            })?;
            out
        }
        "mode" => scalars(&value, &["mode", "hostip", "hostname", "ssid"])?,
        "netmode" => scalars(&value, &["netmode"])?,
        "bridge_lan" => {
            let mut out = json!({});
            nested(&mut out, &value, "ipv4", ppp_state)?;
            nested(&mut out, &value, "ipv6", |v| {
                let mut state = scalars(v, &["ip", "gw"])?;
                nested(&mut state, v, "dns", |v| strings(v, 0))?;
                Ok(state)
            })?;
            out
        }
        _ => return Err(not_found()),
    };
    Ok(out)
}

fn number(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| {
        value
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit()))
            .and_then(|s| s.parse().ok())
    })
}
fn text<'a>(input: &'a Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(Value::as_str)
}
fn present(input: &Value, key: &str) -> bool {
    input
        .get(key)
        .is_some_and(|v| !v.is_null() && v.as_str() != Some(""))
}
fn n(input: &Value, key: &str) -> Option<i64> {
    input.get(key).and_then(number)
}
fn require(input: &Value, keys: &[&str]) -> Result<(), Error> {
    if keys.iter().all(|k| present(input, k)) {
        Ok(())
    } else {
        Err(invalid())
    }
}
fn ipv4(s: &str) -> Option<Ipv4Addr> {
    s.parse::<Ipv4Addr>().ok().filter(|ip| ip.to_string() == s)
}
fn unicast4(s: &str) -> Option<u32> {
    ipv4(s)
        .filter(|ip| {
            let a = ip.octets()[0];
            a != 0 && a != 127 && a < 224
        })
        .map(u32::from)
}
fn ipv6(s: &str) -> Option<Ipv6Addr> {
    s.parse::<Ipv6Addr>()
        .ok()
        .filter(|ip| !ip.is_unspecified() && !ip.is_multicast())
}
fn v6_cidr(s: &str) -> bool {
    if let Some((ip, prefix)) = s.split_once('/') {
        ipv6(ip).is_some() && canonical_unsigned(prefix).is_some_and(|n| n <= 128)
    } else {
        ipv6(s).is_some()
    }
}
fn canonical_unsigned(s: &str) -> Option<u64> {
    s.parse::<u64>().ok().filter(|n| n.to_string() == s)
}
fn mask(s: &str) -> Option<u32> {
    let m = u32::from(ipv4(s)?);
    let inverse = !m;
    (m != 0 && m.count_ones() <= 30 && inverse & inverse.wrapping_add(1) == 0).then_some(m)
}
fn host_in_subnet(ip: u32, m: u32) -> bool {
    let host = ip & !m;
    host != 0 && host != !m
}
fn mac(s: &str) -> bool {
    let parts = s.split(':').collect::<Vec<_>>();
    if parts.len() != 6
        || parts
            .iter()
            .any(|s| s.len() != 2 || !s.bytes().all(|c| c.is_ascii_hexdigit()))
    {
        return false;
    }
    let first = u8::from_str_radix(parts[0], 16).unwrap_or(1);
    first & 1 == 0 && !parts.iter().all(|s| *s == "00")
}
fn mac_list(s: &str) -> Option<Vec<String>> {
    if s.is_empty() || s.len() > 64 * 18 {
        return None;
    }
    let rows = s.split(',').collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    if rows.len() > 64
        || rows
            .iter()
            .any(|s| !mac(s) || !seen.insert(s.to_ascii_lowercase()))
    {
        return None;
    }
    Some(rows.iter().map(|s| s.to_ascii_lowercase()).collect())
}
fn ports(s: &str, count: usize) -> bool {
    let rows = s.split(' ').collect::<Vec<_>>();
    let mut seen = BTreeSet::new();
    rows.len() == count
        && rows
            .iter()
            .all(|p| matches!(*p, "1" | "2" | "3" | "4") && seen.insert(*p))
}
fn optional_ips(input: &Value, keys: &[&str], v6: bool) -> bool {
    keys.iter().all(|key| {
        !present(input, key)
            || text(input, key).is_some_and(|s| {
                if v6 {
                    ipv6(s).is_some()
                } else {
                    unicast4(s).is_some()
                }
            })
    })
}
fn lease(s: &str) -> bool {
    if s.len() < 2 {
        return false;
    }
    let (digits, unit) = s.split_at(s.len() - 1);
    canonical_unsigned(digits).is_some_and(|n| match unit {
        "h" => (1..=48).contains(&n),
        "m" => (2..=2880).contains(&n),
        _ => false,
    })
}
fn credentials(input: &Value, keys: &[&str]) -> bool {
    keys.iter().all(|key| {
        !present(input, key)
            || text(input, key).is_some_and(|s| s.len() <= 256 && !s.chars().any(char::is_control))
    })
}
fn prefix(input: &Value) -> bool {
    let Some(ip) = text(input, "ip6prefix").and_then(ipv6) else {
        return false;
    };
    let len = n(input, "ip6prefixlen").unwrap_or(64);
    (1..=128).contains(&len) && (len == 128 || u128::from(ip) & ((1u128 << (128 - len)) - 1) == 0)
}
/// Canonical schema and cross-field checks run before any factory setter.
/// Runtime LAN containment, physical-port conflicts and supported mode checks
/// remain enforced by the factory owner, using current device configuration.
pub fn validate(action_id: &str, input: &Value) -> Result<(), Error> {
    let action = DOMAIN
        .actions
        .iter()
        .find(|a| a.id == action_id)
        .ok_or_else(invalid)?;
    let map = input.as_object().ok_or_else(invalid)?;
    if serde_json::to_vec(input).map_err(|_| invalid())?.len() > 64 << 10 {
        return Err(invalid());
    }
    for (key, value) in map {
        let f = action
            .fields
            .iter()
            .find(|f| f.key == key)
            .ok_or_else(invalid)?;
        let empty = value.as_str() == Some("");
        if empty && !f.required {
            continue;
        }
        let valid = match f.kind {
            FieldKind::Text | FieldKind::Secret => value
                .as_str()
                .is_some_and(|s| s.len() <= 4096 && !s.chars().any(char::is_control)),
            FieldKind::Select => value.as_str().is_some_and(|s| f.options.contains(&s)),
            FieldKind::Integer => value
                .as_i64()
                .is_some_and(|n| f.min.is_none_or(|m| n >= m) && f.max.is_none_or(|m| n <= m)),
            FieldKind::Ipv4 => value.as_str().is_some_and(|s| ipv4(s).is_some()),
            FieldKind::Ipv6 => value
                .as_str()
                .is_some_and(|s| s.parse::<Ipv6Addr>().is_ok()),
            FieldKind::Mac => value.as_str().is_some_and(mac),
            FieldKind::Json => value.is_array() || value.is_object(),
            FieldKind::Boolean => value.is_boolean(),
        };
        if !valid {
            return Err(invalid());
        }
    }
    for f in action.fields.iter().filter(|f| f.required) {
        require(input, &[f.key])?;
    }
    if !credentials(
        input,
        &[
            "pppoeName",
            "pppoePwd",
            "ipv6DialAccount",
            "ipv6DialPassword",
            "service",
            "password",
            "oname",
        ],
    ) {
        return Err(invalid());
    }
    if action_id == "set_wan6"
        && ["ipv6DialAccount", "ipv6DialPassword"].iter().any(|k| {
            text(input, k).is_some_and(|s| s.contains(['\\', '`', '"', '$', '&', '|', ';']))
        })
    {
        return Err(invalid());
    }
    let valid = match action_id {
        "set_wan" => {
            if !optional_ips(input, &["dns1", "dns2"], false) {
                return Err(invalid());
            }
            if text(input, "autoset") == Some("1")
                && (present(input, "dns1") || present(input, "dns2"))
            {
                return Err(invalid());
            }
            match text(input, "wanType") {
                Some("pppoe") => !["staticIp", "staticMask", "staticGateway"]
                    .iter()
                    .any(|k| present(input, k)),
                Some("static") => {
                    require(input, &["staticIp", "staticMask", "staticGateway"])?;
                    let ip = text(input, "staticIp")
                        .and_then(unicast4)
                        .ok_or_else(invalid)?;
                    let gw = text(input, "staticGateway")
                        .and_then(unicast4)
                        .ok_or_else(invalid)?;
                    let m = text(input, "staticMask")
                        .and_then(mask)
                        .ok_or_else(invalid)?;
                    host_in_subnet(ip, m)
                        && host_in_subnet(gw, m)
                        && ip != gw
                        && ip & m == gw & m
                        && (present(input, "dns1") || present(input, "dns2"))
                        && !["pppoeName", "pppoePwd", "mtu", "service", "special"]
                            .iter()
                            .any(|k| present(input, k))
                }
                Some("dhcp") => ![
                    "pppoeName",
                    "pppoePwd",
                    "staticIp",
                    "staticMask",
                    "staticGateway",
                    "mtu",
                    "service",
                    "special",
                ]
                .iter()
                .any(|k| present(input, k)),
                _ => false,
            }
        }
        "set_wan_speed" => n(input, "speed").is_some_and(|n| matches!(n, 0 | 100 | 1000 | 2500)),
        "set_lan_ip" => {
            let ip = text(input, "ip").and_then(unicast4).ok_or_else(invalid)?;
            let m = text(input, "mask").and_then(mask).ok_or_else(invalid)?;
            host_in_subnet(ip, m)
        }
        "set_lan_dhcp" => {
            if !optional_ips(
                input,
                &["startip", "endip", "router", "dns1", "dns2"],
                false,
            ) {
                return Err(invalid());
            }
            if text(input, "ignore") == Some("1") {
                true
            } else {
                if !text(input, "leasetime").is_some_and(lease) {
                    return Err(invalid());
                }
                let offsets = present(input, "start") || present(input, "end");
                let full = present(input, "startip") || present(input, "endip");
                if offsets == full {
                    false
                } else if offsets {
                    n(input, "start")
                        .zip(n(input, "end"))
                        .is_some_and(|(a, b)| a <= b)
                } else {
                    text(input, "startip")
                        .and_then(unicast4)
                        .zip(text(input, "endip").and_then(unicast4))
                        .is_some_and(|(a, b)| a <= b)
                }
            }
        }
        "mac_bind" => {
            let rows = input
                .get("data")
                .and_then(Value::as_array)
                .ok_or_else(invalid)?;
            if rows.is_empty() || rows.len() > 128 {
                return Err(invalid());
            }
            let mut macs = BTreeSet::new();
            let mut ips = BTreeSet::new();
            rows.iter().all(|row| {
                let Some(map) = row.as_object() else {
                    return false;
                };
                map.keys()
                    .all(|k| ["mac", "ip", "name", "instance"].contains(&k.as_str()))
                    && text(row, "mac")
                        .is_some_and(|s| mac(s) && macs.insert(s.to_ascii_lowercase()))
                    && text(row, "ip")
                        .and_then(unicast4)
                        .is_some_and(|ip| ips.insert(ip))
                    && row.get("name").is_some_and(|v| {
                        v.as_str().is_some_and(|s| {
                            s.len() <= 64
                                && !s.chars().any(char::is_control)
                                && !s.contains(['\'', '"', '\\'])
                        })
                    })
                    && row
                        .get("instance")
                        .is_none_or(|v| v.as_i64().is_some_and(|n| (0..=65535).contains(&n)))
            })
        }
        "mac_unbind" => text(input, "mac").and_then(mac_list).is_some(),
        "set_multiwan_dev_policy" => {
            let macs = text(input, "mac").and_then(mac_list).ok_or_else(invalid)?;
            text(input, "manual") != Some("1")
                || (macs.len() == 1 && (text(input, "opt") == Some("1") || present(input, "oname")))
        }
        "set_wan6" => {
            if !optional_ips(input, &["dns1", "dns2", "ip6gw"], true) {
                return Err(invalid());
            }
            if present(input, "ip6addr") && !text(input, "ip6addr").is_some_and(v6_cidr) {
                return Err(invalid());
            }
            if present(input, "ip6prefix") && !prefix(input) {
                return Err(invalid());
            }
            match text(input, "ipv6_mode") {
                Some("static") => {
                    require(input, &["ip6addr", "ip6gw", "ip6prefix"])?;
                    prefix(input)
                        && ![
                            "nat6_enabled",
                            "use_pppoev4",
                            "ipv6DialAccount",
                            "ipv6DialPassword",
                        ]
                        .iter()
                        .any(|k| present(input, k))
                }
                Some("dhcpv6") => {
                    (n(input, "nat6_enabled").unwrap_or(0) == 0 || prefix(input))
                        && ![
                            "use_pppoev4",
                            "ipv6DialAccount",
                            "ipv6DialPassword",
                            "ip6addr",
                            "ip6gw",
                        ]
                        .iter()
                        .any(|k| present(input, k))
                }
                Some("pppoev6") => {
                    (n(input, "nat6_enabled").unwrap_or(0) == 0 || prefix(input))
                        && !["ip6addr", "ip6gw"].iter().any(|k| present(input, k))
                        && (n(input, "use_pppoev4").unwrap_or(1) == 0
                            || !["ipv6DialAccount", "ipv6DialPassword"]
                                .iter()
                                .any(|k| present(input, k)))
                }
                Some("off" | "native" | "pi_relay") => ![
                    "nat6_enabled",
                    "use_pppoev4",
                    "ipv6DialAccount",
                    "ipv6DialPassword",
                    "ip6addr",
                    "ip6gw",
                    "ip6prefix",
                    "ip6prefixlen",
                ]
                .iter()
                .any(|k| present(input, k)),
                _ => false,
            }
        }
        "ps_wan" => {
            n(input, "mode") != Some(1) || text(input, "wan_port").is_some_and(|s| ports(s, 1))
        }
        "ps_wandt" => {
            n(input, "enable") == Some(1) || text(input, "wan_port").is_some_and(|s| ports(s, 1))
        }
        "ps_lag" => {
            n(input, "enable") == Some(0) || text(input, "ports").is_some_and(|s| ports(s, 2))
        }
        "ps_game" => {
            n(input, "enable") == Some(0) || text(input, "ports").is_some_and(|s| ports(s, 1))
        }
        "ps_iptv" => {
            if n(input, "enable") == Some(0) {
                true
            } else {
                require(input, &["profile", "vid", "priority", "ports"])?;
                text(input, "ports").is_some_and(|s| ports(s, 1))
                    && match n(input, "profile") {
                        Some(0) => n(input, "vid").is_some_and(|n| n >= 1),
                        Some(1) => n(input, "vid") == Some(0) && n(input, "priority") == Some(0),
                        _ => false,
                    }
            }
        }
        "ps_wantag" => {
            if n(input, "profile") == Some(0) {
                true
            } else {
                require(input, &["vid", "priority"])?;
                true
            }
        }
        "ps_multiwan" => {
            if n(input, "enable") == Some(0) {
                true
            } else {
                require(
                    input,
                    &[
                        "port_map%5B0%5D%5Bname%5D",
                        "port_map%5B0%5D%5Bport%5D",
                        "port_map%5B1%5D%5Bname%5D",
                        "port_map%5B1%5D%5Bport%5D",
                    ],
                )?;
                let a = text(input, "port_map%5B0%5D%5Bport%5D").unwrap_or("");
                let b = text(input, "port_map%5B1%5D%5Bport%5D").unwrap_or("");
                if n(input, "policy%5Bmode%5D") == Some(0) {
                    require(
                        input,
                        &["policy%5Bbandwidth_wan1%5D", "policy%5Bbandwidth_wan2%5D"],
                    )?;
                }
                ports(a, 1) && ports(b, 1) && a != b
            }
        }
        "set_vlan_internet" => {
            if text(input, "opt") == Some("clean") {
                true
            } else {
                require(input, &["internet_profile"])?;
                if n(input, "internet_profile") != Some(0) {
                    require(input, &["internet_vid", "internet_priority"])?;
                }
                true
            }
        }
        "set_wifi_ap" => {
            text(input, "ssid")
                .is_some_and(|s| !s.is_empty() && s.len() <= 32 && !s.contains(['"', '\\']))
                && text(input, "password").is_some_and(|s| {
                    s.is_empty() || ((8..=63).contains(&s.len()) && !s.contains(['"', '\\']))
                })
        }
        _ => true,
    };
    if valid { Ok(()) } else { Err(invalid()) }
}

/// Resolve keep-existing credentials from the PRIVATE exact getter response.
/// Empty secret controls mean keep; public *Configured flags are not values.
/// Never log or return this result to the browser. Input stays on Vendor stdin.
pub fn prepare_input(action_id: &str, mut input: Value, current: &Value) -> Result<Value, Error> {
    if !input.is_object() {
        return Err(invalid());
    }
    let pairs: &[(&str, &str)] = match action_id {
        "set_wan" if text(&input, "wanType") == Some("pppoe") => &[
            ("pppoeName", "/info/details/username"),
            ("pppoePwd", "/info/details/password"),
        ],
        "set_wan6"
            if text(&input, "ipv6_mode") == Some("pppoev6")
                && n(&input, "use_pppoev4").unwrap_or(1) == 0 =>
        {
            &[
                ("ipv6DialAccount", "/wan6_cfg/username"),
                ("ipv6DialPassword", "/wan6_cfg/password"),
            ]
        }
        _ => &[],
    };
    for (form, path) in pairs {
        if !present(&input, form) {
            let value = current
                .pointer(path)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or_else(invalid)?;
            input[*form] = json!(value);
        }
    }
    if action_id == "set_wan6" && !present(&input, "automode") {
        // Native setter defaults automode=1, which rewrites a requested manual
        // mode to native on subsequent getWan6Cfg. Submit explicit manual 0.
        input["automode"] = json!(0);
    }
    validate(action_id, &input)?;
    Ok(input)
}

fn same(actual: Option<&Value>, expected: Option<&Value>) -> bool {
    match (actual, expected) {
        (Some(a), Some(b)) if !a.is_null() && !b.is_null() => {
            a == b || number(a).zip(number(b)).is_some_and(|(a, b)| a == b)
        }
        _ => false,
    }
}
fn matching(after: &Value, input: &Value, pairs: &[(&str, &str)]) -> bool {
    pairs
        .iter()
        .all(|(a, b)| !present(input, b) || same(after.get(*a), input.get(*b)))
}
fn dns_matches(actual: Option<&Value>, input: &Value) -> bool {
    let expected = ["dns1", "dns2"]
        .iter()
        .filter_map(|k| text(input, k))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let mut got = Vec::new();
    if let Some(v) = actual {
        if let Some(s) = v.as_str() {
            got.extend(s.split_whitespace());
        } else if let Some(a) = v.as_array() {
            for v in a {
                let Some(s) = v.as_str() else { return false };
                got.push(s);
            }
        }
    }
    got == expected
}
fn same_mac(a: Option<&Value>, b: Option<&Value>) -> bool {
    a.and_then(Value::as_str)
        .zip(b.and_then(Value::as_str))
        .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
}
/// Match an expected change against the exact readback DTO, not code=0.
/// LAN IP and mode transitions deliberately remain pending: their old endpoint
/// does not prove new-address management health or completed reboot/reconnect.
pub fn verify(action_id: &str, input: &Value, after: &Value) -> bool {
    if validate(action_id, input).is_err() || !after.is_object() {
        return false;
    }
    match action_id {
        "set_lan_ip" | "set_lan_ap" | "disable_lan_ap" | "set_wifi_ap" | "disable_wifi_ap" => false,
        "set_wan" => {
            let Some(info) = after.get("info") else {
                return false;
            };
            let Some(d) = info.get("details") else {
                return false;
            };
            if !same(d.get("wanType"), input.get("wanType")) || n(info, "status") != Some(1) {
                return false;
            }
            match text(input, "wanType") {
                Some("dhcp") => dns_matches(d.get("dns"), input),
                Some("static") => {
                    matching(
                        d,
                        input,
                        &[
                            ("ipaddr", "staticIp"),
                            ("netmask", "staticMask"),
                            ("gateway", "staticGateway"),
                        ],
                    ) && dns_matches(d.get("dns"), input)
                        && info.get("ipv4").and_then(Value::as_array).is_some_and(|a| {
                            a.iter().any(|v| {
                                same(v.get("ip"), input.get("staticIp"))
                                    && same(v.get("mask"), input.get("staticMask"))
                            })
                        })
                }
                Some("pppoe") => {
                    d.get("usernameConfigured") == Some(&Value::Bool(true))
                        && d.get("passwordConfigured") == Some(&Value::Bool(true))
                        && matching(
                            d,
                            input,
                            &[
                                ("mtu", "mtu"),
                                ("service", "service"),
                                ("special", "special"),
                            ],
                        )
                        && dns_matches(d.get("dns"), input)
                }
                _ => false,
            }
        }
        "pppoe_start" => text(after, "proto") == Some("pppoe") && n(after, "status") == Some(2),
        "pppoe_stop" => text(after, "proto") == Some("pppoe") && n(after, "status") == Some(4),
        "wan_up" => n(after, "status") == Some(2),
        "wan_down" => n(after, "status").is_some_and(|n| n == 0 || n == 4),
        "mac_clone" => after
            .get("info")
            .is_some_and(|v| same_mac(v.get("mac"), input.get("mac"))),
        "set_wan_speed" => same(after.get("speed"), input.get("speed")),
        "set_lan_dhcp" => {
            let Some(info) = after.get("info") else {
                return false;
            };
            if !same(info.get("ignore"), input.get("ignore")) {
                return false;
            }
            if text(input, "ignore") == Some("1") {
                return true;
            }
            matching(
                info,
                input,
                &[
                    ("startip", "startip"),
                    ("endip", "endip"),
                    ("leasetime", "leasetime"),
                    ("router", "router"),
                    ("dns1", "dns1"),
                    ("dns2", "dns2"),
                ],
            ) && (!present(input, "start")
                || (same(info.get("start"), input.get("start"))
                    && n(info, "limit")
                        == n(input, "end")
                            .zip(n(input, "start"))
                            .map(|(a, b)| a - b + 1)))
        }
        "mac_bind" => {
            let Some(rows) = after.get("list").and_then(Value::as_array) else {
                return false;
            };
            input["data"].as_array().is_some_and(|a| {
                a.iter().all(|want| {
                    rows.iter().any(|got| {
                        same_mac(got.get("mac"), want.get("mac"))
                            && same(got.get("ip"), want.get("ip"))
                            && matching(got, want, &[("name", "name"), ("instance", "instance")])
                    })
                })
            })
        }
        "mac_unbind" => {
            let Some(rows) = after.get("list").and_then(Value::as_array) else {
                return false;
            };
            let Some(want) = text(input, "mac").and_then(mac_list) else {
                return false;
            };
            want.iter().all(|mac| {
                rows.iter()
                    .all(|row| text(row, "mac").is_some_and(|s| !s.eq_ignore_ascii_case(mac)))
            })
        }
        "ipmac_check_enable" => same(after.get("enable"), input.get("enable")),
        "set_wan6_switch" => same(after.get("enabled"), input.get("enabled")),
        "set_ipv6_firewall" => same(after.get("mode"), input.get("mode")),
        "set_lan6" => after.get("lan6_cfg").is_some_and(|v| {
            same(v.get("mode"), input.get("mode"))
                && same(v.get("ip6assign"), input.get("ip6assign"))
        }),
        "set_wan6" => {
            // getWan6Cfg does not expose automode. Non-native auto detection
            // cannot be proved by this single configuration getter.
            let Some(cfg) = after.get("wan6_cfg") else {
                return false;
            };
            if n(input, "automode") == Some(1) && text(input, "ipv6_mode") != Some("native") {
                return false;
            }
            if !same(cfg.get("ipv6_mode"), input.get("ipv6_mode")) {
                return false;
            }
            if text(input, "ipv6_mode") == Some("off") {
                return true;
            }
            let has_dns = present(input, "dns1") || present(input, "dns2");
            if n(cfg, "peerdns") != Some(if has_dns { 0 } else { 1 })
                || (has_dns && !dns_matches(cfg.get("dns"), input))
            {
                return false;
            }
            matching(
                cfg,
                input,
                &[
                    ("nat6_enabled", "nat6_enabled"),
                    ("use_pppoev4", "use_pppoev4"),
                    ("ip6prefix", "ip6prefix"),
                    ("ip6prefixlen", "ip6prefixlen"),
                    ("ip6addr", "ip6addr"),
                    ("ip6gw", "ip6gw"),
                ],
            ) && (n(input, "use_pppoev4") != Some(0)
                || (cfg.get("usernameConfigured") == Some(&Value::Bool(true))
                    && cfg.get("passwordConfigured") == Some(&Value::Bool(true))))
        }
        "set_multiwan_enable" => after
            .get("info")
            .is_some_and(|v| same(v.get("enable"), input.get("enable"))),
        "set_multiwan_policy" => after
            .get("info")
            .is_some_and(|v| same(v.get("policy"), input.get("policy"))),
        "set_multiwan_weight" => after.get("info").is_some_and(|v| {
            matching(
                v,
                input,
                &[
                    ("bandwidth_wan1", "bandwidth_wan1"),
                    ("bandwidth_wan2", "bandwidth_wan2"),
                ],
            ) && n(v, "weight1").is_some_and(|n| n > 0)
                && n(v, "weight2").is_some_and(|n| n > 0)
        }),
        "set_multiwan_dev_policy" => {
            let Some(rows) = after
                .pointer("/info/dev_policies")
                .and_then(Value::as_array)
            else {
                return false;
            };
            let Some(want) = text(input, "mac").and_then(mac_list) else {
                return false;
            };
            want.iter().all(|mac| {
                let row = rows
                    .iter()
                    .find(|row| text(row, "mac").is_some_and(|s| s.eq_ignore_ascii_case(mac)));
                if text(input, "opt") == Some("1") {
                    row.is_none()
                } else {
                    row.is_some_and(|r| {
                        same(r.get("wan"), input.get("wan"))
                            && same(r.get("manual"), input.get("manual"))
                    })
                }
            })
        }
        "ps_wan" => after.get("wan").is_some_and(|v| {
            same(v.get("mode"), input.get("mode"))
                && (n(input, "mode") != Some(1) || same(v.get("wan_port"), input.get("wan_port")))
        }),
        "ps_wandt" => after.get("wandt").is_some_and(|v| {
            same(v.get("enable"), input.get("enable"))
                && (n(input, "enable") == Some(1) || same(v.get("wan_port"), input.get("wan_port")))
        }),
        "ps_game" => after.get("game").is_some_and(|v| {
            same(v.get("enable"), input.get("enable"))
                && (n(input, "enable") == Some(0) || same(v.get("ports"), input.get("ports")))
        }),
        "ps_lag" => after.get("lag").is_some_and(|v| {
            same(v.get("enable"), input.get("enable"))
                && same(v.get("mode"), input.get("mode"))
                && (n(input, "enable") == Some(0)
                    || (same(v.get("ports"), input.get("ports")) && n(v, "status") == Some(0)))
        }),
        "ps_iptv" => after.get("iptv").is_some_and(|v| {
            same(v.get("enable"), input.get("enable"))
                && (n(input, "enable") == Some(0)
                    || matching(
                        v,
                        input,
                        &[
                            ("ports", "ports"),
                            ("profile", "profile"),
                            ("vid", "vid"),
                            ("priority", "priority"),
                        ],
                    ))
        }),
        "ps_wantag" => after.get("wantag").is_some_and(|v| {
            same(v.get("interface"), input.get("interface"))
                && same(v.get("profile"), input.get("profile"))
                && (n(input, "profile") == Some(0)
                    || matching(v, input, &[("vid", "vid"), ("priority", "priority")]))
        }),
        "ps_multiwan" => {
            let Some(cfg) = after.get("multiwan") else {
                return false;
            };
            if !same(cfg.get("enable"), input.get("enable")) {
                return false;
            }
            if n(input, "enable") == Some(0) {
                return true;
            }
            let Some(rows) = cfg.get("port_map").and_then(Value::as_array) else {
                return false;
            };
            for (name, key) in [
                ("WAN1", "port_map%5B0%5D%5Bport%5D"),
                ("WAN2", "port_map%5B1%5D%5Bport%5D"),
            ] {
                if !rows
                    .iter()
                    .any(|v| text(v, "name") == Some(name) && same(v.get("port"), input.get(key)))
                {
                    return false;
                }
            }
            !present(input, "policy%5Bmode%5D")
                || cfg.get("policy").is_some_and(|v| {
                    matching(
                        v,
                        input,
                        &[
                            ("mode", "policy%5Bmode%5D"),
                            ("bandwidth_wan1", "policy%5Bbandwidth_wan1%5D"),
                            ("bandwidth_wan2", "policy%5Bbandwidth_wan2%5D"),
                        ],
                    )
                })
        }
        "set_vlan_internet" => {
            let expected = if text(input, "opt") == Some("clean") {
                Some(0)
            } else {
                n(input, "internet_profile")
            };
            n(after, "internet_profile") == expected
                && (expected == Some(0)
                    || matching(
                        after,
                        input,
                        &[
                            ("internet_vid", "internet_vid"),
                            ("internet_priority", "internet_priority"),
                        ],
                    ))
        }
        _ => false,
    }
}

/// Additional private equality gate for credentials. `input` must be the
/// prepared invocation payload, including secrets kept by prepare_input.
/// The root owner ANDs this with verify(project(native_after)); never publishes
/// native_after or this payload. Merely being configured is not proof.
pub fn verify_private(action_id: &str, input: &Value, native_after: &Value) -> bool {
    let pairs: &[(&str, &str)] = match action_id {
        "set_wan" if text(input, "wanType") == Some("pppoe") => &[
            ("pppoeName", "/info/details/username"),
            ("pppoePwd", "/info/details/password"),
        ],
        "set_wan6"
            if text(input, "ipv6_mode") == Some("pppoev6")
                && n(input, "use_pppoev4").unwrap_or(1) == 0 =>
        {
            &[
                ("ipv6DialAccount", "/wan6_cfg/username"),
                ("ipv6DialPassword", "/wan6_cfg/password"),
            ]
        }
        _ => return DOMAIN.actions.iter().any(|a| a.id == action_id),
    };
    pairs.iter().all(|(form, path)| {
        text(input, form)
            .filter(|s| !s.is_empty())
            .zip(native_after.pointer(path).and_then(Value::as_str))
            .is_some_and(|(a, b)| a == b)
    })
}
