//! RN02 factory wireless, guest, IoT and Mesh adapters.
//!
//! Routes are not Lua function names: descriptors bind the globals assigned by
//! the factory controllers. The parent owns serialized IO, checkpoints and
//! factory lifecycle execution. In particular, setWifi(index=3) is a no-op;
//! the guest branch of setWifiWithoutRestart calls XQGuestWifi and applies its
//! network/DHCP/firewall lifecycle despite the route's misleading name.
//!
//! validate() checks the intent only. The executor must also check current
//! available_channels/widths, regional calibration, MLO support and BSD/AX
//! dependencies, TWT/AX compatibility, and Mesh role before applying. Runtime
//! association, guest isolation and MLD links need separate qualification.
use crate::features::{
    Action, Domain, Error, Field, FieldKind, Impact, Read, choice, field, integer, invalid,
};
use serde_json::{Map, Value, json};

const BIT: &[&str] = &["0", "1"];
const ENCRYPTION: &[&str] = &[
    "none",
    "psk",
    "mixed-psk",
    "psk2",
    "psk2+ccmp",
    "ccmp",
    "wep-open",
];
const POWER: &[&str] = &["min", "mid", "max"];
const WIDTH: &[&str] = &["0", "20", "40", "80", "160"];
const MODES: &[&str] = &[
    "11bgn", "11n", "11an", "11ac", "11ax", "11axg", "11axa", "11beg", "11bea",
];
const WIFI: &[&str] = &["wireless", "misc", "xiaoqiang"];
const MESH: &[&str] = &[
    "wireless",
    "misc",
    "xiaoqiang",
    "network",
    "dhcp",
    "firewall",
];
const GUEST: &[&str] = &["wireless", "misc", "network", "dhcp", "firewall", "miqos"];
const RADIO_INDEX: &[Field] = &[integer(
    "wifiIndex",
    "频段索引（1：2.4 GHz；2：5 GHz）",
    true,
    1,
    2,
)];
const RADIO_FIELDS: &[Field] = &[
    integer("wifiIndex", "频段索引", true, 1, 2),
    field("ssid", "Wi-Fi 名称", FieldKind::Text, false),
    field("pwd", "新密码", FieldKind::Secret, false),
    choice("encryption", "加密方式", false, ENCRYPTION),
    integer("channel", "信道（0：自动）", false, 0, 165),
    choice("bandwidth", "带宽（0：自动）", false, WIDTH),
    choice("txpwr", "发射功率", false, POWER),
    choice("hidden", "隐藏名称", false, BIT),
    choice("on", "启用", false, BIT),
    choice("txbf", "MU-MIMO / 波束赋形", false, &["0", "3"]),
    choice("weakenable", "弱信号限制", false, BIT),
    integer("weakthreshold", "拒绝关联阈值（dBm）", false, -100, 0),
    integer("kickthreshold", "断开阈值（dBm）", false, -100, 0),
    choice("wifimode", "协议模式", false, MODES),
];
const ALL_FIELDS: &[Field] = &[
    choice("bsd", "多频合一", true, BIT),
    choice("ver", "原厂显式频段设置版本", true, &["1"]),
    choice("user_confirm", "确认 DFS 等待", false, BIT),
    choice("on1", "2.4 GHz 启用", true, BIT),
    field("ssid1", "2.4 GHz 名称", FieldKind::Text, true),
    field("pwd1", "2.4 GHz 新密码", FieldKind::Secret, false),
    choice("encryption1", "2.4 GHz 加密", true, ENCRYPTION),
    integer("channel1", "2.4 GHz 信道", false, 0, 13),
    choice("bandwidth1", "2.4 GHz 带宽", false, &["0", "20", "40"]),
    choice("txpwr1", "2.4 GHz 功率", false, POWER),
    choice("hidden1", "2.4 GHz 隐藏名称", false, BIT),
    choice(
        "wifimode1",
        "2.4 GHz 协议模式",
        false,
        &["11bgn", "11n", "11ax", "11axg", "11beg"],
    ),
    choice("on2", "5 GHz 启用", true, BIT),
    field("ssid2", "5 GHz 名称", FieldKind::Text, true),
    field("pwd2", "5 GHz 新密码", FieldKind::Secret, false),
    choice("encryption2", "5 GHz 加密", true, ENCRYPTION),
    integer("channel2", "5 GHz 信道", false, 0, 165),
    choice("bandwidth2", "5 GHz 带宽", false, WIDTH),
    choice("txpwr2", "5 GHz 功率", false, POWER),
    choice("hidden2", "5 GHz 隐藏名称", false, BIT),
    choice(
        "wifimode2",
        "5 GHz 协议模式",
        false,
        &["11n", "11an", "11ac", "11ax", "11axa", "11bea"],
    ),
    choice("txbf", "MU-MIMO / 波束赋形", false, &["0", "3"]),
    choice("weakenable", "弱信号限制", false, BIT),
    integer("weakthreshold", "拒绝关联阈值（dBm）", false, -100, 0),
    integer("kickthreshold", "断开阈值（dBm）", false, -100, 0),
];
const GUEST_FIELDS: &[Field] = &[
    integer("wifiIndex", "原厂访客索引", true, 3, 3),
    field("ssid", "访客 Wi-Fi 名称", FieldKind::Text, true),
    field("pwd", "访客新密码", FieldKind::Secret, false),
    choice("encryption", "访客加密方式", true, ENCRYPTION),
    choice("on", "访客 Wi-Fi 启用", true, BIT),
];
const WEAK_FIELDS: &[Field] = &[
    integer("wifiIndex", "频段索引", true, 1, 2),
    choice("weakenable", "弱信号限制", true, BIT),
    integer("weakthreshold", "拒绝关联阈值（dBm）", false, -100, 0),
    integer("kickthreshold", "断开阈值（dBm）", false, -100, 0),
];
const ON: &[Field] = &[choice("on", "启用", true, BIT)];
const MAC: &[Field] = &[field("mac", "Mesh 节点 MAC", FieldKind::Mac, true)];
const READS: &[Read] = &[
    Read {
        id: "wifi_detail_all",
        title: "无线配置与频段能力",
        controller: "xqnetwork",
        handler: "getAllWifiInfo",
        fields: &[],
    },
    Read {
        id: "wifi_detail",
        title: "单频段配置",
        controller: "xqnetwork",
        handler: "getWifiInfo",
        fields: RADIO_INDEX,
    },
    Read {
        id: "wifi_status",
        title: "无线运行状态",
        controller: "xqnetwork",
        handler: "getWifiStatus",
        fields: &[],
    },
    Read {
        id: "wifi_txpwr_channel",
        title: "信道与功率",
        controller: "xqnetwork",
        handler: "getWifiChTx",
        fields: &[],
    },
    Read {
        id: "wifi_connect_devices",
        title: "无线关联终端",
        controller: "xqnetwork",
        handler: "getWifiConDev",
        fields: &[],
    },
    Read {
        id: "get_hostap_mlo",
        title: "MLO 能力与配置",
        controller: "xqnetwork",
        handler: "getHostapMLO",
        fields: &[],
    },
    Read {
        id: "get_twt",
        title: "TWT 配置",
        controller: "xqnetwork",
        handler: "getTwt",
        fields: &[],
    },
    Read {
        id: "get_wifi_weak",
        title: "弱信号阈值",
        controller: "xqnetwork",
        handler: "getWifiWeakInfo",
        fields: &[],
    },
    Read {
        id: "get_miotrelay_switch",
        title: "IoT 用户开关",
        controller: "xqnetwork",
        handler: "getMiotrelaySwitch",
        fields: &[],
    },
    Read {
        id: "get_miscan_switch",
        title: "IoT 扫描开关",
        controller: "xqnetwork",
        handler: "getMiscanSwitch",
        fields: &[],
    },
    Read {
        id: "get_mesh_switch",
        title: "Mesh 开关与模式",
        controller: "xqnetwork",
        handler: "getMeshSwitch",
        fields: &[],
    },
    Read {
        id: "topo_graph",
        title: "Mesh 拓扑",
        controller: "misystem",
        handler: "getTopoGraph",
        fields: &[choice("simplified", "简化拓扑", false, BIT)],
    },
    Read {
        id: "get_mesh_bh_mode",
        title: "Mesh 回程模式",
        controller: "misystem",
        handler: "getMeshBhMode",
        fields: &[],
    },
    Read {
        id: "get_addnode_status",
        title: "Mesh 加入进度",
        controller: "xqnetwork",
        handler: "getMeshNodeStatus",
        fields: MAC,
    },
    Read {
        id: "wifi_share_info",
        title: "访客配置与状态",
        controller: "misns",
        handler: "wifiShareInfo",
        fields: &[integer("guest_index", "访客频段索引", false, 1, 2)],
    },
    Read {
        id: "guest_qos",
        title: "访客带宽比例",
        controller: "misystem",
        handler: "getQosInfo",
        fields: &[],
    },
];
const ACTIONS: &[Action] = &[
    Action {
        id: "set_wifi",
        title: "配置单频段 Wi-Fi",
        controller: "xqnetwork",
        handler: "setWifi",
        fields: RADIO_FIELDS,
        impact: Impact::Wireless,
        configs: WIFI,
        readback: "wifi_detail_all",
    },
    Action {
        id: "set_all_wifi",
        title: "配置双频与多频合一",
        controller: "xqnetwork",
        handler: "setAllWifi",
        fields: ALL_FIELDS,
        impact: Impact::Wireless,
        configs: WIFI,
        readback: "wifi_detail_all",
    },
    Action {
        id: "set_wifi_txpwr",
        title: "调整双频发射功率",
        controller: "xqnetwork",
        handler: "setWifiTxpwr",
        fields: &[choice("txpwr", "发射功率", true, POWER)],
        impact: Impact::Wireless,
        configs: WIFI,
        readback: "wifi_txpwr_channel",
    },
    Action {
        id: "set_wifi_ax",
        title: "Wi-Fi 5 兼容模式",
        controller: "xqnetwork",
        handler: "setWifiAx",
        fields: &[
            choice("ax", "AX 启用（0：兼容模式）", true, BIT),
            choice("user_confirm", "确认 DFS 等待", false, BIT),
        ],
        impact: Impact::Wireless,
        configs: WIFI,
        readback: "wifi_detail_all",
    },
    Action {
        id: "set_wifi_txbf",
        title: "MU-MIMO / 波束赋形",
        controller: "xqnetwork",
        handler: "setWifiTxbf",
        fields: &[
            choice("txbf", "启用", true, &["0", "3"]),
            choice("user_confirm", "确认 DFS 等待", false, BIT),
        ],
        impact: Impact::Wireless,
        configs: WIFI,
        readback: "wifi_detail_all",
    },
    Action {
        id: "set_hostap_mlo",
        title: "设置 MLO",
        controller: "xqnetwork",
        handler: "setHostapMLO",
        fields: &[choice("mlo_enable", "启用 MLO", true, BIT)],
        impact: Impact::Wireless,
        configs: WIFI,
        readback: "get_hostap_mlo",
    },
    Action {
        id: "set_twt",
        title: "设置 TWT 节能",
        controller: "xqnetwork",
        handler: "setTwt",
        fields: ON,
        impact: Impact::Wireless,
        configs: WIFI,
        readback: "get_twt",
    },
    Action {
        id: "set_guest_wifi",
        title: "配置访客 Wi-Fi",
        controller: "xqnetwork",
        handler: "setWifiWithoutRestart",
        fields: GUEST_FIELDS,
        impact: Impact::Wireless,
        configs: GUEST,
        readback: "wifi_share_info",
    },
    Action {
        id: "qos_guest",
        title: "设置访客带宽比例",
        controller: "misystem",
        handler: "qosGuest",
        fields: &[
            field(
                "percent",
                "下载比例（0–1；0 为不限制）",
                FieldKind::Text,
                true,
            ),
            field("percent_up", "上传比例（0–1）", FieldKind::Text, false),
        ],
        impact: Impact::Network,
        configs: &["miqos"],
        readback: "guest_qos",
    },
    Action {
        id: "miotrelay_switch",
        title: "设置 IoT relay 用户开关",
        controller: "xqnetwork",
        handler: "miotrelaySwitch",
        fields: ON,
        impact: Impact::Wireless,
        configs: WIFI,
        readback: "get_miotrelay_switch",
    },
    Action {
        id: "miscan_switch",
        title: "设置 IoT 扫描",
        controller: "xqnetwork",
        handler: "miscanSwitch",
        fields: ON,
        impact: Impact::Wireless,
        configs: &["miscan", "wireless", "misc"],
        readback: "get_miscan_switch",
    },
    Action {
        id: "scan_mesh_node",
        title: "扫描可加入的 Mesh 节点",
        controller: "xqnetwork",
        handler: "scanMeshNode",
        fields: &[],
        impact: Impact::Wireless,
        configs: MESH,
        readback: "",
    },
    Action {
        id: "add_mesh_node",
        title: "加入 Mesh 节点",
        controller: "xqnetwork",
        handler: "addMeshNode",
        fields: &[
            field("mac", "节点 MAC", FieldKind::Mac, true),
            field("locate", "节点位置", FieldKind::Text, true),
        ],
        impact: Impact::Wireless,
        configs: MESH,
        readback: "get_addnode_status",
    },
    Action {
        id: "set_mesh_switch",
        title: "设置 Mesh 开关",
        controller: "xqnetwork",
        handler: "setMeshSwitch",
        fields: ON,
        impact: Impact::Wireless,
        configs: &["xiaoqiang", "wireless", "misc"],
        readback: "get_mesh_switch",
    },
    Action {
        id: "set_mesh_bh_mode",
        title: "设置 Mesh 回程模式",
        controller: "misystem",
        handler: "setMeshBhMode",
        fields: &[choice("bhmode", "回程模式", true, &["auto", "wired"])],
        impact: Impact::Wireless,
        configs: WIFI,
        readback: "get_mesh_bh_mode",
    },
    Action {
        id: "set_wifi_weak",
        title: "设置漫游弱信号阈值",
        controller: "xqnetwork",
        handler: "setWifiWeakInfo",
        fields: WEAK_FIELDS,
        impact: Impact::Wireless,
        configs: WIFI,
        readback: "get_wifi_weak",
    },
];
pub const DOMAIN: Domain = Domain {
    id: "wireless",
    title: "无线、Wi-Fi 7 与 Mesh",
    reads: READS,
    actions: ACTIONS,
};

