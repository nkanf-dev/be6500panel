//! Typed services, policy and maintenance contracts for the RN02 factory firmware.
//! Controller names below are fixed executor aliases, not public route names.
//! All writes are descriptors: the root executor owns checkpoint/apply/readback jobs.
//! `verify` proves native configuration only. Runtime-only reloads and destructive
//! maintenance never complete from an acknowledgement or a still-reachable getter.
use crate::features::{
    Action, Domain, Error, Field, FieldKind, Impact, Read, choice, field, integer, invalid,
    public_secrets,
};
use serde_json::{Map, Value, json};
use std::{collections::HashSet, net::Ipv4Addr};

const ON: &[Field] = &[integer("on", "开关", true, 0, 1)];
const ENABLE: &[Field] = &[integer("enable", "开关", true, 0, 1)];
const MAC: &[Field] = &[field("mac", "设备 MAC", FieldKind::Mac, true)];
const USER_ID: &[Field] = &[integer("user_id", "家庭用户 ID", true, 1, 65535)];
// XQDDNS uses one fixed section per provider, not arbitrary section names/URLs.
const DDNS_ID: &[Field] = &[integer("id", "服务商 ID（1–9）", true, 1, 9)];
const DDNS_ADD: &[Field] = &[
    integer("id", "服务商 ID（1–9）", true, 1, 9),
    integer("enable", "实例开关", true, 0, 1),
    field("domain", "更新域名", FieldKind::Text, true),
    field("username", "服务商账号", FieldKind::Secret, true),
    field("password", "服务商密码或令牌", FieldKind::Secret, true),
    choice("wanindex", "地址来源", true, &["WAN1", "WAN2"]),
    choice("iptype", "地址类型", true, &["0", "1", "2"]),
    integer("checkinterval", "检查间隔（分钟）", true, 1, 10080),
    integer("forceinterval", "强制更新间隔（小时）", true, 1, 8760),
];
const DDNS_EDIT: &[Field] = &[
    integer("id", "服务商 ID（1–9）", true, 1, 9),
    field("domain", "更新域名", FieldKind::Text, true),
    choice("wanindex", "地址来源", true, &["WAN1", "WAN2"]),
    choice("iptype", "地址类型", true, &["0", "1", "2"]),
    field("username", "新账号（留空保留）", FieldKind::Secret, false),
    field(
        "password",
        "新密码或令牌（留空保留）",
        FieldKind::Secret,
        false,
    ),
    integer("checkinterval", "检查间隔（分钟）", false, 1, 10080),
    integer("forceinterval", "强制更新间隔（小时）", false, 1, 8760),
];
const FORWARD_SINGLE: &[Field] = &[
    field("name", "规则名称", FieldKind::Text, true),
    field("ip", "目标 IPv4", FieldKind::Ipv4, true),
    integer("proto", "协议（1 TCP / 2 UDP / 3 两者）", true, 1, 3),
    integer("sport", "外部端口", true, 1, 65535),
    integer("dport", "内部端口", true, 1, 65535),
];
const FORWARD_RANGE: &[Field] = &[
    field("name", "规则名称", FieldKind::Text, true),
    field("ip", "目标 IPv4", FieldKind::Ipv4, true),
    integer("proto", "协议（1 TCP / 2 UDP / 3 两者）", true, 1, 3),
    integer("fport", "起始端口", true, 1, 65535),
    integer("tport", "结束端口", true, 1, 65535),
];
const LED: &[Field] = &[
    integer("on", "指示灯开关", true, 0, 1),
    integer("timer_on", "定时开关", false, 0, 1),
    field("timer_open", "开启时间 HH:MM", FieldKind::Text, false),
    field("timer_close", "关闭时间 HH:MM", FieldKind::Text, false),
];

