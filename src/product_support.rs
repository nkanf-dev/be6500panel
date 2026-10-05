//! Fixed native service actions and bounded product operation logs.
use crate::{
    product_io::{Backend, Output, Program},
    readiness_tun::Budget,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::VecDeque;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApiError {
    pub status: u16,
    pub code: &'static str,
    pub message: &'static str,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    sequence: u64,
    time: String,
    level: &'static str,
    code: &'static str,
    module: &'static str,
    message: &'static str,
}
pub struct Logs {
    entries: VecDeque<Entry>,
    sequence: u64,
}
impl Default for Logs {
    fn default() -> Self {
        Self::new()
    }
}
impl Logs {
    pub fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(500),
            sequence: 0,
        }
    }
    pub fn push(
        &mut self,
        now: u64,
        level: &'static str,
        code: &'static str,
        module: &'static str,
        message: &'static str,
    ) {
        self.sequence = self.sequence.saturating_add(1);
        if self.entries.len() == 500 {
            self.entries.pop_front();
        }
        self.entries.push_back(Entry {
            sequence: self.sequence,
            time: crate::product_io::timestamp(now),
            level,
            code,
            module,
            message,
        });
    }
    pub fn response(&self, query: &str) -> Result<Value, ApiError> {
        let limit = if query.is_empty() {
            100
        } else {
            let (name, value) = query.split_once('=').ok_or_else(invalid)?;
            if name != "limit" || value.contains('&') {
                return Err(invalid());
            }
            value.parse::<usize>().map_err(|_| invalid())?
        };
        if !(1..=500).contains(&limit) {
            return Err(invalid());
        }
        Ok(
            json!({"entries":self.entries.iter().skip(self.entries.len().saturating_sub(limit)).collect::<Vec<_>>(),"capacity":500}),
        )
    }
}
fn invalid() -> ApiError {
    ApiError {
        status: 400,
        code: "invalid_input",
        message: "Service action fields are invalid.",
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Action {
    service: String,
    action: String,
    confirm_impact: bool,
}
/// Fresh observation and control-pending coordination are supplied by the one
/// product owner before this fixed native command can run.
pub fn service_action(
    body: &[u8],
    snapshot: &Value,
    io: &mut impl Backend,
    budget: &Budget<'_>,
) -> Result<(Value, bool), ApiError> {
    if body.len() > 1024
        || body
            .iter()
            .find(|b| !b.is_ascii_whitespace())
            .is_none_or(|b| *b != b'{')
    {
        return Err(invalid());
    }
    let action: Action = serde_json::from_slice(body).map_err(|_| invalid())?;
    let allowed = match action.service.as_str() {
        "ddns" => matches!(
            action.action.as_str(),
            "start" | "stop" | "restart" | "reload"
        ),
        "dnsmasq" => matches!(action.action.as_str(), "reload" | "restart"),
        _ => false,
    };
    if !allowed {
        return Err(ApiError {
            status: 400,
            code: "service_action_not_allowed",
            message: "This native service action is not allowed.",
        });
    }
    if action.service == "dnsmasq" && !action.confirm_impact {
        return Err(ApiError {
            status: 409,
            code: "service_impact_confirmation_required",
            message: "Confirm the brief DNS/DHCP interruption before this action.",
        });
    }
    if snapshot["stale"] == true {
        return Err(ApiError {
            status: 409,
            code: "service_observation_unavailable",
            message: "Current native service observation is unavailable.",
        });
    }
    let rows = snapshot["services"].as_array().ok_or(ApiError {
        status: 503,
        code: "service_observation_unavailable",
        message: "Current native service observation is unavailable.",
    })?;
    let available = rows.iter().any(|row| {
        row["name"] == action.service
            && row["configured"] == "present"
            && row["protected"] == false
            && row["actions"]
                .as_array()
                .is_some_and(|actions| actions.iter().any(|value| value == &action.action))
    });
    if !available {
        return Err(ApiError {
            status: 409,
            code: "service_action_not_available",
            message: "This action is not available for the observed service.",
        });
    }
    let actual = io.run(
        Program::Service,
        &[action.service.clone(), action.action.clone()],
        None,
        4096,
        budget,
    );
    let accepted = matches!(actual, Ok(Output { code: 0, .. }));
    let mut result = json!({"service":action.service,"action":action.action,"commandAccepted":accepted,"snapshot":snapshot});
    if !accepted {
        result["errorCode"] = "service_command_failed".into();
    }
    Ok((result, accepted))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logring_is_ordered_bounded_and_fixed() {
        let mut logs = Logs::new();
        for _ in 0..510 {
            logs.push(
                0,
                "INFO",
                "native_operation",
                "system",
                "Native operation observed.",
            );
        }
        let snapshot = logs.response("limit=500").unwrap();
        assert_eq!(snapshot["entries"].as_array().unwrap().len(), 500);
        assert_eq!(snapshot["entries"][0]["sequence"], 11);
        assert!(logs.response("limit=0").is_err());
        assert!(logs.response("limit=10&path=private").is_err());
    }
}