fn bad_reply() -> Error {
    Error {
        status: 502,
        code: "vendor_invalid_response",
        message: "原厂无线服务响应无效。",
    }
}
fn object(v: &Value) -> Result<&Map<String, Value>, Error> {
    v.as_object().ok_or_else(bad_reply)
}
fn scalar(v: &Value) -> bool {
    v.is_null() || v.is_string() || v.is_boolean() || v.is_number()
}
/// Every leaf is shape-checked. Merely picking an allowed outer key could leak
/// a secret inside an unexpected object supplied as e.g. ssid or signal.
fn leaves(v: &Value, keys: &[&str]) -> Result<Value, Error> {
    let source = object(v)?;
    let mut out = Map::new();
    for key in keys {
        if let Some(value) = source.get(*key) {
            if !scalar(value) {
                return Err(bad_reply());
            }
            out.insert((*key).into(), value.clone());
        }
    }
    Ok(Value::Object(out))
}
fn array(
    v: &Value,
    max: usize,
    project: fn(&Value) -> Result<Value, Error>,
) -> Result<Value, Error> {
    // cjson encodes an empty Lua table as {}, not always [].
    if v.as_object().is_some_and(Map::is_empty) {
        return Ok(json!([]));
    }
    let items = v.as_array().ok_or_else(bad_reply)?;
    if items.len() > max {
        return Err(bad_reply());
    }
    items
        .iter()
        .map(project)
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}
fn scalar_array(v: &Value) -> Result<Value, Error> {
    array(v, 256, |item| {
        if scalar(item) {
            Ok(item.clone())
        } else {
            Err(bad_reply())
        }
    })
}
fn password_presence(source: &Value, target: &mut Value) -> Result<(), Error> {
    if let Some(password) = source.get("password") {
        if !(password.is_null() || password.is_string()) {
            return Err(bad_reply());
        }
        target["passwordConfigured"] = json!(password.as_str().is_some_and(|s| !s.is_empty()));
    }
    Ok(())
}
fn channel(v: &Value) -> Result<Value, Error> {
    let mut out = leaves(v, &["c"])?;
    if let Some(b) = v.get("b") {
        out["b"] = scalar_array(b)?;
    }
    Ok(out)
}
fn radio(v: &Value) -> Result<Value, Error> {
    let mut out = leaves(
        v,
        &[
            "channel",
            "bandwidth",
            "ssid",
            "ssidHtmlEncode",
            "status",
            "ifname",
            "wifimode",
            "device",
            "mode",
            "hidden",
            "signal",
            "encryption",
            "txpwr",
            "bsd",
            "txbf",
            "ax",
            "weakenable",
            "weakthreshold",
            "kickthreshold",
            "ssid_len_limit",
        ],
    )?;
    password_presence(v, &mut out)?;
    if let Some(info) = v.get("channelInfo") {
        let mut channel_info = leaves(info, &["channel", "bandwidth"])?;
        if let Some(b) = info.get("bandList") {
            channel_info["bandList"] = scalar_array(b)?;
        }
        out["channelInfo"] = channel_info;
    }
    if let Some(channels) = v.get("available_channels") {
        out["available_channels"] = array(channels, 256, channel)?;
    }
    Ok(out)
}
fn weak(v: &Value) -> Result<Value, Error> {
    leaves(v, &["weakenable", "weakthreshold", "kickthreshold"])
}
fn topology(v: &Value, depth: usize, remaining: &mut usize) -> Result<Value, Error> {
    if depth > 16 || *remaining == 0 {
        return Err(bad_reply());
    }
    *remaining -= 1;
    let mut out = leaves(
        v,
        &[
            "ip",
            "name",
            "locale",
            "hardware",
            "channel",
            "mode",
            "version",
            "ssid",
            "color",
            "signal",
            "link_type",
            "internet",
            "onlines",
            "renumber",
            "mac",
            "mac5G",
            "needConvert",
        ],
    )?;
    if let Some(leafs) = v.get("leafs") {
        let list = leafs.as_array().ok_or_else(bad_reply)?;
        if list.len() > 256 {
            return Err(bad_reply());
        }
        out["leafs"] = Value::Array(
            list.iter()
                .map(|n| topology(n, depth + 1, remaining))
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    Ok(out)
}
fn success(v: &Value) -> Result<(), Error> {
    object(v)?;
    match v.get("code").and_then(Value::as_i64) {
        Some(0) => Ok(()),
        Some(_) => Err(Error {
            status: 409,
            code: "vendor_rejected",
            message: "原厂无线服务未接受该请求。",
        }),
        None => Err(bad_reply()),
    }
}
fn required<'a>(v: &'a Value, key: &str) -> Result<&'a Value, Error> {
    v.get(key).ok_or_else(bad_reply)
}