pub const DOMAIN: Domain = Domain {
    id: "services",
    title: "服务、访问策略与系统维护",
    reads: &[
        Read {
            id: "forwarding",
            title: "端口转发",
            controller: "xqsystem",
            handler: "portForward",
            fields: &[integer(
                "ftype",
                "规则类型（0 全部 / 1 单端口 / 2 区间）",
                false,
                0,
                2,
            )],
        },
        Read {
            id: "dmz",
            title: "DMZ 入站暴露",
            controller: "xqsystem",
            handler: "getDMZInfo",
            fields: &[],
        },
        Read {
            id: "upnp",
            title: "UPnP 映射",
            controller: "xqsystem",
            handler: "upnpList",
            fields: &[],
        },
        Read {
            id: "ddns",
            title: "DDNS 实例",
            controller: "xqnetwork",
            handler: "ddnsStatus",
            fields: &[],
        },
        Read {
            id: "ddns_detail",
            title: "DDNS 实例设置",
            controller: "xqnetwork",
            handler: "getServer",
            fields: DDNS_ID,
        },
        Read {
            id: "qos",
            title: "QoS 模式、设备与访客",
            controller: "misystem",
            handler: "getQosInfo",
            fields: &[],
        },
        Read {
            id: "qos_device",
            title: "设备限速",
            controller: "misystem",
            handler: "getMACQoSInfo",
            fields: MAC,
        },
        Read {
            id: "acceleration_setting",
            title: "原厂加速强制启用配置（不是实际引擎状态）",
            controller: "misystem",
            handler: "hwnatStatus",
            fields: &[],
        },
        Read {
            id: "access",
            title: "无线黑白名单",
            controller: "xqnetwork",
            handler: "getWifiMacfilterInfo",
            fields: &[integer("model", "名单（0 黑 / 1 白）", false, 0, 1)],
        },
        Read {
            id: "web_access",
            title: "管理入口访问名单",
            controller: "misystem",
            handler: "getWebAccessInfo",
            fields: &[],
        },
        Read {
            id: "firewall",
            title: "防火墙总开关",
            controller: "xqsystem",
            handler: "get_firewall_enable",
            fields: &[],
        },
        Read {
            id: "spi",
            title: "SPI 防护",
            controller: "xqsystem",
            handler: "get_spi_firewall",
            fields: &[],
        },
        Read {
            id: "dos",
            title: "DoS 防护",
            controller: "xqsystem",
            handler: "get_dos_firewall",
            fields: &[],
        },
        Read {
            id: "wan_ping",
            title: "WAN Ping 策略",
            controller: "xqsystem",
            handler: "get_wanping_firewall",
            fields: &[],
        },
        Read {
            id: "https",
            title: "强制 HTTPS",
            controller: "xqsystem",
            handler: "getForceHttps",
            fields: &[],
        },
        Read {
            id: "anti_attack",
            title: "反向路径、DoS 与扫描防护",
            controller: "anti_attack",
            handler: "get_status_api",
            fields: &[],
        },
        Read {
            id: "gateway_security",
            title: "网关安全",
            controller: "xqnetwork",
            handler: "getGwSecurity",
            fields: &[],
        },
        Read {
            id: "parental_users",
            title: "家庭用户",
            controller: "misystem",
            handler: "getPctlUserList",
            fields: &[],
        },
        Read {
            id: "parental_devices",
            title: "家庭设备归属",
            controller: "misystem",
            handler: "getPctlDev",
            fields: &[],
        },
        Read {
            id: "parental_time",
            title: "家庭禁网时段",
            controller: "misystem",
            handler: "getPctlDenyTime",
            fields: USER_ID,
        },
        Read {
            id: "parental_apps",
            title: "家庭应用限时",
            controller: "misystem",
            handler: "getPctlApp",
            fields: USER_ID,
        },
        Read {
            id: "parental_hosts",
            title: "家庭域名过滤",
            controller: "misystem",
            handler: "getPctlBanHost",
            fields: USER_ID,
        },
        Read {
            id: "parental_temporary",
            title: "家庭临时禁网",
            controller: "misystem",
            handler: "getPctlTempBan",
            fields: USER_ID,
        },
        Read {
            id: "router_name",
            title: "路由器名称",
            controller: "misystem",
            handler: "getRouterName",
            fields: &[],
        },
        Read {
            id: "time",
            title: "系统时间与时区",
            controller: "misystem",
            handler: "getSysTime",
            fields: &[],
        },
        Read {
            id: "led",
            title: "状态灯与时段",
            controller: "misystem",
            handler: "ledCtl",
            fields: &[],
        },
        Read {
            id: "eth_led",
            title: "网口灯与时段",
            controller: "misystem",
            handler: "DoEthLED",
            fields: &[],
        },
        Read {
            id: "all_led",
            title: "全部指示灯",
            controller: "misystem",
            handler: "DoAllLED",
            fields: &[],
        },
        Read {
            id: "version",
            title: "型号与 ROM 版本",
            controller: "xqsystem",
            handler: "getInitInfo",
            fields: &[],
        },
        Read {
            id: "ota",
            title: "自动 OTA 设置",
            controller: "xqsystem",
            handler: "getOTAInfo",
            fields: &[],
        },
        Read {
            id: "rom_update",
            title: "官方 ROM 更新状态",
            controller: "xqsystem",
            handler: "checkRomUpdate",
            fields: &[],
        },
        Read {
            id: "upgrade_status",
            title: "官方升级任务进度",
            controller: "xqsystem",
            handler: "upgradeStatus",
            fields: &[],
        },
        Read {
            id: "ntp",
            title: "NTP 服务器",
            controller: "sysutil",
            handler: "getNTPServerList",
            fields: &[],
        },
        Read {
            id: "time_mode",
            title: "自动对时模式",
            controller: "sysutil",
            handler: "timeMode",
            fields: &[],
        },
        Read {
            id: "scheduled_reboot",
            title: "计划重启",
            controller: "maintenance",
            handler: "getSchedule",
            fields: &[],
        },
        Read {
            id: "acceleration_status",
            title: "实际硬件加速引擎与计数",
            controller: "maintenance",
            handler: "acceleration_status",
            fields: &[],
        },
        Read {
            id: "qos_history",
            title: "设备原厂 QoS 配置",
            controller: "misystem",
            handler: "getQos",
            fields: &[],
        },
    ],
    actions: &[
        Action {
            id: "forward_add",
            title: "添加单端口转发",
            controller: "xqsystem",
            handler: "addRedirect",
            fields: FORWARD_SINGLE,
            impact: Impact::Network,
            configs: &["firewall"],
            readback: "forwarding",
        },
        Action {
            id: "forward_range_add",
            title: "添加端口区间转发",
            controller: "xqsystem",
            handler: "addRangeRedirect",
            fields: FORWARD_RANGE,
            impact: Impact::Network,
            configs: &["firewall"],
            readback: "forwarding",
        },
        Action {
            id: "forward_delete",
            title: "删除端口转发",
            controller: "xqsystem",
            handler: "deleteRedirect",
            fields: &[
                integer("port", "规则起始端口", true, 1, 65535),
                integer("proto", "协议（1 TCP / 2 UDP / 3 两者）", true, 1, 3),
            ],
            impact: Impact::Network,
            configs: &["firewall"],
            readback: "forwarding",
        },
        Action {
            id: "forward_apply",
            title: "应用原厂端口转发规则",
            controller: "xqsystem",
            handler: "redirectApply",
            fields: &[],
            impact: Impact::Network,
            configs: &["firewall"],
            readback: "forwarding",
        },
        Action {
            id: "dmz_set",
            title: "设置 DMZ 暴露目标",
            controller: "xqsystem",
            handler: "setDMZ",
            fields: &[
                field("ip", "目标 IPv4", FieldKind::Ipv4, true),
                field("mac", "目标 MAC", FieldKind::Mac, false),
                integer("mode", "原厂 DMZ 模式", true, 0, 1),
            ],
            impact: Impact::Network,
            configs: &["firewall", "network", "xiaoqiang", "mwan3"],
            readback: "dmz",
        },
        Action {
            id: "dmz_off",
            title: "关闭 DMZ",
            controller: "xqsystem",
            handler: "closeDMZ",
            fields: &[integer("mode", "原厂 DMZ 模式", true, 0, 1)],
            impact: Impact::Network,
            configs: &["firewall", "network", "xiaoqiang", "mwan3"],
            readback: "dmz",
        },
        Action {
            id: "dmz_reload",
            title: "重载 DMZ",
            controller: "xqsystem",
            handler: "reloadDMZ",
            fields: &[integer("mode", "原厂 DMZ 模式", true, 0, 1)],
            impact: Impact::Network,
            configs: &["firewall", "network", "xiaoqiang", "mwan3"],
            readback: "dmz",
        },
        Action {
            id: "upnp_switch",
            title: "切换 UPnP",
            controller: "xqsystem",
            handler: "upnpSwitch",
            fields: &[integer("switch", "UPnP 开关", true, 0, 1)],
            impact: Impact::Network,
            configs: &["upnpd"],
            readback: "upnp",
        },
        Action {
            id: "ddns_add",
            title: "添加服务商实例",
            controller: "xqnetwork",
            handler: "addServer",
            fields: DDNS_ADD,
            impact: Impact::Local,
            configs: &["ddns"],
            readback: "ddns_detail",
        },
        Action {
            id: "ddns_edit",
            title: "修改服务商实例",
            controller: "xqnetwork",
            handler: "ddnsEdit",
            fields: DDNS_EDIT,
            impact: Impact::Local,
            configs: &["ddns"],
            readback: "ddns_detail",
        },
        Action {
            id: "ddns_delete",
            title: "删除服务商实例",
            controller: "xqnetwork",
            handler: "deleteServer",
            fields: DDNS_ID,
            impact: Impact::Local,
            configs: &["ddns"],
            readback: "ddns",
        },
        Action {
            id: "ddns_switch",
            title: "切换服务商实例",
            controller: "xqnetwork",
            handler: "serverSwitch",
            fields: &[
                integer("id", "服务商 ID", true, 1, 9),
                integer("on", "实例开关", true, 0, 1),
            ],
            impact: Impact::Local,
            configs: &["ddns"],
            readback: "ddns",
        },
        Action {
            id: "ddns_reload",
            title: "刷新原厂 DDNS 实例",
            controller: "xqnetwork",
            handler: "ddnsReload",
            fields: &[],
            impact: Impact::Local,
            configs: &["ddns"],
            readback: "ddns",
        },
        Action {
            id: "qos_switch",
            title: "切换 QoS",
            controller: "misystem",
            handler: "qosSwitch",
            fields: ON,
            impact: Impact::Network,
            configs: &["miqos", "hwnat"],
            readback: "qos",
        },
        Action {
            id: "qos_mode",
            title: "设置 QoS 模式",
            controller: "misystem",
            handler: "qosMode",
            fields: &[integer(
                "mode",
                "模式（0 自动 / 1 优先级 / 2 限速 / 3–6 应用）",
                true,
                0,
                6,
            )],
            impact: Impact::Network,
            configs: &["miqos", "hwnat"],
            readback: "qos",
        },
        Action {
            id: "qos_band",
            title: "设置上行与下行带宽",
            controller: "misystem",
            handler: "setBand",
            fields: &[
                integer("upload", "上行 Mbps", true, 1, 100000),
                integer("download", "下行 Mbps", true, 1, 100000),
                integer("manual", "手动带宽", true, 1, 1),
            ],
            impact: Impact::Network,
            configs: &["miqos", "hwnat", "xiaoqiang"],
            readback: "qos",
        },
        Action {
            id: "qos_limit",
            title: "设置设备优先级或限速",
            controller: "misystem",
            handler: "qosLimit",
            fields: &[
                field("mac", "设备 MAC", FieldKind::Mac, true),
                integer("mode", "模式（1 优先级 / 2 限速）", true, 1, 2),
                integer("upload", "上行级别 1–3 或 KB/s", true, 0, 100000000),
                integer("download", "下行级别 1–3 或 KB/s", true, 0, 100000000),
            ],
            impact: Impact::Network,
            configs: &["miqos", "hwnat"],
            readback: "qos_history",
        },
        Action {
            id: "qos_limits",
            title: "批量设置设备优先级或限速",
            controller: "misystem",
            handler: "qosLimits",
            fields: &[
                integer("mode", "模式（1 优先级 / 2 限速）", true, 1, 2),
                field(
                    "data",
                    "设备列表（mac / maxup / maxdown）",
                    FieldKind::Json,
                    true,
                ),
            ],
            impact: Impact::Network,
            configs: &["miqos", "hwnat"],
            readback: "qos_history",
        },
        Action {
            id: "qos_device",
            title: "设置设备固定限速",
            controller: "misystem",
            handler: "setMACQoSInfo",
            fields: &[
                field("mac", "设备 MAC", FieldKind::Mac, true),
                integer("upload", "上行 KB/s", true, 0, 100000000),
                integer("download", "下行 KB/s", true, 0, 100000000),
            ],
            impact: Impact::Network,
            configs: &["miqos", "hwnat"],
            readback: "qos_device",
        },
        Action {
            id: "qos_offlimit",
            title: "取消设备限速",
            controller: "misystem",
            handler: "qosOffLimit",
            fields: MAC,
            impact: Impact::Network,
            configs: &["miqos", "hwnat"],
            readback: "qos_device",
        },
        Action {
            id: "qos_guest",
            title: "设置访客带宽比例",
            controller: "misystem",
            handler: "qosGuest",
            fields: &[
                field("percent", "下行比例（0–1）", FieldKind::Text, true),
                field("percent_up", "上行比例（0–1）", FieldKind::Text, true),
            ],
            impact: Impact::Network,
            configs: &["miqos", "hwnat"],
            readback: "qos",
        },
        Action {
            id: "access_switch",
            title: "切换无线访问名单",
            controller: "xqnetwork",
            handler: "setWifiMacfilter",
            fields: &[
                integer("enable", "开关", true, 0, 1),
                integer("model", "名单（0 黑 / 1 白）", true, 0, 1),
            ],
            impact: Impact::Network,
            configs: &["macfilter", "wifiblist", "wifiwlist", "webfilter"],
            readback: "access",
        },
        Action {
            id: "access_edit",
            title: "增删名单中的设备",
            controller: "xqnetwork",
            handler: "editDevice",
            fields: &[
                field("mac", "设备 MAC（单个）", FieldKind::Mac, true),
                integer("model", "名单（0 黑 / 1 白）", true, 0, 1),
                integer("option", "操作（0 添加 / 1 移除）", true, 0, 1),
            ],
            impact: Impact::Network,
            configs: &["macfilter", "wifiblist", "wifiwlist", "webfilter"],
            readback: "access",
        },
        Action {
            id: "access_add",
            title: "手动添加名单设备",
            controller: "xqnetwork",
            handler: "manuallyAdd",
            fields: &[
                field("mac", "设备 MAC", FieldKind::Mac, true),
                field("name", "设备名称", FieldKind::Text, true),
                integer("model", "名单（0 黑 / 1 白）", true, 0, 1),
            ],
            impact: Impact::Network,
            configs: &["macfilter", "wifiblist", "wifiwlist", "webfilter"],
            readback: "access",
        },
        Action {
            id: "web_access",
            title: "管理入口名单开关与设备",
            controller: "misystem",
            handler: "webAccess",
            fields: &[
                integer("open", "名单开关", true, 0, 1),
                field("mac", "设备 MAC", FieldKind::Mac, false),
                integer("opt", "操作（0 添加 / 1 移除）", false, 0, 1),
            ],
            impact: Impact::Network,
            configs: &["webfilter"],
            readback: "web_access",
        },
        Action {
            id: "firewall_switch",
            title: "切换防火墙",
            controller: "xqsystem",
            handler: "set_firewall_enable",
            fields: &[integer("firewall_enable", "开关", true, 0, 1)],
            impact: Impact::Network,
            configs: &["firewall"],
            readback: "firewall",
        },
        Action {
            id: "spi_switch",
            title: "切换 SPI 防护",
            controller: "xqsystem",
            handler: "set_spi_firewall",
            fields: &[integer("spi_firewall", "开关", true, 0, 1)],
            impact: Impact::Network,
            configs: &["firewall"],
            readback: "spi",
        },
        Action {
            id: "dos_switch",
            title: "切换 DoS 防护",
            controller: "xqsystem",
            handler: "set_dos_firewall",
            fields: &[integer("dos_firewall", "开关", true, 0, 1)],
            impact: Impact::Network,
            configs: &["firewall", "firewall_cpp"],
            readback: "dos",
        },
        Action {
            id: "wan_ping_switch",
            title: "切换 WAN Ping 策略",
            controller: "xqsystem",
            handler: "set_wanping_firewall",
            fields: &[integer("wanping_firewall", "开关", true, 0, 1)],
            impact: Impact::Network,
            configs: &["firewall"],
            readback: "wan_ping",
        },
        Action {
            id: "anti_rpfilter",
            title: "切换反向路径检查",
            controller: "anti_attack",
            handler: "set_rpfilter_api",
            fields: ENABLE,
            impact: Impact::Network,
            configs: &["firewall_cpp"],
            readback: "anti_attack",
        },
        Action {
            id: "anti_dos",
            title: "切换原厂 DoS 防护",
            controller: "anti_attack",
            handler: "set_dos_api",
            fields: ENABLE,
            impact: Impact::Network,
            configs: &["firewall_cpp"],
            readback: "anti_attack",
        },
        Action {
            id: "anti_scan",
            title: "切换扫描防护",
            controller: "anti_attack",
            handler: "set_scan_api",
            fields: ENABLE,
            impact: Impact::Network,
            configs: &["firewall_cpp"],
            readback: "anti_attack",
        },
        Action {
            id: "https_switch",
            title: "切换强制 HTTPS",
            controller: "xqsystem",
            handler: "setForceHttps",
            fields: ON,
            impact: Impact::Network,
            configs: &["nginx"],
            readback: "https",
        },
        Action {
            id: "gateway_security",
            title: "切换网关安全",
            controller: "xqnetwork",
            handler: "setGwSecurity",
            fields: ON,
            impact: Impact::Network,
            configs: &["local_gw_security"],
            readback: "gateway_security",
        },
        Action {
            id: "router_name",
            title: "修改路由器名称",
            controller: "misystem",
            handler: "setRouterName",
            fields: &[
                field("name", "名称（最多 27 字节）", FieldKind::Text, true),
                field("locale", "位置（最多 24 字节）", FieldKind::Text, false),
            ],
            impact: Impact::Local,
            configs: &["xiaoqiang"],
            readback: "router_name",
        },
        Action {
            id: "time_set",
            title: "设置时区或手动时间",
            controller: "misystem",
            handler: "setSysTime",
            fields: &[
                field(
                    "time",
                    "手动时间 YYYY-MM-DD HH:MM:SS",
                    FieldKind::Text,
                    false,
                ),
                field("timezone", "时区显示名称", FieldKind::Text, false),
                field("index", "原厂时区索引", FieldKind::Text, true),
            ],
            impact: Impact::Local,
            configs: &["system"],
            readback: "time",
        },
        Action {
            id: "led_set",
            title: "设置状态灯与时段",
            controller: "misystem",
            handler: "ledCtl",
            fields: LED,
            impact: Impact::Local,
            configs: &["xiaoqiang"],
            readback: "led",
        },
        Action {
            id: "eth_led_set",
            title: "设置网口灯与时段",
            controller: "misystem",
            handler: "DoEthLED",
            fields: LED,
            impact: Impact::Local,
            configs: &["xiaoqiang"],
            readback: "eth_led",
        },
        Action {
            id: "all_led_set",
            title: "设置全部指示灯",
            controller: "misystem",
            handler: "DoAllLED",
            fields: ON,
            impact: Impact::Local,
            configs: &["xiaoqiang"],
            readback: "all_led",
        },
        Action {
            id: "ota_set",
            title: "设置自动 OTA",
            controller: "xqsystem",
            handler: "setOTAInfo",
            fields: &[integer("auto", "自动 OTA", true, 0, 1)],
            impact: Impact::Maintenance,
            configs: &["otapred"],
            readback: "ota",
        },
        Action {
            id: "reboot",
            title: "重启路由器（维护任务）",
            controller: "xqsystem",
            handler: "reboot",
            fields: &[choice("client", "客户端标记", false, &["web"])],
            impact: Impact::Maintenance,
            configs: &["system", "xiaoqiang"],
            readback: "version",
        },
        Action {
            id: "factory_reset",
            title: "恢复出厂（独立维护任务）",
            controller: "xqsystem",
            handler: "reset",
            fields: &[integer("format", "不格式化用户磁盘", true, 0, 0)],
            impact: Impact::Maintenance,
            configs: &[
                "system",
                "network",
                "wireless",
                "dhcp",
                "firewall",
                "xiaoqiang",
                "macfilter",
                "miqos",
                "ddns",
                "nginx",
                "webfilter",
            ],
            readback: "version",
        },
        Action {
            id: "official_upgrade",
            title: "下载并升级官方固件",
            controller: "xqsystem",
            handler: "upgradeRom",
            fields: &[],
            impact: Impact::Maintenance,
            configs: &[
                "system",
                "network",
                "wireless",
                "dhcp",
                "firewall",
                "xiaoqiang",
                "miqos",
                "ddns",
                "nginx",
            ],
            readback: "upgrade_status",
        },
        Action {
            id: "ntp_set",
            title: "设置 NTP 服务器",
            controller: "sysutil",
            handler: "setNTPServer",
            fields: &[
                field("server1", "NTP 主服务器", FieldKind::Text, true),
                field("server2", "NTP 备用服务器", FieldKind::Text, false),
            ],
            impact: Impact::Local,
            configs: &["system"],
            readback: "ntp",
        },
        Action {
            id: "time_mode_set",
            title: "切换自动对时模式",
            controller: "sysutil",
            handler: "timeMode",
            fields: &[
                integer("sync", "立即对时", true, 0, 1),
                integer("mode", "模式（0 NTP / 1 手动）", true, 0, 1),
            ],
            impact: Impact::Local,
            configs: &["system"],
            readback: "time_mode",
        },
        Action {
            id: "scheduled_reboot_set",
            title: "设置计划重启",
            controller: "maintenance",
            handler: "setSchedule",
            fields: &[
                field("enabled", "启用计划", FieldKind::Boolean, true),
                field("time", "本地时间 HH:MM", FieldKind::Text, true),
                field("weekdays", "星期 0–6 列表", FieldKind::Json, true),
            ],
            impact: Impact::Maintenance,
            configs: &["system"],
            readback: "scheduled_reboot",
        },
        Action {
            id: "parental_user_add",
            title: "添加家庭用户",
            controller: "misystem",
            handler: "setPctl",
            fields: &[field(
                "opt_list",
                "单项原厂 v2 操作：mipctl_add_user",
                FieldKind::Json,
                true,
            )],
            impact: Impact::Network,
            configs: &["mipctl_user", "macfilter", "firewall_cpp"],
            readback: "parental_users",
        },
        Action {
            id: "parental_user_edit",
            title: "修改家庭用户",
            controller: "misystem",
            handler: "setPctl",
            fields: &[field(
                "opt_list",
                "单项原厂 v2 操作：mipctl_edit_user",
                FieldKind::Json,
                true,
            )],
            impact: Impact::Network,
            configs: &["mipctl_user", "macfilter", "firewall_cpp"],
            readback: "parental_users",
        },
        Action {
            id: "parental_user_delete",
            title: "删除家庭用户",
            controller: "misystem",
            handler: "setPctl",
            fields: &[field(
                "opt_list",
                "单项原厂 v2 操作：mipctl_del_user",
                FieldKind::Json,
                true,
            )],
            impact: Impact::Network,
            configs: &["mipctl_user", "macfilter", "firewall_cpp"],
            readback: "parental_users",
        },
        Action {
            id: "parental_devices_set",
            title: "设置家庭设备归属",
            controller: "misystem",
            handler: "setPctl",
            fields: &[field(
                "opt_list",
                "单项原厂 v2 操作：mipctl_set_device",
                FieldKind::Json,
                true,
            )],
            impact: Impact::Network,
            configs: &["mipctl_user", "macfilter", "firewall_cpp", "parentalctl"],
            readback: "parental_devices",
        },
        Action {
            id: "parental_time_set",
            title: "设置家庭禁网时段",
            controller: "misystem",
            handler: "setPctl",
            fields: &[field(
                "opt_list",
                "单项原厂 v2 操作：mipctl_set_deny_time",
                FieldKind::Json,
                true,
            )],
            impact: Impact::Network,
            configs: &["mipctl_user", "macfilter", "firewall_cpp"],
            readback: "parental_time",
        },
        Action {
            id: "parental_apps_set",
            title: "设置家庭应用限时",
            controller: "misystem",
            handler: "setPctl",
            fields: &[field(
                "opt_list",
                "单项原厂 v2 操作：mipctl_set_app_antiaddict",
                FieldKind::Json,
                true,
            )],
            impact: Impact::Network,
            configs: &["mipctl_user", "macfilter", "firewall_cpp"],
            readback: "parental_apps",
        },
        Action {
            id: "parental_hosts_set",
            title: "设置家庭域名过滤",
            controller: "misystem",
            handler: "setPctl",
            fields: &[field(
                "opt_list",
                "单项原厂 v2 操作：mipctl_set_filting_net",
                FieldKind::Json,
                true,
            )],
            impact: Impact::Network,
            configs: &["mipctl_user", "macfilter", "firewall_cpp"],
            readback: "parental_hosts",
        },
        Action {
            id: "parental_temporary_set",
            title: "设置家庭临时禁网",
            controller: "misystem",
            handler: "setPctl",
            fields: &[field(
                "opt_list",
                "单项原厂 v2 操作：mipctl_set_temp_deny",
                FieldKind::Json,
                true,
            )],
            impact: Impact::Network,
            configs: &["mipctl_user", "macfilter", "firewall_cpp"],
            readback: "parental_temporary",
        },
    ],
};

