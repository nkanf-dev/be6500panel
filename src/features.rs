//! On-demand router features on the authenticated serial management lane.
use crate::{
    product_io::{Backend, Program},
    readiness_tun::Budget,
};
use serde::Serialize;
use serde_json::{Value, json};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    Secret,
    Boolean,
    Integer,
    Ipv4,
    Ipv6,
    Mac,
    Select,
    Json,
}
#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: FieldKind,
    pub required: bool,
    pub options: &'static [&'static str],
    pub min: Option<i64>,
    pub max: Option<i64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Impact {
    Local,
    Network,
    Wireless,
    Maintenance,
}
#[derive(Clone, Copy, Debug)]
pub struct Action {
    pub id: &'static str,
    pub title: &'static str,
    pub controller: &'static str,
    pub handler: &'static str,
    pub fields: &'static [Field],
    pub impact: Impact,
    pub configs: &'static [&'static str],
    pub readback: &'static str,
}
#[derive(Clone, Copy, Debug)]
pub struct Read {
    pub id: &'static str,
    pub title: &'static str,
    pub controller: &'static str,
    pub handler: &'static str,
    pub fields: &'static [Field],
}
#[derive(Clone, Copy, Debug)]
pub struct Domain {
    pub id: &'static str,
    pub title: &'static str,
    pub reads: &'static [Read],
    pub actions: &'static [Action],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error {
    pub status: u16,
    pub code: &'static str,
    pub message: &'static str,
}
pub const fn field(
    key: &'static str,
    label: &'static str,
    kind: FieldKind,
    required: bool,
) -> Field {
    Field {
        key,
        label,
        kind,
        required,
        options: &[],
        min: None,
        max: None,
    }
}
pub const fn choice(
    key: &'static str,
    label: &'static str,
    required: bool,
    options: &'static [&'static str],
) -> Field {
    Field {
        key,
        label,
        kind: FieldKind::Select,
        required,
        options,
        min: None,
        max: None,
    }
}
pub const fn integer(
    key: &'static str,
    label: &'static str,
    required: bool,
    min: i64,
    max: i64,
) -> Field {
    Field {
        key,
        label,
        kind: FieldKind::Integer,
        required,
        options: &[],
        min: Some(min),
        max: Some(max),
    }
}
pub fn invalid() -> Error {
    Error {
        status: 400,
        code: "invalid_feature_input",
        message: "字段或取值不符合该功能要求。",
    }
}
pub fn invoke<B: Backend>(
    io: &mut B,
    controller: &str,
    handler: &str,
    input: &Value,
    b: &Budget<'_>,
) -> Result<Value, Error> {
    b.check().map_err(|_| Error {
        status: 504,
        code: "operation_timeout",
        message: "操作超时。",
    })?;
    let raw = serde_json::to_vec(input).map_err(|_| invalid())?;
    if raw.len() > 64 << 10 {
        return Err(invalid());
    }
    let out = io
        .run(
            Program::Vendor,
            &[controller.into(), handler.into()],
            Some(&raw),
            256 << 10,
            b,
        )
        .map_err(|_| Error {
            status: 503,
            code: "vendor_unavailable",
            message: "暂时无法读取原厂服务。",
        })?;
    if out.code != 0 {
        return Err(Error {
            status: 503,
            code: "vendor_unavailable",
            message: "原厂服务暂不可用。",
        });
    }
    let value: Value = serde_json::from_slice(&out.stdout).map_err(|_| Error {
        status: 502,
        code: "vendor_invalid_response",
        message: "原厂服务响应无效。",
    })?;
    // Native getters sometimes use `code` as a string-valued setting. Domain
    // projectors validate their exact read contract; mutation checks stay in root.
    Ok(value)
}
/// Domain implementations use explicit public projections; never emit complete native replies.
pub fn pick(value: &Value, keys: &[&str]) -> Value {
    let mut result = serde_json::Map::new();
    for k in keys {
        if let Some(v) = value.get(*k) {
            result.insert((*k).into(), v.clone());
        }
    }
    Value::Object(result)
}
pub fn public_secrets(value: &mut Value) {
    match value {
        Value::Object(m) => {
            let keys = m.keys().cloned().collect::<Vec<_>>();
            for key in keys {
                let low = key.to_ascii_lowercase();
                if [
                    "password",
                    "passwd",
                    "token",
                    "secret",
                    "pswd",
                    "key",
                    "username",
                    "account",
                    "pppoename",
                    "pppoepwd",
                ]
                .iter()
                .any(|s| low == *s || low.ends_with(s))
                {
                    let set = m
                        .get(&key)
                        .is_some_and(|v| v.as_str().is_some_and(|s| !s.is_empty()));
                    m.remove(&key);
                    m.insert(format!("{key}Configured"), json!(set));
                } else if let Some(v) = m.get_mut(&key) {
                    public_secrets(v);
                }
            }
        }
        Value::Array(a) => {
            for v in a {
                public_secrets(v)
            }
        }
        _ => {}
    }
}