/// Public DTOs keep the native field names. Unknown fields at every depth are
/// omitted; native credentials never leave this module, only their presence.
pub fn project(read_id: &str, value: Value) -> Result<Value, Error> {
    let known = READS.iter().any(|r| r.id == read_id) || ACTIONS.iter().any(|a| a.id == read_id);
    if !known {
        return Err(Error {
            status: 404,
            code: "unknown_feature_read",
            message: "无线功能不存在。",
        });
    }
    success(&value)?;
    let mut out = json!({"code":0});
    match read_id {
        "wifi_detail_all" => {
            out["info"] = array(required(&value, "info")?, 2, radio)?;
            let meta = leaves(&value, &["bsd", "dwb_type", "dwb_band", "dwb_status"])?;
            out.as_object_mut()
                .unwrap()
                .extend(meta.as_object().unwrap().clone());
        }
        "wifi_detail" => out["info"] = radio(required(&value, "info")?)?,
        "wifi_status" => {
            out["status"] = array(required(&value, "status")?, 2, |v| {
                leaves(v, &["ssid", "up"])
            })?
        }
        "wifi_txpwr_channel" => {
            out["list"] = array(required(&value, "list")?, 2, |v| {
                leaves(v, &["channel", "txpwr"])
            })?
        }
        "wifi_connect_devices" => {
            out["list"] = array(required(&value, "list")?, 512, |v| {
                leaves(v, &["mac", "signal", "wifiIndex"])
            })?
        }
        "get_wifi_weak" => out["info"] = array(required(&value, "info")?, 2, weak)?,
        "get_hostap_mlo" => {
            out = leaves(&value, &["code", "mlo_support", "mlo_enable"])?;
            required(&out, "mlo_support")?;
            required(&out, "mlo_enable")?;
        }
        "get_twt" => {
            out = leaves(&value, &["code", "status"])?;
            required(&out, "status")?;
        }
        "get_miotrelay_switch" | "get_miscan_switch" | "get_mesh_switch" => {
            out = leaves(&value, &["code", "enabled"])?;
            required(&out, "enabled")?;
        }
        "topo_graph" => {
            out = leaves(&value, &["code", "show"])?;
            out["graph"] = topology(required(&value, "graph")?, 0, &mut 256)?;
        }
        "get_mesh_bh_mode" => {
            out = leaves(&value, &["code", "bhmode"])?;
            required(&out, "bhmode")?;
        }
        "get_addnode_status" => {
            let status = required(&value, "status")?
                .as_i64()
                .filter(|n| (0..=4).contains(n))
                .ok_or_else(bad_reply)?;
            out["status"] = json!(status);
            out["pending"] = json!((1..=3).contains(&status));
            out["complete"] = json!(status == 0);
            out["failed"] = json!(status == 4);
        }
        "wifi_share_info" => {
            out = leaves(&value, &["code", "closingTime"])?;
            let native = required(&value, "info")?;
            let mut info = leaves(native, &["guest", "share", "need"])?;
            let data = required(native, "data")?;
            let mut public_data =
                leaves(data, &["ssid", "encryption", "hidden", "ssidHtmlEncode"])?;
            password_presence(data, &mut public_data)?;
            info["data"] = public_data;
            out["info"] = info;
        }
        "guest_qos" => {
            out["guest"] = leaves(
                required(&value, "guest")?,
                &["percent", "percent_up", "UP", "DOWN"],
            )?
        }
        "scan_mesh_node" => {
            out["list"] = array(required(&value, "list")?, 128, |v| {
                leaves(v, &["mac", "obssid", "ssid", "rssi", "mesh_ver"])
            })?;
            out["complete"] = json!(true);
            out["pending"] = json!(false);
        }
        "add_mesh_node" => {
            // Acknowledgement is not join success. Poll get_addnode_status with
            // the same MAC; native 1/2/3 are init/connected/syncd, 4 is failure.
            out["pending"] = json!(true);
            out["complete"] = json!(false);
            out["readback"] = json!("get_addnode_status");
            out["pendingStatuses"] = json!([1, 2, 3]);
            out["successStatus"] = json!(0);
            out["failureStatus"] = json!(4);
        }
        _ => {
            // Setter replies expose DFS timing only, never native msg/errorDetails.
            out = leaves(&value, &["code", "cac_time", "need_confirm"])?;
        }
    }
    Ok(out)
}