fn bad_response() -> Error {
    Error {
        status: 502,
        code: "vendor_invalid_response",
        message: "原厂服务响应无效。",
    }
}
fn scalar(value: &Value) -> bool {
    value.is_null()
        || value.is_boolean()
        || value.is_number()
        || value.as_str().is_some_and(|s| s.len() <= 4096)
}
fn scalars(value: &Value, keys: &[&str]) -> Value {
    let mut out = Map::new();
    for key in keys {
        if let Some(v) = value.get(*key).filter(|v| scalar(v)) {
            out.insert((*key).into(), v.clone());
        }
    }
    Value::Object(out)
}
fn nested(out: &mut Value, source: &Value, key: &str, project: fn(&Value) -> Value) {
    if let Some(v) = source.get(key).filter(|v| v.is_object()) {
        out[key] = project(v);
    }
}
fn rows(out: &mut Value, source: &Value, key: &str, project: fn(&Value) -> Value) {
    if let Some(v) = source.get(key).and_then(Value::as_array) {
        out[key] = Value::Array(
            v.iter()
                .take(512)
                .filter(|v| v.is_object())
                .map(project)
                .collect(),
        );
    } else if source
        .get(key)
        .is_some_and(|v| v.is_object() && v.as_object().is_some_and(Map::is_empty))
    {
        // Lua encodes empty native lists as {} on some firmware revisions.
        out[key] = json!([]);
    }
}
fn list_scalars(out: &mut Value, source: &Value, key: &str, accept: fn(&Value) -> bool) {
    if let Some(v) = source.get(key).and_then(Value::as_array) {
        out[key] = Value::Array(v.iter().take(512).filter(|v| accept(v)).cloned().collect());
    } else if source
        .get(key)
        .is_some_and(|v| v.is_object() && v.as_object().is_some_and(Map::is_empty))
    {
        out[key] = json!([]);
    }
}
fn number(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
        .filter(|n| n.is_finite())
}
fn integral(v: &Value) -> Option<i64> {
    let n = number(v)?;
    (n.fract() == 0.0 && n >= i64::MIN as f64 && n < i64::MAX as f64).then_some(n as i64)
}
fn mac_str(s: &str) -> bool {
    let parts: Vec<_> = s.split(':').collect();
    parts.len() == 6
        && parts
            .iter()
            .all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_hexdigit()))
        && u8::from_str_radix(parts[0], 16).is_ok_and(|b| b & 1 == 0)
        && s != "00:00:00:00:00:00"
}
fn mac_value(v: &Value) -> bool {
    v.as_str().is_some_and(mac_str)
}
fn bit_value(v: &Value) -> bool {
    integral(v).is_some_and(|n| n == 0 || n == 1)
}
fn host_value(v: &Value) -> bool {
    v.as_str().is_some_and(host)
}
fn host(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 253
        && !s.contains('/')
        && s.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}
fn forward_row(v: &Value) -> Value {
    let mut out = scalars(v, &["name", "destip", "proto", "destport", "ftype"]);
    if let Some(port) = v.get("srcport") {
        if integral(port).is_some_and(|n| (1..=65535).contains(&n)) {
            out["srcport"] = port.clone();
        } else if port.is_object() {
            out["srcport"] = scalars(port, &["f", "t"]);
        }
    }
    out
}
fn ddns_row(v: &Value) -> Value {
    // Do not emit free-form provider error/URL text: it can include an account or token.
    scalars(
        v,
        &[
            "id",
            "status",
            "enabled",
            "servicename",
            "domain",
            "wanip",
            "wan6ip",
            "wanname",
            "iptype",
            "checkinterval",
            "lastupdate",
        ],
    )
}
fn qos_limits(v: &Value) -> Value {
    scalars(
        v,
        &[
            "downmax",
            "downmin",
            "upmax",
            "upmin",
            "maxdownper",
            "upmaxper",
            "level",
            "flag",
        ],
    )
}
fn device_row(v: &Value) -> Value {
    let mut out = scalars(v, &["mac", "name", "ip", "online", "added", "isap"]);
    nested(&mut out, v, "qos", qos_limits);
    nested(&mut out, v, "authority", |v| {
        scalars(v, &["wan", "lan", "admin"])
    });
    out
}
fn parental_user(v: &Value) -> Value {
    scalars(v, &["user_id", "user_name", "icon", "status"])
}
fn parental_device(v: &Value) -> Value {
    let mut out = scalars(v, &["user_id"]);
    list_scalars(&mut out, v, "devices", mac_value);
    out
}
fn parental_time(v: &Value) -> Value {
    let mut out = scalars(v, &["id", "start", "end"]);
    list_scalars(&mut out, v, "enable", bit_value);
    out
}
fn parental_app(v: &Value) -> Value {
    let mut out = scalars(v, &["class_name", "enable", "time_quota"]);
    rows(&mut out, v, "app_list", |v| scalars(v, &["name", "enable"]));
    out
}