fn text<'a>(v: &'a Value, key: &str) -> Result<Option<&'a str>, Error> {
    v.get(key)
        .map(|n| n.as_str().ok_or_else(invalid))
        .transpose()
}
fn number(v: &Value) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}
fn integer_value(v: &Value, key: &str) -> Result<Option<i64>, Error> {
    v.get(key)
        .map(|n| number(n).ok_or_else(invalid))
        .transpose()
}
fn fraction(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_str().and_then(|s| s.parse::<f64>().ok()))
        .filter(|n| n.is_finite() && (0.0..=1.0).contains(n))
}
fn ssid(s: &str, limit: usize) -> bool {
    !s.is_empty() && s.len() <= limit && !s.chars().any(char::is_control)
}
fn passphrase(p: &str, encryption: &str) -> bool {
    if encryption == "none" {
        return p.is_empty();
    }
    // Factory checkWifiPasswd accepts 8..63 bytes, not a 64-hex PSK.
    p.is_ascii()
        && !p.bytes().any(|b| b < 32 || b == 127)
        && if encryption == "wep-open" {
            p.len() == 5 || p.len() == 13
        } else {
            (8..=63).contains(&p.len())
        }
}
fn credentials(v: &Value, suffix: &str, limit: usize, full: bool) -> Result<(), Error> {
    let s = format!("ssid{suffix}");
    let e = format!("encryption{suffix}");
    let p = format!("pwd{suffix}");
    if let Some(name) = text(v, &s)? {
        if !ssid(name, limit) {
            return Err(invalid());
        }
    } else if full {
        return Err(invalid());
    }
    let encryption = text(v, &e)?;
    let password = text(v, &p)?;
    match (encryption, password) {
        (Some(e), Some(p)) if ENCRYPTION.contains(&e) && passphrase(p, e) => Ok(()),
        (Some("none"), None) => Ok(()),
        (None, None) if !full => Ok(()),
        _ => Err(invalid()),
    }
}
fn radio_semantics(v: &Value, suffix: &str, index: i64) -> Result<(), Error> {
    if let Some(channel) = integer_value(v, &format!("channel{suffix}"))? {
        let valid = channel == 0
            || if index == 1 {
                (1..=13).contains(&channel)
            } else {
                ((36..=64).contains(&channel) || (100..=144).contains(&channel)) && channel % 4 == 0
                    || [149, 153, 157, 161, 165].contains(&channel)
            };
        if !valid {
            return Err(invalid());
        }
        if channel == 165
            && text(v, &format!("bandwidth{suffix}"))?.is_some_and(|w| w != "20" && w != "0")
        {
            return Err(invalid());
        }
    }
    if index == 1
        && text(v, &format!("bandwidth{suffix}"))?.is_some_and(|w| !matches!(w, "0" | "20" | "40"))
    {
        return Err(invalid());
    }
    if let Some(mode) = text(v, &format!("wifimode{suffix}"))?
        && (index == 1 && matches!(mode, "11an" | "11ac" | "11axa" | "11bea")
            || index == 2 && matches!(mode, "11bgn" | "11axg" | "11beg"))
    {
        return Err(invalid());
    }
    Ok(())
}
fn weak_semantics(v: &Value) -> Result<(), Error> {
    let weak = integer_value(v, "weakthreshold")?;
    let kick = integer_value(v, "kickthreshold")?;
    for threshold in [weak, kick].into_iter().flatten() {
        if !(-100..=0).contains(&threshold) {
            return Err(invalid());
        }
    }
    if text(v, "weakenable")? == Some("1")
        && (weak.is_none() || kick.is_none() || weak == Some(0) || kick == Some(0))
    {
        return Err(invalid());
    }
    if let (Some(w), Some(k)) = (weak, kick)
        && w != 0
        && k != 0
        && k > w
    {
        return Err(invalid());
    }
    Ok(())
}
fn mac(s: &str) -> bool {
    let parts = s.split(':').collect::<Vec<_>>();
    parts.len() == 6
        && parts
            .iter()
            .all(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_hexdigit()))
        && u8::from_str_radix(parts[0], 16).is_ok_and(|b| b & 1 == 0)
        && parts.iter().any(|p| *p != "00")
}
/// Semantic checks supplement root generic field validation. The small shape
/// checks here also make direct calls fail closed rather than panic.
pub fn validate(action_id: &str, input: &Value) -> Result<(), Error> {
    let action = ACTIONS
        .iter()
        .find(|a| a.id == action_id)
        .ok_or_else(invalid)?;
    let obj = input.as_object().ok_or_else(invalid)?;
    if obj
        .keys()
        .any(|k| !action.fields.iter().any(|f| f.key == k))
    {
        return Err(invalid());
    }
    for f in action.fields {
        let Some(v) = obj.get(f.key) else {
            if f.required {
                return Err(invalid());
            }
            continue;
        };
        let ok = match f.kind {
            FieldKind::Select => v.as_str().is_some_and(|s| f.options.contains(&s)),
            FieldKind::Integer => number(v).is_some_and(|n| {
                f.min.is_none_or(|min| n >= min) && f.max.is_none_or(|max| n <= max)
            }),
            FieldKind::Text | FieldKind::Secret | FieldKind::Mac => v.is_string(),
            _ => false,
        };
        if !ok {
            return Err(invalid());
        }
    }
    match action_id {
        "set_wifi" => {
            let index = integer_value(input, "wifiIndex")?.ok_or_else(invalid)?;
            if obj.len() < 2 {
                return Err(invalid());
            }
            credentials(input, "", if index == 1 { 28 } else { 31 }, false)?;
            radio_semantics(input, "", index)?;
            weak_semantics(input)?;
        }
        "set_all_wifi" => {
            credentials(input, "1", 28, true)?;
            credentials(input, "2", 31, true)?;
            radio_semantics(input, "1", 1)?;
            radio_semantics(input, "2", 2)?;
            weak_semantics(input)?;
            if text(input, "bsd")? == Some("1") {
                for key in ["ssid", "encryption", "pwd", "hidden", "on"] {
                    let one = input.get(format!("{key}1"));
                    let two = input.get(format!("{key}2"));
                    if one != two {
                        return Err(invalid());
                    }
                }
            }
        }
        "set_guest_wifi" => {
            credentials(input, "", 31, true)?;
        }
        "set_wifi_weak" => {
            weak_semantics(input)?;
        }
        "qos_guest" => {
            if input.get("percent").and_then(fraction).is_none()
                || input
                    .get("percent_up")
                    .is_some_and(|v| fraction(v).is_none())
            {
                return Err(invalid());
            }
        }
        "add_mesh_node" => {
            if !text(input, "mac")?.is_some_and(mac) {
                return Err(invalid());
            }
            // mesh_ver4_add_node embeds locate in both shell single quotes and
            // JSON double quotes without escaping. Permit human labels, not syntax.
            let locate = text(input, "locate")?.ok_or_else(invalid)?;
            if locate.len() > 64
                || !locate
                    .chars()
                    .all(|c| c.is_alphanumeric() || matches!(c, ' ' | '_' | '-'))
            {
                return Err(invalid());
            }
        }
        _ => {}
    }
    Ok(())
}