/// Emit only the documented public scalar fields at each native nesting level.
/// Never return an unfiltered native DTO, including for malformed known fields.
pub fn project(read_id: &str, value: Value) -> Result<Value, Error> {
    if !DOMAIN.reads.iter().any(|r| r.id == read_id) {
        return Err(invalid());
    }
    if !value.is_object() {
        return Err(bad_response());
    }
    // Current RN02 getServer uses 1614 when this provider has no instance.
    // This is an explicit absent setting, not a malformed reply or credentials.
    if read_id == "ddns_detail" && value.get("code").and_then(integral) == Some(1614) {
        return Ok(json!({"configured":false}));
    }
    if let Some(code) = value.get("code")
        && integral(code) != Some(0)
    {
        return Err(bad_response());
    }
    let mut out = match read_id {
        "forwarding" => {
            let mut out = scalars(&value, &["status", "lanmask"]);
            rows(&mut out, &value, "list", forward_row);
            out
        }
        "dmz" => scalars(&value, &["status", "ip", "lanip", "lanmask"]),
        "upnp" => {
            let mut out = scalars(&value, &["status"]);
            rows(&mut out, &value, "list", |v| {
                scalars(v, &["protocol", "rport", "ip", "cport", "time", "name"])
            });
            out
        }
        "ddns" => {
            let mut out = scalars(&value, &["on", "flag"]);
            rows(&mut out, &value, "list", ddns_row);
            out
        }
        "ddns_detail" => {
            let mut out = scalars(
                &value,
                &[
                    "domain",
                    "checkinterval",
                    "forceinterval",
                    "wanindex",
                    "iptype",
                ],
            );
            for key in ["username", "password"] {
                out[key] = json!(value.get(key).and_then(Value::as_str).unwrap_or(""));
            }
            public_secrets(&mut out);
            out
        }
        "qos" | "qos_device" => {
            let mut out = json!({});
            nested(&mut out, &value, "status", |v| scalars(v, &["on", "mode"]));
            nested(&mut out, &value, "band", |v| {
                scalars(v, &["upload", "download"])
            });
            nested(&mut out, &value, "limit", |v| {
                scalars(v, &["upmax", "downmax", "flag"])
            });
            nested(&mut out, &value, "guest", |v| {
                scalars(v, &["percent", "percent_up", "UP", "DOWN"])
            });
            rows(&mut out, &value, "list", device_row);
            out
        }
        "qos_history" => {
            let mut out = json!({});
            nested(&mut out, &value, "status", |v| scalars(v, &["on", "mode"]));
            nested(&mut out, &value, "band", |v| {
                scalars(v, &["upload", "download"])
            });
            let mut dict = Map::new();
            if let Some(m) = value.get("dict").and_then(Value::as_object) {
                for (key, row) in m.iter().take(512) {
                    if mac_str(key) && row.is_object() {
                        dict.insert(
                            key.clone(),
                            scalars(row, &["mac", "level", "upmax", "downmax", "flag"]),
                        );
                    }
                }
                out["dict"] = Value::Object(dict);
            }
            out
        }
        "acceleration_setting" => {
            let mut out = json!({"source":"hwnat.switch.force_start","runtimeObserved":false});
            if let Some(v) = value.get("status").filter(|v| scalar(v)) {
                out["status"] = v.clone();
            }
            out
        }
        "acceleration_status" => {
            let mut out = json!({});
            for (key, allowed) in [
                ("engine", &["unknown", "ecm", "sfe", "ppe"][..]),
                ("state", &["active", "inactive", "unknown"][..]),
            ] {
                if let Some(v) = value
                    .get(key)
                    .filter(|v| v.as_str().is_some_and(|s| allowed.contains(&s)))
                {
                    out[key] = v.clone();
                }
            }
            if let Some(v) = value
                .get("frontend")
                .filter(|v| v.as_str().is_some_and(|s| s.len() <= 128))
            {
                out["frontend"] = v.clone();
            }
            for key in ["ecm", "sfe", "ppe"] {
                if let Some(v) = value.get(key).filter(|v| v.is_boolean() || v.is_null()) {
                    out[key] = v.clone();
                }
            }
            nested(&mut out, &value, "counters", |v| {
                let mut out = json!({});
                for key in [
                    "accelerated",
                    "pending",
                    "connections",
                    "hits",
                    "misses",
                    "drops",
                ] {
                    if let Some(n) = v.get(key).filter(|v| number(v).is_some_and(|n| n >= 0.0)) {
                        out[key] = n.clone();
                    }
                }
                out
            });
            out
        }
        "access" => {
            let mut out = scalars(&value, &["enable", "model"]);
            for key in ["list", "flist", "macfilter"] {
                rows(&mut out, &value, key, device_row);
            }
            list_scalars(&mut out, &value, "weblist", mac_value);
            out
        }
        "web_access" => {
            let mut out = scalars(&value, &["open"]);
            list_scalars(&mut out, &value, "list", mac_value);
            out
        }
        "firewall" => scalars(&value, &["firewall_enable"]),
        "spi" => scalars(&value, &["spi_firewall"]),
        "dos" => scalars(&value, &["dos_firewall"]),
        "wan_ping" => scalars(&value, &["wanping_firewall"]),
        "https" => scalars(&value, &["on"]),
        "gateway_security" => scalars(&value, &["enable"]),
        "anti_attack" => scalars(&value, &["rpfilter", "dos", "scan"]),
        "parental_users" => {
            let mut out = json!({});
            rows(&mut out, &value, "user_list", parental_user);
            out
        }
        "parental_devices" => {
            let mut out = json!({});
            rows(&mut out, &value, "list", parental_device);
            out
        }
        "parental_time" => {
            let mut out = json!({});
            rows(&mut out, &value, "time_list", parental_time);
            out
        }
        "parental_apps" => {
            let mut out = json!({});
            rows(&mut out, &value, "list", parental_app);
            out
        }
        "parental_hosts" => {
            let mut out = json!({});
            list_scalars(&mut out, &value, "list", host_value);
            out
        }
        "parental_temporary" => scalars(&value, &["deny", "show", "nxt_permit"]),
        "router_name" => scalars(&value, &["name", "locale"]),
        "time" => {
            let mut out = scalars(&value, &["role"]);
            nested(&mut out, &value, "time", |v| {
                scalars(
                    v,
                    &[
                        "timezone", "index", "year", "month", "day", "hour", "min", "sec",
                    ],
                )
            });
            out
        }
        "ntp" => {
            let mut out = json!({});
            list_scalars(&mut out, &value, "servers", host_value);
            out
        }
        "time_mode" => {
            let mut out = json!({});
            nested(&mut out, &value, "info", |v| scalars(v, &["mode", "sync"]));
            out
        }
        "scheduled_reboot" => {
            let mut out = scalars(&value, &["enabled", "time", "timeBasis", "reloadPending"]);
            list_scalars(&mut out, &value, "weekdays", |v| {
                integral(v).is_some_and(|n| (0..=6).contains(&n))
            });
            out
        }
        "led" | "eth_led" => scalars(
            &value,
            &[
                "status",
                "timer_status",
                "timer_open",
                "timer_close",
                "error",
            ],
        ),
        "all_led" => scalars(&value, &["status", "error"]),
        "version" => scalars(
            &value,
            &[
                "hardware",
                "model",
                "romversion",
                "language",
                "countrycode",
                "routername",
                "displayName",
            ],
        ),
        "ota" => scalars(&value, &["auto", "time"]),
        "rom_update" => {
            // Download URLs, hashes, change logs and otherParam remain backend-private.
            let mut out = scalars(
                &value,
                &["needUpdate", "version", "fileSize", "buildTime", "weight"],
            );
            nested(&mut out, &value, "status", |v| {
                scalars(v, &["status", "percent"])
            });
            out
        }
        "upgrade_status" => scalars(&value, &["status", "percent"]),
        _ => return Err(invalid()),
    };
    public_secrets(&mut out);
    Ok(out)
}

fn safe_text(s: &str, max: usize, empty: bool) -> bool {
    (empty || !s.trim().is_empty())
        && s.len() <= max
        && !s.chars().any(|c| {
            c.is_control() || ['`', '$', ';', '&', '|', '<', '>', '\\', '"', '\''].contains(&c)
        })
}
fn ipv4_target(s: &str) -> bool {
    s.parse::<Ipv4Addr>().is_ok_and(|ip| {
        let o = ip.octets();
        o[0] != 0 && o[0] != 127 && o[0] < 224 && o[3] != 0 && o[3] != 255
    })
}
fn hhmm(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 5
        && b[2] == b':'
        && [0, 1, 3, 4].iter().all(|i| b[*i].is_ascii_digit())
        && s[..2].parse::<u8>().is_ok_and(|n| n < 24)
        && s[3..].parse::<u8>().is_ok_and(|n| n < 60)
}
fn date_time(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 19
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b' '
        || b[13] != b':'
        || b[16] != b':'
    {
        return false;
    }
    if !(0..19)
        .filter(|i| ![4, 7, 10, 13, 16].contains(i))
        .all(|i| b[i].is_ascii_digit())
    {
        return false;
    }
    let y = s[..4].parse::<u32>().unwrap_or(0);
    let m = s[5..7].parse::<u32>().unwrap_or(0);
    let d = s[8..10].parse::<u32>().unwrap_or(0);
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let max = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        _ => 0,
    };
    (2000..=2099).contains(&y)
        && d > 0
        && d <= max
        && hhmm(&s[11..16])
        && s[17..].parse::<u8>().is_ok_and(|n| n < 60)
}
fn decoded(v: &Value) -> Result<Value, Error> {
    if let Some(s) = v.as_str() {
        if s.len() > 16 << 10 {
            return Err(invalid());
        }
        serde_json::from_str(s).map_err(|_| invalid())
    } else if v.is_array() || v.is_object() {
        if serde_json::to_vec(v).map_err(|_| invalid())?.len() > 16 << 10 {
            return Err(invalid());
        }
        Ok(v.clone())
    } else {
        Err(invalid())
    }
}
fn exact(v: &Value, required: &[&str], optional: &[&str]) -> bool {
    v.as_object().is_some_and(|m| {
        required.iter().all(|k| m.contains_key(*k))
            && m.keys()
                .all(|k| required.contains(&k.as_str()) || optional.contains(&k.as_str()))
    })
}
fn int_between(v: &Value, min: i64, max: i64) -> bool {
    integral(v).is_some_and(|n| (min..=max).contains(&n))
}
fn unique_list(v: &Value, max: usize, accept: fn(&Value) -> bool) -> bool {
    v.as_array().is_some_and(|a| {
        if a.len() > max || !a.iter().all(accept) {
            return false;
        }
        let mut seen = HashSet::new();
        a.iter()
            .all(|v| seen.insert(v.to_string().to_ascii_lowercase()))
    })
}
fn pctl_op(id: &str) -> Option<&'static str> {
    match id {
        "parental_user_add" => Some("mipctl_add_user"),
        "parental_user_edit" => Some("mipctl_edit_user"),
        "parental_user_delete" => Some("mipctl_del_user"),
        "parental_devices_set" => Some("mipctl_set_device"),
        "parental_time_set" => Some("mipctl_set_deny_time"),
        "parental_apps_set" => Some("mipctl_set_app_antiaddict"),
        "parental_hosts_set" => Some("mipctl_set_filting_net"),
        "parental_temporary_set" => Some("mipctl_set_temp_deny"),
        _ => None,
    }
}
fn parental_input(id: &str, input: &Value) -> Result<Value, Error> {
    let decoded = decoded(&input["opt_list"])?;
    let a = decoded.as_array().ok_or_else(invalid)?;
    if a.len() != 1 {
        return Err(invalid());
    }
    let op = &a[0];
    if op["opt"].as_str() != pctl_op(id) {
        return Err(invalid());
    }
    if id != "parental_user_add" && !int_between(&op["user_id"], 1, 65535) {
        return Err(invalid());
    }
    let ok = match id {
        "parental_user_add" | "parental_user_edit" => {
            let required = if id == "parental_user_add" {
                &["opt", "user_name", "icon"][..]
            } else {
                &["opt", "user_id", "user_name", "icon"][..]
            };
            exact(op, required, &[])
                && op["user_name"].as_str().is_some_and(|s| {
                    safe_text(s, 36, false)
                        && s.chars()
                            .map(|c| if c.is_ascii() { 1 } else { 2 })
                            .sum::<usize>()
                            <= 12
                })
                && op["icon"].as_str().is_some_and(|s| {
                    safe_text(s, 64, false) && !s.contains(['/', ':', '%', '?', '*', '^'])
                })
        }
        "parental_user_delete" => exact(op, &["opt", "user_id"], &[]),
        "parental_devices_set" => {
            exact(op, &["opt", "user_id", "devices"], &[])
                && unique_list(&op["devices"], 64, mac_value)
        }
        "parental_hosts_set" => {
            exact(op, &["opt", "user_id", "list"], &[]) && unique_list(&op["list"], 128, host_value)
        }
        "parental_temporary_set" => {
            exact(op, &["opt", "user_id", "deny"], &[]) && op["deny"].is_boolean()
        }
        "parental_time_set" => {
            exact(op, &["opt", "user_id", "time_list"], &[])
                && op["time_list"].as_array().is_some_and(|a| {
                    a.len() <= 32
                        && a.iter().all(|t| {
                            exact(t, &["start", "end", "enable"], &["id"])
                                && int_between(&t["start"], 0, 2879)
                                && int_between(&t["end"], 1, 2880)
                                && integral(&t["start"]) < integral(&t["end"])
                                && t["enable"]
                                    .as_array()
                                    .is_some_and(|a| a.len() == 7 && a.iter().all(bit_value))
                                && t.get("id").is_none_or(|v| {
                                    v.as_str().is_some_and(|s| {
                                        s.len() <= 80
                                            && s.bytes().all(|b| b.is_ascii_digit() || b == b'_')
                                    })
                                })
                        })
                })
        }
        "parental_apps_set" => {
            exact(op, &["opt", "user_id", "list"], &[])
                && op["list"].as_array().is_some_and(|a| {
                    a.len() <= 64
                        && !a.is_empty()
                        && a.iter().all(|c| {
                            exact(c, &["class_name", "enable", "time_quota", "app_list"], &[])
                                && c["class_name"]
                                    .as_str()
                                    .is_some_and(|s| safe_text(s, 64, false))
                                && c["enable"].is_boolean()
                                && int_between(&c["time_quota"], 0, 1440)
                                && c["app_list"].as_array().is_some_and(|a| {
                                    a.len() <= 128
                                        && a.iter().all(|app| {
                                            exact(app, &["name", "enable"], &[])
                                                && app["name"]
                                                    .as_str()
                                                    .is_some_and(|s| safe_text(s, 64, false))
                                                && app["enable"].is_boolean()
                                        })
                                })
                        })
                })
        }
        _ => false,
    };
    if ok { Ok(op.clone()) } else { Err(invalid()) }
}