fn unhtml(s: &str) -> String {
    // Factory encode4HtmlValue encodes ampersands first. Decode ampersands last
    // exactly once, so a literal "&lt;" password never becomes "<".
    s.replace("&#039;", "'")
        .replace("&quot;", "\"")
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&amp;", "&")
}
fn native_password(native: &Value) -> Result<String, Error> {
    let password = native
        .get("password")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    Ok(if number(&native["ssidHtmlEncode"]) == Some(1) {
        unhtml(password)
    } else {
        password.to_owned()
    })
}
fn fill_password(input: &mut Value, suffix: &str, native: &Value) -> Result<(), Error> {
    let pwd = format!("pwd{suffix}");
    let encryption = format!("encryption{suffix}");
    let preserve = input.get(&pwd).is_none() || input.get(&pwd).and_then(Value::as_str) == Some("");
    if input.get(&encryption).and_then(Value::as_str) == Some("none") {
        input[pwd.as_str()] = json!("");
        return Ok(());
    }
    if preserve {
        let current = native_password(native)?;
        if current.is_empty() {
            return Err(invalid());
        }
        input[pwd.as_str()] = json!(current);
    }
    Ok(())
}
fn capability_check(input: &Value, suffix: &str, native: &Value, index: i64) -> Result<(), Error> {
    if let Some(name) = input.get(format!("ssid{suffix}")).and_then(Value::as_str)
        && let Some(limit) = native.get("ssid_len_limit").and_then(number)
        && (limit <= 0 || name.len() > limit as usize)
    {
        return Err(invalid());
    }
    // setWifi does not propagate the unified identity to the other BSS.
    // Changed common fields therefore belong to the setAllWifi owner.
    if suffix.is_empty() && native.get("bsd").and_then(number) == Some(1) {
        for (key, native_key) in [
            ("on", "status"),
            ("encryption", "encryption"),
            ("hidden", "hidden"),
        ] {
            if input
                .get(key)
                .is_some_and(|v| !equal(v, native.get(native_key)))
            {
                return Err(invalid());
            }
        }
        if let Some(name) = input.get("ssid").and_then(Value::as_str) {
            let current = native
                .get("ssid")
                .and_then(Value::as_str)
                .ok_or_else(invalid)?;
            let current = if number(&native["ssidHtmlEncode"]) == Some(1) {
                unhtml(current)
            } else {
                current.to_owned()
            };
            if name != current {
                return Err(invalid());
            }
        }
        if let Some(password) = input
            .get("pwd")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            && password != native_password(native)?
        {
            return Err(invalid());
        }
    }
    let channel_key = format!("channel{suffix}");
    let width_key = format!("bandwidth{suffix}");
    if input.get(&channel_key).is_none() && input.get(&width_key).is_none() {
        return Ok(());
    }
    // Do not treat a syntactically valid channel as regionally permitted.
    let channel = input
        .get(&channel_key)
        .or_else(|| native.get("channel"))
        .and_then(number)
        .ok_or_else(invalid)?;
    let allowed = native
        .get("available_channels")
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    let entry = allowed
        .iter()
        .find(|item| item.get("c").and_then(number) == Some(channel))
        .ok_or_else(invalid)?;
    if let Some(width) = input.get(&width_key).and_then(Value::as_str)
        && width != "0"
        && !entry
            .get("b")
            .and_then(Value::as_array)
            .is_some_and(|list| list.iter().any(|n| n.as_str() == Some(width)))
    {
        return Err(invalid());
    }
    radio_semantics(input, suffix, index)
}
/// Current-state dependencies need a separate getAllWifiInfo response because
/// the MLO/TWT getters do not contain BSD/AX state. This function never derives
/// hardware capability from a generic template or a successful setter reply.
pub fn validate_environment(
    action_id: &str,
    input: &Value,
    wifi_current: &Value,
) -> Result<(), Error> {
    if !matches!(
        action_id,
        "set_wifi" | "set_all_wifi" | "set_hostap_mlo" | "set_twt"
    ) {
        return Ok(());
    }
    success(wifi_current)?;
    let radios = wifi_current
        .get("info")
        .and_then(Value::as_array)
        .filter(|a| a.len() == 2)
        .ok_or_else(invalid)?;
    match action_id {
        "set_wifi" => {
            let index = integer_value(input, "wifiIndex")?
                .filter(|n| (1..=2).contains(n))
                .ok_or_else(invalid)?;
            capability_check(input, "", &radios[(index - 1) as usize], index)?;
        }
        "set_all_wifi" => {
            for (i, radio) in radios.iter().enumerate() {
                capability_check(input, &(i + 1).to_string(), radio, (i + 1) as i64)?;
            }
        }
        "set_hostap_mlo" if input.get("mlo_enable").and_then(Value::as_str) == Some("1") => {
            if wifi_current.get("bsd").and_then(number) != Some(1)
                || !radios
                    .iter()
                    .all(|r| r.get("bsd").and_then(number) == Some(1))
            {
                return Err(invalid());
            }
            if !radios.iter().all(|r| {
                r.get("ax").and_then(number) == Some(1)
                    || matches!(
                        r.get("wifimode").and_then(Value::as_str),
                        Some("11beg" | "11bea")
                    )
            }) {
                return Err(invalid());
            }
            if radios
                .iter()
                .any(|r| r.get("status").and_then(number) != Some(1))
            {
                return Err(invalid());
            }
        }
        "set_twt" if input.get("on").and_then(Value::as_str) == Some("1") => {
            if !radios.iter().all(|r| {
                r.get("ax").and_then(number) == Some(1)
                    || matches!(
                        r.get("wifimode").and_then(Value::as_str),
                        Some("11beg" | "11bea")
                    )
            }) {
                return Err(invalid());
            }
        }
        _ => {}
    }
    Ok(())
}

/// Populate omitted/empty optional passwords from the PRIVATE native readback.
/// current is the unprojected response for Action.readback; a presence-only DTO
/// is not accepted as a credential source. Root must never log/serialize the
/// prepared intent into a public response. Blank means preserve; selecting
/// encryption=none explicitly clears the credential.
pub fn prepare_input(action_id: &str, mut input: Value, current: &Value) -> Result<Value, Error> {
    if !input.is_object() {
        return Err(invalid());
    }
    match action_id {
        "set_wifi" => {
            success(current)?;
            let index = integer_value(&input, "wifiIndex")?
                .filter(|n| (1..=2).contains(n))
                .ok_or_else(invalid)?;
            let native = current
                .get("info")
                .and_then(Value::as_array)
                .and_then(|a| a.get((index - 1) as usize))
                .ok_or_else(invalid)?;
            // pwd alone has no effect in setWifiBasicInfo: always supply the
            // current encryption when changing only the secret.
            if input.get("pwd").is_some() && input.get("encryption").is_none() {
                input["encryption"] = native.get("encryption").cloned().ok_or_else(invalid)?;
            }
            if input.get("encryption").is_some() {
                fill_password(&mut input, "", native)?;
            }
            capability_check(&input, "", native, index)?;
        }
        "set_all_wifi" => {
            success(current)?;
            let radios = current
                .get("info")
                .and_then(Value::as_array)
                .filter(|a| a.len() == 2)
                .ok_or_else(invalid)?;
            let preserve_second = input.get("pwd2").is_none()
                || input.get("pwd2").and_then(Value::as_str) == Some("");
            for (i, native) in radios.iter().enumerate() {
                let suffix = (i + 1).to_string();
                fill_password(&mut input, &suffix, native)?;
                capability_check(&input, &suffix, native, (i + 1) as i64)?;
            }
            // The factory copies first-band credentials in BSD mode. Preserve
            // that exact behavior, but never replace an explicit second secret.
            if input.get("bsd").and_then(Value::as_str) == Some("1") && preserve_second {
                input["pwd2"] = input["pwd1"].clone();
            }
        }
        "set_guest_wifi" => {
            success(current)?;
            let native = current.pointer("/info/data").ok_or_else(invalid)?;
            fill_password(&mut input, "", native)?;
        }
        "set_hostap_mlo" => {
            success(current)?;
            if input.get("mlo_enable").and_then(Value::as_str) == Some("1")
                && current.get("mlo_support").and_then(number) != Some(1)
            {
                return Err(invalid());
            }
            // getHostapMLO has no AX/BSD fields. Those dependencies need the
            // executor's additional private wifi_detail_all capability read.
        }
        "set_twt" => {
            success(current)?;
        }
        _ => {}
    }
    validate(action_id, &input)?;
    Ok(input)
}