/// Validate native form keys, typed values and operation-specific semantics.
/// Current-LAN containment, ACL self-lockout and live mark/qdisc conflicts belong
/// to the root preflight, where the authenticated manager and runtime are known.
pub fn validate(action_id: &str, input: &Value) -> Result<(), Error> {
    let action = DOMAIN
        .actions
        .iter()
        .find(|a| a.id == action_id)
        .ok_or_else(invalid)?;
    let object = input.as_object().ok_or_else(invalid)?;
    if object
        .keys()
        .any(|key| !action.fields.iter().any(|f| f.key == key))
    {
        return Err(invalid());
    }
    for f in action.fields {
        let Some(v) = object.get(f.key) else {
            if f.required {
                return Err(invalid());
            } else {
                continue;
            }
        };
        let ok = match f.kind {
            FieldKind::Integer => integral(v)
                .is_some_and(|n| f.min.is_none_or(|m| n >= m) && f.max.is_none_or(|m| n <= m)),
            FieldKind::Select => v.as_str().is_some_and(|s| f.options.contains(&s)),
            FieldKind::Boolean => v.is_boolean(),
            FieldKind::Ipv4 => v.as_str().is_some_and(ipv4_target),
            FieldKind::Mac => mac_value(v),
            FieldKind::Secret => v.as_str().is_some_and(|s| {
                (s.len() <= 512)
                    && (!f.required || !s.is_empty())
                    && !s.chars().any(char::is_control)
            }),
            FieldKind::Text => v.as_str().is_some_and(|s| safe_text(s, 253, !f.required)),
            FieldKind::Json => decoded(v).is_ok(),
            _ => false,
        };
        if !ok {
            return Err(invalid());
        }
    }
    match action_id {
        "forward_range_add" if integral(&input["fport"]) > integral(&input["tport"]) => {
            return Err(invalid());
        }
        "ddns_add" | "ddns_edit" => {
            if input.get("domain").is_some_and(|v| !host_value(v)) {
                return Err(invalid());
            }
            // Native setDdns returns false after saving a disabled new instance.
            // Use add enabled, then the explicit instance switch for disable.
            if action_id == "ddns_add" && integral(&input["enable"]) != Some(1) {
                return Err(invalid());
            }
        }
        "qos_limit" => {
            if integral(&input["mode"]) == Some(1)
                && (!int_between(&input["upload"], 1, 3) || !int_between(&input["download"], 1, 3))
            {
                return Err(invalid());
            }
        }
        "qos_limits" => {
            let list = decoded(&input["data"])?;
            let a = list.as_array().ok_or_else(invalid)?;
            let priority = integral(&input["mode"]) == Some(1);
            let mut seen = HashSet::new();
            if a.is_empty()
                || a.len() > 64
                || !a.iter().all(|v| {
                    exact(v, &["mac", "maxup", "maxdown"], &[])
                        && mac_value(&v["mac"])
                        && seen.insert(v["mac"].as_str().unwrap_or("").to_ascii_lowercase())
                        && int_between(
                            &v["maxup"],
                            if priority { 1 } else { 0 },
                            if priority { 3 } else { 100000000 },
                        )
                        && int_between(
                            &v["maxdown"],
                            if priority { 1 } else { 0 },
                            if priority { 3 } else { 100000000 },
                        )
                })
            {
                return Err(invalid());
            }
        }
        "qos_guest" => {
            for key in ["percent", "percent_up"] {
                if !number(&input[key]).is_some_and(|n| (0.0..=1.0).contains(&n)) {
                    return Err(invalid());
                }
            }
        }
        "web_access" => {
            if integral(&input["open"]) == Some(1)
                && (input.get("mac").is_none() || input.get("opt").is_none())
            {
                return Err(invalid());
            }
        }
        "router_name" => {
            if input["name"].as_str().unwrap_or("").len() > 27
                || input
                    .get("locale")
                    .is_some_and(|v| v.as_str().unwrap_or("").len() > 24)
            {
                return Err(invalid());
            }
        }
        "time_set" => {
            let index = input["index"].as_str().unwrap_or("");
            if index.is_empty()
                || index.len() > 16
                || !index.bytes().all(|b| b.is_ascii_digit() || b == b'.')
                || index.split('.').any(str::is_empty)
            {
                return Err(invalid());
            }
            if input
                .get("time")
                .is_some_and(|v| !v.as_str().is_some_and(|s| s.is_empty() || date_time(s)))
            {
                return Err(invalid());
            }
        }
        "led_set" | "eth_led_set" => {
            for key in ["timer_open", "timer_close"] {
                if input
                    .get(key)
                    .is_some_and(|v| !v.as_str().is_some_and(hhmm))
                {
                    return Err(invalid());
                }
            }
            if integral(&input["timer_on"]) == Some(1)
                && (input.get("timer_open").is_none()
                    || input.get("timer_close").is_none()
                    || input["timer_open"] == input["timer_close"])
            {
                return Err(invalid());
            }
        }
        "ntp_set" => {
            if !host_value(&input["server1"])
                || input
                    .get("server2")
                    .is_some_and(|v| !v.as_str().is_some_and(|s| s.is_empty() || host(s)))
            {
                return Err(invalid());
            }
        }
        "scheduled_reboot_set" => {
            let days = decoded(&input["weekdays"])?;
            if !input["time"].as_str().is_some_and(hhmm)
                || !unique_list(&days, 7, |v| int_between(v, 0, 6))
                || (input["enabled"] == true && days.as_array().is_none_or(Vec::is_empty))
            {
                return Err(invalid());
            }
        }
        id if pctl_op(id).is_some() => {
            parental_input(id, input)?;
        }
        _ => {}
    }
    Ok(())
}

/// Factory XQDDNS.editDdns already preserves blank/absent credentials.
/// Strip optional blank secrets rather than filling them from a public DTO.
/// Native-only fields in `current` stay private and never enter public projections.
pub fn prepare_input(action_id: &str, mut input: Value, _current: &Value) -> Result<Value, Error> {
    if action_id == "ddns_edit" {
        let m = input.as_object_mut().ok_or_else(invalid)?;
        for key in ["username", "password"] {
            if m.get(key).is_some_and(|v| v.as_str() == Some("")) {
                m.remove(key);
            }
        }
    }
    validate(action_id, &input)?;
    let action = DOMAIN
        .actions
        .iter()
        .find(|a| a.id == action_id)
        .ok_or_else(invalid)?;
    for f in action.fields.iter().filter(|f| f.kind == FieldKind::Json) {
        if let Some(value) = input.get(f.key) {
            let value = decoded(value)?;
            input[f.key] = value;
        }
    }
    Ok(input)
}

fn equal(a: &Value, b: &Value) -> bool {
    match (number(a), number(b)) {
        (Some(a), Some(b)) => (a - b).abs() < 0.005,
        _ => a == b && !a.is_null(),
    }
}
fn same_mac(a: &Value, b: &Value) -> bool {
    a.as_str()
        .zip(b.as_str())
        .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
}
fn matching_fields(input: &Value, after: &Value, keys: &[&str]) -> bool {
    keys.iter()
        .all(|k| input.get(*k).is_none_or(|v| equal(v, &after[*k])))
}
fn list_has_mac(after: &Value, key: &str, mac: &Value) -> Option<bool> {
    after.get(key)?.as_array().map(|a| {
        a.iter()
            .any(|v| same_mac(if v.is_object() { &v["mac"] } else { v }, mac))
    })
}
fn same_list(a: &Value, b: &Value) -> bool {
    let Some(a) = a.as_array() else { return false };
    let Some(b) = b.as_array() else { return false };
    a.len() == b.len() && a.iter().all(|a| b.iter().any(|b| equal(a, b)))
}
fn qos_row<'a>(after: &'a Value, mac: &Value) -> Option<&'a Value> {
    let dict = after.get("dict")?.as_object()?;
    dict.iter()
        .find(|(k, _)| mac.as_str().is_some_and(|m| k.eq_ignore_ascii_case(m)))
        .map(|(_, v)| v)
}
fn qos_match(after: &Value, mac: &Value, mode: &Value, up: &Value, down: &Value) -> bool {
    if !equal(&after["status"]["mode"], mode) {
        return false;
    }
    let Some(row) = qos_row(after, mac) else {
        return false;
    };
    if integral(mode) == Some(1) {
        // Native qosHistory exposes one downlink level. A single level cannot
        // prove two independent priorities, so unequal directions stay pending.
        equal(up, down) && equal(&row["level"], down)
    } else {
        equal(&row["upmax"], up) && equal(&row["downmax"], down)
    }
}

/// Private equality gate for credentials. The root must call this on the raw
/// getter before public projection when completing a DDNS credential change.
/// Presence flags alone do not prove that a requested credential was stored.
pub fn verify_private(action_id: &str, input: &Value, native_after: &Value) -> bool {
    if validate(action_id, input).is_err() {
        return false;
    }
    if !matches!(action_id, "ddns_add" | "ddns_edit") {
        return true;
    }
    ["username", "password"].iter().all(|key| {
        input
            .get(*key)
            .is_none_or(|v| v.as_str() == Some("") || native_after.get(*key) == Some(v))
    })
}