fn equal(expected: &Value, actual: Option<&Value>) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    expected == actual
        || number(expected)
            .zip(number(actual))
            .is_some_and(|(a, b)| a == b)
}
fn match_fields(input: &Value, after: &Value, pairs: &[(&str, &str)]) -> bool {
    pairs
        .iter()
        .all(|(from, to)| input.get(*from).is_none_or(|v| equal(v, after.get(*to))))
}
fn html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#039;")
}
fn verify_credentials(input: &Value, suffix: &str, after: &Value) -> bool {
    let ssid_key = format!("ssid{suffix}");
    let enc_key = format!("encryption{suffix}");
    let pwd_key = format!("pwd{suffix}");
    if let Some(s) = input.get(&ssid_key).and_then(Value::as_str) {
        let wanted = if number(&after["ssidHtmlEncode"]) == Some(1) {
            html(s)
        } else {
            s.to_owned()
        };
        if after.get("ssid").and_then(Value::as_str) != Some(wanted.as_str()) {
            return false;
        }
    }
    if input
        .get(&enc_key)
        .is_some_and(|v| !equal(v, after.get("encryption")))
    {
        return false;
    }
    if let Some(p) = input.get(&pwd_key).and_then(Value::as_str) {
        if after.get("passwordConfigured").and_then(Value::as_bool) != Some(!p.is_empty()) {
            return false;
        }
    } else if input.get(&enc_key).and_then(Value::as_str) == Some("none")
        && after.get("passwordConfigured").and_then(Value::as_bool) != Some(false)
    {
        return false;
    }
    true
}
fn verify_radio(input: &Value, suffix: &str, after: &Value) -> bool {
    if !verify_credentials(input, suffix, after) {
        return false;
    }
    for (from, to) in [
        ("on", "status"),
        ("channel", "channel"),
        ("bandwidth", "bandwidth"),
        ("txpwr", "txpwr"),
        ("hidden", "hidden"),
        ("wifimode", "wifimode"),
    ] {
        if input
            .get(format!("{from}{suffix}"))
            .is_some_and(|v| !equal(v, after.get(to)))
        {
            return false;
        }
    }
    match_fields(
        input,
        after,
        &[
            ("txbf", "txbf"),
            ("weakenable", "weakenable"),
            ("weakthreshold", "weakthreshold"),
            ("kickthreshold", "kickthreshold"),
        ],
    )
}
/// Check the corresponding public readback, not the setter's code=0 reply.
/// The executor must ALSO compare private password values before claiming a
/// password change verified: a presence-only DTO cannot prove secret equality.
/// For Mesh scan `after` is project("scan_mesh_node", setter_result). For join,
/// it is project("get_addnode_status", poll_result) for the SAME action MAC.
pub fn verify(action_id: &str, input: &Value, after: &Value) -> bool {
    if validate(action_id, input).is_err() || after.get("code").and_then(Value::as_i64) != Some(0) {
        return false;
    }
    match action_id {
        "set_wifi" => {
            let Some(index) = integer_value(input, "wifiIndex").ok().flatten() else {
                return false;
            };
            after
                .get("info")
                .and_then(Value::as_array)
                .and_then(|a| a.get((index - 1) as usize))
                .is_some_and(|r| verify_radio(input, "", r))
        }
        "set_all_wifi" => {
            let Some(radios) = after
                .get("info")
                .and_then(Value::as_array)
                .filter(|a| a.len() == 2)
            else {
                return false;
            };
            equal(&input["bsd"], after.get("bsd"))
                && radios.iter().enumerate().all(|(i, r)| {
                    equal(&input["bsd"], r.get("bsd"))
                        && verify_radio(input, &(i + 1).to_string(), r)
                })
        }
        "set_wifi_txpwr" => after
            .get("list")
            .and_then(Value::as_array)
            .is_some_and(|a| {
                a.len() == 2 && a.iter().all(|r| equal(&input["txpwr"], r.get("txpwr")))
            }),
        "set_wifi_ax" | "set_wifi_txbf" => {
            let key = if action_id == "set_wifi_ax" {
                "ax"
            } else {
                "txbf"
            };
            after
                .get("info")
                .and_then(Value::as_array)
                .is_some_and(|a| a.len() == 2 && a.iter().all(|r| equal(&input[key], r.get(key))))
        }
        "set_hostap_mlo" => {
            equal(&input["mlo_enable"], after.get("mlo_enable"))
                && (input["mlo_enable"] == "0" || number(&after["mlo_support"]) == Some(1))
        }
        "set_twt" => equal(&input["on"], after.get("status")),
        "miotrelay_switch" | "miscan_switch" | "set_mesh_switch" => {
            equal(&input["on"], after.get("enabled"))
        }
        "set_mesh_bh_mode" => equal(&input["bhmode"], after.get("bhmode")),
        "set_wifi_weak" => {
            let Some(index) = integer_value(input, "wifiIndex").ok().flatten() else {
                return false;
            };
            after
                .get("info")
                .and_then(Value::as_array)
                .and_then(|a| a.get((index - 1) as usize))
                .is_some_and(|r| {
                    match_fields(
                        input,
                        r,
                        &[
                            ("weakenable", "weakenable"),
                            ("weakthreshold", "weakthreshold"),
                            ("kickthreshold", "kickthreshold"),
                        ],
                    )
                })
        }
        "set_guest_wifi" => {
            equal(&input["on"], after.pointer("/info/guest"))
                && after
                    .pointer("/info/data")
                    .is_some_and(|r| verify_credentials(input, "", r))
        }
        "qos_guest" => {
            let wanted = input.get("percent").and_then(fraction);
            let up = input.get("percent_up").and_then(fraction).or(wanted);
            wanted.is_some()
                && wanted == after.pointer("/guest/percent").and_then(fraction)
                && up == after.pointer("/guest/percent_up").and_then(fraction)
        }
        "scan_mesh_node" => {
            after.get("complete") == Some(&json!(true))
                && after.get("list").is_some_and(Value::is_array)
        }
        "add_mesh_node" => {
            after.get("complete") == Some(&json!(true))
                && after.get("pending") == Some(&json!(false))
                && after.get("status").and_then(Value::as_i64) == Some(0)
        }
        _ => false,
    }
}

/// Supplement public verify() with exact PRIVATE password comparison. A
/// present credential is not proof that the setter stored the requested one.
/// current must be the raw native readback, never its public projection.
pub fn verify_private(action_id: &str, input: &Value, current: &Value) -> bool {
    if success(current).is_err() {
        return false;
    }
    let matches = |suffix: &str, native: &Value| {
        let key = format!("pwd{suffix}");
        input
            .get(&key)
            .and_then(Value::as_str)
            .is_none_or(|wanted| native_password(native).is_ok_and(|actual| wanted == actual))
    };
    match action_id {
        "set_wifi" => {
            let Some(index) = integer_value(input, "wifiIndex")
                .ok()
                .flatten()
                .filter(|n| (1..=2).contains(n))
            else {
                return false;
            };
            current
                .get("info")
                .and_then(Value::as_array)
                .and_then(|a| a.get((index - 1) as usize))
                .is_some_and(|r| matches("", r))
        }
        "set_all_wifi" => current
            .get("info")
            .and_then(Value::as_array)
            .is_some_and(|a| {
                a.len() == 2
                    && a.iter()
                        .enumerate()
                        .all(|(i, r)| matches(&(i + 1).to_string(), r))
            }),
        "set_guest_wifi" => current
            .pointer("/info/data")
            .is_some_and(|r| matches("", r)),
        _ => true,
    }
}