/// Compare the projected readback with the requested native change.
/// A false result means pending: the root must continue runtime verification,
/// handle a dedicated lifecycle result, or fail at its bounded deadline.
pub fn verify(action_id: &str, input: &Value, after: &Value) -> bool {
    if validate(action_id, input).is_err() || !after.is_object() {
        return false;
    }
    match action_id {
        "forward_add" | "forward_range_add" => after["list"].as_array().is_some_and(|rows| {
            rows.iter().any(|r| {
                equal(&r["name"], &input["name"])
                    && equal(&r["destip"], &input["ip"])
                    && equal(&r["proto"], &input["proto"])
                    && if action_id == "forward_add" {
                        equal(&r["ftype"], &json!(1))
                            && equal(&r["srcport"], &input["sport"])
                            && equal(&r["destport"], &input["dport"])
                    } else {
                        equal(&r["ftype"], &json!(2))
                            && equal(&r["srcport"]["f"], &input["fport"])
                            && equal(&r["srcport"]["t"], &input["tport"])
                    }
            })
        }),
        "forward_delete" => after["list"].as_array().is_some_and(|rows| {
            !rows.iter().any(|r| {
                equal(&r["proto"], &input["proto"])
                    && (equal(&r["srcport"], &input["port"])
                        || equal(&r["srcport"]["f"], &input["port"]))
            })
        }),
        "dmz_set" => equal(&after["status"], &json!(1)) && equal(&after["ip"], &input["ip"]),
        "dmz_off" => equal(&after["status"], &json!(0)),
        "upnp_switch" => equal(&after["status"], &input["switch"]),
        "ddns_add" | "ddns_edit" => {
            matching_fields(
                input,
                after,
                &[
                    "domain",
                    "checkinterval",
                    "forceinterval",
                    "wanindex",
                    "iptype",
                ],
            ) && ["username", "password"].iter().all(|k| {
                input.get(*k).is_none_or(|v| {
                    v.as_str() == Some("") || after[format!("{k}Configured")] == true
                })
            })
        }
        "ddns_delete" => after["list"]
            .as_array()
            .is_some_and(|a| !a.iter().any(|r| equal(&r["id"], &input["id"]))),
        "ddns_switch" => after["list"].as_array().is_some_and(|a| {
            a.iter()
                .any(|r| equal(&r["id"], &input["id"]) && equal(&r["enabled"], &input["on"]))
        }),
        "qos_switch" => equal(&after["status"]["on"], &input["on"]),
        "qos_mode" => equal(&after["status"]["mode"], &input["mode"]),
        "qos_band" => {
            equal(&after["band"]["upload"], &input["upload"])
                && equal(&after["band"]["download"], &input["download"])
        }
        "qos_limit" => qos_match(
            after,
            &input["mac"],
            &input["mode"],
            &input["upload"],
            &input["download"],
        ),
        "qos_limits" => decoded(&input["data"])
            .ok()
            .and_then(|v| v.as_array().cloned())
            .is_some_and(|a| {
                a.iter().all(|v| {
                    qos_match(after, &v["mac"], &input["mode"], &v["maxup"], &v["maxdown"])
                })
            }),
        "qos_device" => {
            equal(&after["limit"]["upmax"], &input["upload"])
                && equal(&after["limit"]["downmax"], &input["download"])
        }
        "qos_offlimit" => after["limit"]["flag"] == "off",
        "qos_guest" => {
            equal(&after["guest"]["percent"], &input["percent"])
                && equal(&after["guest"]["percent_up"], &input["percent_up"])
        }
        "access_switch" => {
            equal(&after["enable"], &input["enable"]) && equal(&after["model"], &input["model"])
        }
        "access_edit" | "access_add" => {
            equal(&after["model"], &input["model"])
                && list_has_mac(after, "macfilter", &input["mac"])
                    == Some(action_id == "access_add" || integral(&input["option"]) == Some(0))
        }
        "web_access" => {
            let open = integral(&input["open"]) == Some(1);
            after["open"] == open
                && (!open
                    || list_has_mac(after, "list", &input["mac"])
                        == Some(integral(&input["opt"]) == Some(0)))
        }
        "firewall_switch" => equal(&after["firewall_enable"], &input["firewall_enable"]),
        "spi_switch" => equal(&after["spi_firewall"], &input["spi_firewall"]),
        "dos_switch" => equal(&after["dos_firewall"], &input["dos_firewall"]),
        "wan_ping_switch" => equal(&after["wanping_firewall"], &input["wanping_firewall"]),
        "https_switch" => equal(&after["on"], &input["on"]),
        "gateway_security" => equal(&after["enable"], &input["on"]),
        "anti_rpfilter" => equal(&after["rpfilter"], &input["enable"]),
        "anti_dos" => equal(&after["dos"], &input["enable"]),
        "anti_scan" => equal(&after["scan"], &input["enable"]),
        "router_name" => matching_fields(input, after, &["name", "locale"]),
        "time_set" => {
            if !equal(&after["time"]["index"], &input["index"]) {
                return false;
            }
            let Some(s) = input
                .get("time")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            else {
                return true;
            };
            let vals = [
                (&s[..4], "year"),
                (&s[5..7], "month"),
                (&s[8..10], "day"),
                (&s[11..13], "hour"),
                (&s[14..16], "min"),
            ];
            vals.iter()
                .all(|(s, key)| number(&json!(s)) == number(&after["time"][*key]))
                && integral(&after["time"]["sec"])
                    .zip(s[17..].parse::<i64>().ok())
                    .is_some_and(|(after, before)| after >= before && after - before <= 10)
        }
        "led_set" | "eth_led_set" => {
            equal(&after["status"], &input["on"])
                && input
                    .get("timer_on")
                    .is_none_or(|on| equal(&after["timer_status"], on))
                && (integral(&input["timer_on"]) != Some(1)
                    || matching_fields(input, after, &["timer_open", "timer_close"]))
        }
        "all_led_set" => equal(&after["status"], &input["on"]),
        "ota_set" => equal(&after["auto"], &input["auto"]),
        "ntp_set" => {
            let mut want = vec![input["server1"].clone()];
            if let Some(v) = input
                .get("server2")
                .filter(|v| v.as_str().is_some_and(|s| !s.is_empty()))
            {
                want.push(v.clone());
            }
            after["servers"] == Value::Array(want)
        }
        "time_mode_set" => {
            equal(&after["info"]["mode"], &input["mode"]) && integral(&input["sync"]) != Some(1)
        } // sync needs actual time/NTP evidence, not getter's fixed sync=0.
        "scheduled_reboot_set" => {
            after["reloadPending"] == false
                && equal(&after["enabled"], &input["enabled"])
                && equal(&after["time"], &input["time"])
                && decoded(&input["weekdays"])
                    .is_ok_and(|days| same_list(&days, &after["weekdays"]))
        }
        id if pctl_op(id).is_some() => {
            let Ok(op) = parental_input(id, input) else {
                return false;
            };
            match id {
                "parental_user_add" => after["user_list"].as_array().is_some_and(|a| {
                    a.iter()
                        .any(|v| matching_fields(&op, v, &["user_name", "icon"]))
                }),
                "parental_user_edit" => after["user_list"].as_array().is_some_and(|a| {
                    a.iter().any(|v| {
                        equal(&v["user_id"], &op["user_id"])
                            && matching_fields(&op, v, &["user_name", "icon"])
                    })
                }),
                "parental_user_delete" => after["user_list"]
                    .as_array()
                    .is_some_and(|a| !a.iter().any(|v| equal(&v["user_id"], &op["user_id"]))),
                "parental_devices_set" => after["list"].as_array().is_some_and(|a| {
                    a.iter().any(|v| {
                        equal(&v["user_id"], &op["user_id"])
                            && same_list(&op["devices"], &v["devices"])
                    })
                }),
                "parental_hosts_set" => same_list(&op["list"], &after["list"]),
                "parental_temporary_set" => equal(&op["deny"], &after["deny"]),
                "parental_time_set" => op["time_list"]
                    .as_array()
                    .zip(after["time_list"].as_array())
                    .is_some_and(|(want, got)| {
                        want.len() == got.len()
                            && want.iter().all(|w| {
                                got.iter().any(|g| {
                                    matching_fields(w, g, &["start", "end"])
                                        && w["enable"] == g["enable"]
                                })
                            })
                    }),
                "parental_apps_set" => op["list"]
                    .as_array()
                    .zip(after["list"].as_array())
                    .is_some_and(|(want, got)| {
                        want.iter().all(|w| {
                            got.iter().any(|g| {
                                matching_fields(w, g, &["class_name", "enable", "time_quota"])
                                    && w["app_list"]
                                        .as_array()
                                        .zip(g["app_list"].as_array())
                                        .is_some_and(|(apps, got)| {
                                            apps.iter().all(|a| {
                                                got.iter().any(|g| {
                                                    matching_fields(a, g, &["name", "enable"])
                                                })
                                            })
                                        })
                            })
                        })
                    }),
                _ => false,
            }
        }
        // These getters cannot demonstrate a reload, a reboot, reset, or flash
        // lifecycle. A root runtime/maintenance job must verify them separately.
        "forward_apply" | "dmz_reload" | "ddns_reload" | "reboot" | "factory_reset"
        | "official_upgrade" => false,
        _ => false,
    }
}
