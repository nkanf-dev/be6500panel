//! Bounded, pure Clash import and compilation-eligibility summary.
//!
//! YAML syntax is owned by serde-saphyr/granit, not an application YAML parser.
//! A streaming native admission pass checks every ignored subtree and rejects
//! resource amplification before typed deserialization. No document Value/AST
//! is built. A second streaming scalar cursor retains raw scalar spelling and
//! tags for canonical native parsing without caching the whole document. Private DTOs
//! have no Serialize implementation and Debug never traverses private values.
use crate::native::{self, Node};
use crate::policy::{Diagnostic, MAX_RULES, Rule, RuleKind, Target};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use serde_saphyr::granit_parser::{self as yaml, Event, Parser, ScalarStyle, StrInput};
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::{self, Write};
use std::net::IpAddr;

pub const MAX_SUBSCRIPTION_BYTES: usize = 2 << 20;
pub const MAX_NODES: usize = 2048;
pub const MAX_GROUPS: usize = 128;
const MAX_SCALAR_BYTES: usize = 8192;
const MAX_YAML_NODES: usize = 100_000;
const MAX_YAML_EVENTS: usize = 200_004;
const MAX_YAML_DEPTH: usize = 32;

#[derive(Clone, Default)]
pub struct Subscription {
    pub nodes: Vec<Node>,
    pub rules: Vec<Rule>,
    pub diagnostics: Vec<Diagnostic>,
    pub group_count: usize,
    pub fake_ip: bool,
}
impl fmt::Debug for Subscription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Subscription (private)")
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicNode {
    pub id: String,
    pub label: String,
    pub server: String,
    pub port: u16,
    pub protocol: String,
    pub transport: String,
    pub reality: bool,
    pub vision: bool,
    pub utls: bool,
    pub udp: bool,
}
impl Subscription {
    pub fn public_nodes(&self) -> Vec<PublicNode> {
        self.nodes
            .iter()
            .map(|n| PublicNode {
                id: n.id.clone(),
                label: n.name.clone(),
                server: n.server.clone(),
                port: n.port,
                protocol: "vless".into(),
                transport: "tcp".into(),
                reality: true,
                vision: true,
                utls: true,
                udp: n.udp,
            })
            .collect()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubscriptionError {
    message: &'static str,
}
impl fmt::Display for SubscriptionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message)
    }
}
impl std::error::Error for SubscriptionError {}
fn error(message: &'static str) -> SubscriptionError {
    SubscriptionError { message }
}
fn diagnostic(scope: &str, index: i64, code: &str, message: &str) -> Diagnostic {
    Diagnostic {
        scope: scope.into(),
        index,
        code: code.into(),
        message: message.into(),
    }
}
fn parser(input: &str) -> Parser<'_, StrInput<'_>> {
    Parser::with_options(
        StrInput::new(input),
        yaml::options! {
            emit_comments: false, max_buffered_comment_events: 0,
            flow_nesting_limit: MAX_YAML_DEPTH, block_nesting_limit: MAX_YAML_DEPTH,
            simple_key_max_lookahead: 1024, max_directive_bytes: 1024,
            max_reserved_directive_params: 0,
        },
    )
}

// Only the native parser decides YAML syntax. This small event admission filter
// keeps one bounded set of keys per open mapping; it never retains values.
enum Container {
    Map {
        expecting_key: bool,
        keys: BTreeSet<String>,
    },
    Seq,
}
fn finish_value(stack: &mut [Container]) {
    if let Some(Container::Map { expecting_key, .. }) = stack.last_mut() {
        *expecting_key = true;
    }
}
fn admit(input: &str) -> Result<(), SubscriptionError> {
    let mut stack = Vec::new();
    let (mut nodes, mut events, mut documents) = (0, 0, 0);
    for event in parser(input) {
        let (event, _) = event.map_err(|_| error("invalid subscription YAML"))?;
        events += 1;
        if events > MAX_YAML_EVENTS {
            return Err(error("subscription YAML event limit exceeded"));
        }
        match event {
            Event::DocumentStart(..) => {
                documents += 1;
                if documents > 1 {
                    return Err(error("subscription must contain one YAML document"));
                }
            }
            Event::Alias(_) => {
                return Err(error(
                    "subscription YAML aliases and anchors are not supported",
                ));
            }
            Event::Scalar(value, style, anchor, tag) => {
                nodes += 1;
                if anchor != 0 {
                    return Err(error(
                        "subscription YAML aliases and anchors are not supported",
                    ));
                }
                if value.len() > MAX_SCALAR_BYTES {
                    return Err(error("subscription YAML scalar limit exceeded"));
                }
                if let Some(Container::Map {
                    expecting_key,
                    keys,
                }) = stack.last_mut()
                {
                    if *expecting_key {
                        if value == "<<"
                            || !string_scalar(&value, style, tag.as_deref())
                            || !keys.insert(value.into_owned())
                        {
                            return Err(error("subscription YAML keys must be unique strings"));
                        }
                        *expecting_key = false;
                    } else {
                        *expecting_key = true;
                    }
                }
            }
            Event::SequenceStart(_, anchor, _) | Event::MappingStart(_, anchor, _) => {
                nodes += 1;
                if anchor != 0 {
                    return Err(error(
                        "subscription YAML aliases and anchors are not supported",
                    ));
                }
                if matches!(
                    stack.last(),
                    Some(Container::Map {
                        expecting_key: true,
                        ..
                    })
                ) {
                    return Err(error("subscription YAML keys must be unique strings"));
                }
                if stack.len() >= MAX_YAML_DEPTH {
                    return Err(error("subscription YAML nesting limit exceeded"));
                }
                stack.push(if matches!(event, Event::MappingStart(..)) {
                    Container::Map {
                        expecting_key: true,
                        keys: BTreeSet::new(),
                    }
                } else {
                    Container::Seq
                });
            }
            Event::SequenceEnd | Event::MappingEnd => {
                stack.pop();
                finish_value(&mut stack);
            }
            _ => {}
        }
        if nodes > MAX_YAML_NODES {
            return Err(error("subscription YAML node limit exceeded"));
        }
    }
    Ok(())
}
fn string_scalar(value: &str, style: ScalarStyle, tag: Option<&yaml::Tag>) -> bool {
    if let Some(tag) = tag {
        let text = tag.to_string();
        return text == "tag:yaml.org,2002:str" || text == "!";
    }
    if style != ScalarStyle::Plain {
        return true;
    }
    // YAML v3 core-schema nonstrings. This is scalar-tag admission, not YAML
    // syntax parsing. Typed serde visitors remain authoritative for values.
    if matches!(
        value,
        "" | "~"
            | "null"
            | "Null"
            | "NULL"
            | "true"
            | "True"
            | "TRUE"
            | "false"
            | "False"
            | "FALSE"
    ) {
        return false;
    }
    let compact = value.replace('_', "");
    let unsigned = compact.strip_prefix(['+', '-']).unwrap_or(&compact);
    if unsigned.starts_with("0x") || unsigned.starts_with("0o") || unsigned.starts_with("0b") {
        let (radix, digits) = match &unsigned[..2] {
            "0x" => (16, &unsigned[2..]),
            "0o" => (8, &unsigned[2..]),
            _ => (2, &unsigned[2..]),
        };
        if !digits.is_empty() && digits.chars().all(|c| c.is_digit(radix)) {
            return false;
        }
    }
    if compact.parse::<f64>().is_ok()
        || matches!(
            value,
            ".inf" | ".Inf" | ".INF" | "+.inf" | "-.inf" | ".nan" | ".NaN" | ".NAN"
        )
    {
        return false;
    }
    // YAML v3 resolves RFC3339/date forms as timestamps rather than strings.
    !(value.len() >= 10
        && value.as_bytes()[..4].iter().all(u8::is_ascii_digit)
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value.as_bytes()[5..7].iter().all(u8::is_ascii_digit)
        && value.as_bytes()[8..10].iter().all(u8::is_ascii_digit))
}

struct Context<'a> {
    scalars: RefCell<Parser<'a, StrInput<'a>>>,
}
#[derive(Default)]
struct Scalar {
    text: String,
    string: bool,
    true_bool: bool,
}
// A tiny serde typeless visitor consumes scalars or validates/discards compound
// fields. It stores no compound tree and only a unit marker for scalar values.
struct Shape {
    scalar: bool,
}
impl<'de> Deserialize<'de> for Shape {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct ShapeVisitor;
        impl<'de> Visitor<'de> for ShapeVisitor {
            type Value = Shape;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bounded YAML value")
            }
            fn visit_str<E: de::Error>(self, _: &str) -> Result<Shape, E> {
                Ok(Shape { scalar: true })
            }
            fn visit_bool<E: de::Error>(self, _: bool) -> Result<Shape, E> {
                Ok(Shape { scalar: true })
            }
            fn visit_i64<E: de::Error>(self, _: i64) -> Result<Shape, E> {
                Ok(Shape { scalar: true })
            }
            fn visit_u64<E: de::Error>(self, _: u64) -> Result<Shape, E> {
                Ok(Shape { scalar: true })
            }
            fn visit_f64<E: de::Error>(self, _: f64) -> Result<Shape, E> {
                Ok(Shape { scalar: true })
            }
            fn visit_unit<E: de::Error>(self) -> Result<Shape, E> {
                Ok(Shape { scalar: true })
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Shape, A::Error> {
                while a.next_element::<de::IgnoredAny>()?.is_some() {}
                Ok(Shape { scalar: false })
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Shape, A::Error> {
                while a.next_entry::<de::IgnoredAny, de::IgnoredAny>()?.is_some() {}
                Ok(Shape { scalar: false })
            }
        }
        d.deserialize_any(ShapeVisitor)
    }
}
struct ScalarSeed<'a, 'b>(&'b Context<'a>);
impl<'de> DeserializeSeed<'de> for ScalarSeed<'de, '_> {
    type Value = Scalar;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Scalar, D::Error> {
        let shape = serde_saphyr::Spanned::<Shape>::deserialize(d)?;
        if !shape.value.scalar {
            return Ok(Scalar::default());
        }
        let offset = shape
            .referenced
            .span()
            .byte_offset()
            .ok_or_else(|| de::Error::custom("invalid scalar position"))?
            as usize;
        for event in self.0.scalars.borrow_mut().by_ref() {
            let (event, span) = event.map_err(|_| de::Error::custom("invalid YAML scalar"))?;
            if span.start.byte_offset() != Some(offset) {
                continue;
            }
            if let Event::Scalar(value, style, _, tag) = event {
                let string = string_scalar(&value, style, tag.as_deref());
                let bool_tag = tag
                    .as_ref()
                    .is_some_and(|t| t.to_string() == "tag:yaml.org,2002:bool");
                let true_bool =
                    value == "true" && (bool_tag || tag.is_none() && style == ScalarStyle::Plain);
                return Ok(Scalar {
                    text: value.into_owned(),
                    string,
                    true_bool,
                });
            }
        }
        Err(de::Error::custom("invalid scalar position"))
    }
}
#[derive(Default)]
struct Entry {
    mapping: bool,
    fields: BTreeMap<String, Scalar>,
    members: Vec<String>,
    reality: BTreeMap<String, Scalar>,
}
impl Entry {
    fn text(&self, key: &str) -> &str {
        self.fields.get(key).map_or("", |s| s.text.as_str())
    }
    fn boolean(&self, key: &str) -> bool {
        self.fields.get(key).is_some_and(|s| s.true_bool)
    }
}
struct EntrySeed<'a, 'b> {
    context: &'b Context<'a>,
    group: bool,
}
impl<'de> DeserializeSeed<'de> for EntrySeed<'de, '_> {
    type Value = Entry;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Entry, D::Error> {
        d.deserialize_any(self)
    }
}
struct FieldsSeed<'a, 'b>(&'b Context<'a>);
impl<'de> DeserializeSeed<'de> for FieldsSeed<'de, '_> {
    type Value = BTreeMap<String, Scalar>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}
struct MembersSeed<'a, 'b>(&'b Context<'a>);
impl<'de> DeserializeSeed<'de> for MembersSeed<'de, '_> {
    type Value = Vec<String>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}
struct EntriesSeed<'a, 'b> {
    context: &'b Context<'a>,
    group: bool,
}
impl<'de> DeserializeSeed<'de> for EntriesSeed<'de, '_> {
    type Value = Vec<Entry>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        // deserialize_any is intentional: deserialize_seq accepts null as [],
        // but Go requires a real sequence for proxies, groups and rules.
        d.deserialize_any(self)
    }
}
struct RulesSeed<'a, 'b>(&'b Context<'a>);
impl<'de> DeserializeSeed<'de> for RulesSeed<'de, '_> {
    type Value = Vec<String>;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Self::Value, D::Error> {
        d.deserialize_any(self)
    }
}
#[derive(Default)]
struct Parsed {
    nodes: Vec<Entry>,
    groups: Vec<Entry>,
    rules: Vec<String>,
    fake_ip: bool,
}
struct RootSeed<'a, 'b>(&'b Context<'a>);
impl<'de> DeserializeSeed<'de> for RootSeed<'de, '_> {
    type Value = Parsed;
    fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<Parsed, D::Error> {
        d.deserialize_any(self)
    }
}

/// Import bounded YAML bytes, without I/O, inclusion, interpolation or runtime
/// changes. Resource/type errors contain fixed text, never parser/source detail.
pub fn parse_clash_yaml(data: &[u8]) -> Result<Subscription, SubscriptionError> {
    if data.len() > MAX_SUBSCRIPTION_BYTES {
        return Err(error("subscription exceeds 2097152 bytes"));
    }
    let input =
        std::str::from_utf8(data).map_err(|_| error("subscription YAML must be valid UTF-8"))?;
    // Normalize the optional BOM exactly as serde-saphyr's string entry point.
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    admit(input)?;
    let context = Context {
        scalars: RefCell::new(parser(input)),
    };
    let options = serde_saphyr::options! {
        emit_comments:false, with_snippet:false, crop_radius:0,
        duplicate_keys:serde_saphyr::DuplicateKeyPolicy::Error,
        merge_keys:serde_saphyr::MergeKeyPolicy::Error,
        strict_booleans:true, legacy_octal_numbers:true,
        reject_unsupported_tags:true, reject_non_finite_typeless_float:false,
        budget:serde_saphyr::budget! {
            max_reader_input_bytes:Some(MAX_SUBSCRIPTION_BYTES),
            max_events:MAX_YAML_EVENTS,max_nodes:MAX_YAML_NODES,max_depth:MAX_YAML_DEPTH,
            flow_nesting_limit:MAX_YAML_DEPTH,max_documents:1,
            max_aliases:0,max_anchors:0,max_recorded_anchor_events:0,max_recorded_anchor_bytes:0,
            max_total_scalar_bytes:MAX_SUBSCRIPTION_BYTES,max_merge_keys:0,max_inclusion_depth:0,
            max_buffered_comment_events:0,max_total_comment_bytes:0,
        },
    };
    let parsed = serde_saphyr::with_deserializer_from_str_with_options(input, options, |d| {
        RootSeed(&context).deserialize(d)
    })
    .map_err(|_| error("invalid or excessive subscription YAML"))?;
    let mut out = Subscription::default();
    let mut names = BTreeSet::new();
    for (index, item) in parsed.nodes.iter().enumerate() {
        match parse_node(item) {
            Ok(mut node) => {
                if !names.insert(node.name.clone()) {
                    out.diagnostics.push(diagnostic(
                        "node",
                        index as i64,
                        "duplicate-node",
                        "duplicate node name",
                    ));
                    continue;
                }
                let identity = format!(
                    "{}:{}\0{}\0{}",
                    node.server, node.port, node.uuid, node.name
                );
                node.id = format!("{:x}", Sha256::digest(identity.as_bytes()))[..16].into();
                out.nodes.push(node);
            }
            Err(message) => out.diagnostics.push(diagnostic(
                "node",
                index as i64,
                "unsupported-node",
                message,
            )),
        }
    }
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (index, group) in parsed.groups.into_iter().enumerate() {
        let name = group.text("name");
        if name.is_empty() || name.len() > 256 {
            return Err(error("invalid selector group name"));
        }
        names.insert(name.into());
        // Go appends repeated group declarations; it does not replace their
        // membership. Enforce the explicit member budget before extending.
        let members = groups.entry(name.into()).or_default();
        if group.members.len() > MAX_NODES - members.len() {
            return Err(error("selector group member limit exceeded"));
        }
        members.extend(group.members.iter().cloned());
        out.group_count += 1;
        if !matches!(
            group.text("type"),
            "select" | "url-test" | "fallback" | "load-balance"
        ) {
            out.diagnostics.push(diagnostic(
                "group",
                index as i64,
                "unsupported-group",
                "group is not a known selector",
            ));
        }
    }
    if out.group_count > 0 {
        out.diagnostics.push(diagnostic("subscription",-1,"selected-node-policy","proxy selectors use the selected node; unambiguous direct or reject groups retain their routing intent"));
    }
    let mut resolved = BTreeMap::new();
    let targets: BTreeMap<_, _> = groups
        .keys()
        .filter_map(|name| {
            uniform_target(name, &groups, &mut BTreeSet::new(), &mut resolved)
                .map(|target| (name.clone(), target))
        })
        .collect();
    for (index, text) in parsed.rules.into_iter().enumerate() {
        match parse_rule(&text, &names, &targets) {
            Ok(mut rule) => {
                rule.index = index as i64;
                out.rules.push(rule);
            }
            Err((code, message)) => {
                out.diagnostics
                    .push(diagnostic("rule", index as i64, code, message))
            }
        }
    }
    out.fake_ip = parsed.fake_ip;
    if out.nodes.is_empty() {
        return Err(error("subscription contains no supported VLESS nodes"));
    }
    Ok(out)
}
fn parse_node(item: &Entry) -> Result<Node, &'static str> {
    if !item.mapping {
        return Err("node must be a mapping");
    }
    if item.text("type") != "vless" {
        return Err("only VLESS is supported");
    }
    if !matches!(item.text("network"), "" | "tcp") {
        return Err("only native TCP transport is supported");
    }
    if !item.boolean("tls") {
        return Err("TLS is required");
    }
    if item.boolean("skip-cert-verify") {
        return Err("insecure TLS is not supported");
    }
    let port = item.text("port");
    if port.is_empty() || !port.bytes().all(|c| c.is_ascii_digit()) {
        return Err("invalid server port");
    }
    let port = port
        .parse::<u16>()
        .ok()
        .filter(|p| *p != 0)
        .ok_or("invalid server port")?;
    let mut node = Node {
        name: item.text("name").into(),
        server: item.text("server").into(),
        uuid: item.text("uuid").into(),
        port,
        server_name: if item.text("servername").is_empty() {
            item.text("sni")
        } else {
            item.text("servername")
        }
        .into(),
        flow: item.text("flow").into(),
        fingerprint: item.text("client-fingerprint").into(),
        udp: item.boolean("udp"),
        reality_public_key: item
            .reality
            .get("public-key")
            .map_or("", |s| s.text.as_str())
            .into(),
        reality_short_id: item
            .reality
            .get("short-id")
            .map_or("", |s| s.text.as_str())
            .into(),
        ..Node::default()
    };
    if node.name.is_empty() || node.name.len() > 256 {
        return Err("node name is required and bounded");
    }
    native::validate_node(&node).map_err(|e| match e.to_string().as_str() {
        "invalid server endpoint" => "invalid server endpoint",
        "REALITY certificate DNS identity is required" => {
            "REALITY certificate DNS identity is required"
        }
        "invalid VLESS UUID" => "invalid VLESS UUID",
        "only xtls-rprx-vision flow is supported" => "only xtls-rprx-vision flow is supported",
        "only Chrome uTLS fingerprint is supported" => "only Chrome uTLS fingerprint is supported",
        "invalid REALITY public key" => "invalid REALITY public key",
        "invalid REALITY short ID" => "invalid REALITY short ID",
        "VLESS UDP/XUDP must be enabled" => "VLESS UDP/XUDP must be enabled",
        _ => "unsupported node parameters",
    })?;
    if [
        "ws-opts",
        "grpc-opts",
        "http-opts",
        "h2-opts",
        "smux",
        "dialer-proxy",
    ]
    .iter()
    .any(|k| item.fields.contains_key(*k))
    {
        return Err("unsupported transport or multiplex option");
    }
    node.id.clear();
    Ok(node)
}
fn uniform_target(
    name: &str,
    groups: &BTreeMap<String, Vec<String>>,
    visiting: &mut BTreeSet<String>,
    resolved: &mut BTreeMap<String, Option<Target>>,
) -> Option<Target> {
    if let Some(target) = resolved.get(name) {
        return target.clone();
    }
    if visiting.contains(name) || visiting.len() > MAX_GROUPS {
        return None;
    }
    let members = groups.get(name)?;
    if members.is_empty() {
        resolved.insert(name.into(), None);
        return None;
    }
    visiting.insert(name.into());
    let mut result = None;
    for member in members {
        let target = match member.to_uppercase().as_str() {
            "DIRECT" => Some(Target::Direct),
            "REJECT" | "REJECT-DROP" => Some(Target::Block),
            _ => uniform_target(member, groups, visiting, resolved),
        };
        let Some(target) = target else {
            visiting.remove(name);
            resolved.insert(name.into(), None);
            return None;
        };
        if result.as_ref().is_some_and(|r| *r != target) {
            visiting.remove(name);
            resolved.insert(name.into(), None);
            return None;
        }
        result = Some(target);
    }
    visiting.remove(name);
    // Cache by group, including ambiguity. A fan-out DAG is linear in the
    // bounded source edges rather than exponentially revisiting its leaves.
    resolved.insert(name.into(), result.clone());
    result
}
fn valid_domain(domain: &str) -> bool {
    !domain.is_empty()
        && domain.len() <= 253
        && !domain.ends_with('.')
        && domain.split('.').all(|p| {
            !p.is_empty()
                && p.len() <= 63
                && !p.starts_with('-')
                && !p.ends_with('-')
                && p.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}
fn valid_prefix(value: &str) -> bool {
    let Some((address, bits)) = value.rsplit_once('/') else {
        return false;
    };
    if bits.is_empty()
        || !bits.bytes().all(|c| c.is_ascii_digit())
        || bits.len() > 1 && bits.starts_with('0')
    {
        return false;
    }
    let (Ok(ip), Ok(bits)) = (address.parse::<IpAddr>(), bits.parse::<u8>()) else {
        return false;
    };
    match ip {
        IpAddr::V4(_) => bits <= 32,
        IpAddr::V6(a) => bits <= 128 && a.to_ipv4_mapped().is_none(),
    }
}
fn parse_rule(
    text: &str,
    names: &BTreeSet<String>,
    groups: &BTreeMap<String, Target>,
) -> Result<Rule, (&'static str, &'static str)> {
    let parts: Vec<_> = text.split(',').map(str::trim).collect();
    if parts.len() < 2 || parts.len() > 4 {
        return Err(("invalid-rule", "invalid rule fields"));
    }
    let mut out = Rule::default();
    let mut target_index = 2;
    out.kind = match parts[0].to_uppercase().as_str() {
        "DOMAIN" => RuleKind::Domain,
        "DOMAIN-SUFFIX" => RuleKind::DomainSuffix,
        "DOMAIN-KEYWORD" => RuleKind::DomainKeyword,
        "IP-CIDR" | "IP-CIDR6" => RuleKind::IpCidr,
        "MATCH" | "FINAL" => {
            target_index = 1;
            RuleKind::Match
        }
        "GEOIP" | "GEOSITE" => {
            if parts.len() < 3 || !parts[1].eq_ignore_ascii_case("CN") {
                return Err((
                    "unsupported-rule",
                    if parts[0].eq_ignore_ascii_case("GEOIP") {
                        "only CN GEOIP has a controlled SRS mapping"
                    } else {
                        "only CN GEOSITE has a controlled SRS mapping"
                    },
                ));
            }
            out.value = if parts[0].eq_ignore_ascii_case("GEOIP") {
                "cn-ip"
            } else {
                "cn-domain"
            }
            .into();
            RuleKind::RuleSet
        }
        "PROCESS-NAME" | "PROCESS-PATH" => {
            return Err((
                "unsupported-process-rule",
                "process rules cannot classify forwarded LAN clients",
            ));
        }
        _ => return Err(("unsupported-rule", "unsupported rule type")),
    };
    if parts.len() <= target_index {
        return Err(("invalid-rule", "missing rule target"));
    }
    if parts.len() > target_index + 2 {
        return Err(("invalid-rule", "excess rule fields"));
    }
    if parts.len() == target_index + 2 {
        if parts[target_index + 1] != "no-resolve"
            || out.kind != RuleKind::IpCidr && out.value != "cn-ip"
        {
            return Err(("invalid-rule", "unsupported rule option"));
        }
        out.no_resolve = true;
    }
    let target = parts[target_index];
    out.target = match target.to_uppercase().as_str() {
        "DIRECT" => Target::Direct,
        "REJECT" | "REJECT-DROP" => Target::Block,
        "PROXY" => Target::Proxy,
        _ => {
            if let Some(target) = groups.get(target) {
                target.clone()
            } else if names.contains(target) {
                Target::Proxy
            } else {
                return Err((
                    "unknown-rule-target",
                    "rule target is not a known node or selector",
                ));
            }
        }
    };
    if !matches!(out.kind, RuleKind::Match | RuleKind::RuleSet) {
        out.value = parts[1].into();
    }
    let problem = match out.kind {
        RuleKind::Domain | RuleKind::DomainSuffix if !valid_domain(&out.value) => {
            Some("invalid rule domain")
        }
        RuleKind::DomainKeyword
            if out.value.is_empty()
                || out.value.len() > 253
                || out.value.bytes().any(|c| matches!(c, b'\r' | b'\n' | 0)) =>
        {
            Some("invalid rule keyword")
        }
        RuleKind::IpCidr if !valid_prefix(&out.value) => Some("invalid rule IP prefix"),
        _ => None,
    };
    if let Some(message) = problem {
        return Err(("invalid-rule", message));
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicySummary {
    pub total: usize,
    pub supported: usize,
    pub omitted: usize,
    pub reasons: Vec<PolicyReason>,
    pub omitted_rules: Vec<PolicyOmission>,
    pub revision: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PolicyReason {
    pub code: String,
    pub count: usize,
    pub message: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PolicyOmission {
    pub index: i64,
    pub code: String,
    pub message: String,
}
pub fn policy_reason_message(code: &str) -> &'static str {
    match code {
        "unsupported-process-rule" => {
            "路由器网关不能识别 LAN 客户端的应用进程；进程规则不会参与分流"
        }
        "unsupported-rule" => "规则类型或地区规则集不受当前原生编译器支持",
        "invalid-rule" => "规则参数无效，无法生成原生路由规则",
        "unknown-rule-target" => "规则目标不在当前节点或选择组中",
        "unreachable-rule" => "规则位于终止 MATCH 规则之后，不会参与分流",
        _ => "规则无法生成原生路由规则",
    }
}
struct GoJsonFormatter;
impl serde_json::ser::Formatter for GoJsonFormatter {
    fn write_string_fragment<W: ?Sized + Write>(
        &mut self,
        writer: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        let mut start = 0;
        for (offset, c) in fragment.char_indices() {
            let escape = match c {
                '<' => b"\\u003c".as_slice(),
                '>' => b"\\u003e".as_slice(),
                '&' => b"\\u0026".as_slice(),
                '\u{2028}' => b"\\u2028".as_slice(),
                '\u{2029}' => b"\\u2029".as_slice(),
                _ => continue,
            };
            writer.write_all(&fragment.as_bytes()[start..offset])?;
            writer.write_all(escape)?;
            start = offset + c.len_utf8();
        }
        writer.write_all(&fragment.as_bytes()[start..])
    }
}
struct HashWriter(Sha256);
impl Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
/// Exact Go compilation-eligibility identity. This does not report runtime health.
pub fn summarize_policy(sub: &Subscription) -> PolicySummary {
    let mut omissions: BTreeMap<i64, PolicyOmission> = BTreeMap::new();
    let mut indices = BTreeSet::new();
    for d in &sub.diagnostics {
        if d.scope != "rule" {
            continue;
        }
        indices.insert(d.index);
        if omissions.get(&d.index).is_none_or(|o| d.code < o.code) {
            omissions.insert(
                d.index,
                PolicyOmission {
                    index: d.index,
                    code: d.code.clone(),
                    message: policy_reason_message(&d.code).into(),
                },
            );
        }
    }
    let mut terminal = false;
    for r in &sub.rules {
        indices.insert(r.index);
        if terminal {
            omissions.entry(r.index).or_insert_with(|| PolicyOmission {
                index: r.index,
                code: "unreachable-rule".into(),
                message: policy_reason_message("unreachable-rule").into(),
            });
        }
        if r.kind == RuleKind::Match {
            terminal = true;
        }
    }
    let omitted_rules: Vec<_> = omissions.into_values().collect();
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for o in &omitted_rules {
        *counts.entry(&o.code).or_default() += 1;
    }
    let reasons = counts
        .into_iter()
        .map(|(code, count)| PolicyReason {
            code: code.into(),
            count,
            message: policy_reason_message(code).into(),
        })
        .collect();
    #[derive(Serialize)]
    struct OmittedIdentity<'a> {
        index: i64,
        code: &'a str,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Identity<'a> {
        rules: Option<&'a [Rule]>,
        omissions: Vec<OmittedIdentity<'a>>,
        #[serde(rename = "fakeIP")]
        fake_ip: bool,
        selected_node_policy: bool,
    }
    // Go ParseClashYAML leaves its parsed slice nil even when source rules: [].
    let identity = Identity {
        rules: if sub.rules.is_empty() {
            None
        } else {
            Some(&sub.rules)
        },
        omissions: omitted_rules
            .iter()
            .map(|o| OmittedIdentity {
                index: o.index,
                code: &o.code,
            })
            .collect(),
        fake_ip: sub.fake_ip,
        selected_node_policy: sub.group_count > 0,
    };
    let mut writer = HashWriter(Sha256::new());
    identity
        .serialize(&mut serde_json::Serializer::with_formatter(
            &mut writer,
            GoJsonFormatter,
        ))
        .expect("typed policy hash serialization cannot fail");
    PolicySummary {
        total: indices.len(),
        supported: indices.len() - omitted_rules.len(),
        omitted: omitted_rules.len(),
        reasons,
        omitted_rules,
        revision: format!("{:x}", writer.0.finalize()),
    }
}

impl<'de> Visitor<'de> for EntrySeed<'de, '_> {
    type Value = Entry;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded subscription entry")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Entry, A::Error> {
        let mut out = Entry {
            mapping: true,
            ..Entry::default()
        };
        while let Some(key) = a.next_key::<String>()? {
            if self.group && key == "proxies" {
                out.members = a.next_value_seed(MembersSeed(self.context))?;
            } else if !self.group && key == "reality-opts" {
                out.reality = a.next_value_seed(FieldsSeed(self.context))?;
            } else if matches!(
                key.as_str(),
                "name"
                    | "type"
                    | "network"
                    | "tls"
                    | "skip-cert-verify"
                    | "server"
                    | "uuid"
                    | "port"
                    | "servername"
                    | "sni"
                    | "flow"
                    | "client-fingerprint"
                    | "udp"
                    | "ws-opts"
                    | "grpc-opts"
                    | "http-opts"
                    | "h2-opts"
                    | "smux"
                    | "dialer-proxy"
            ) {
                out.fields
                    .insert(key, a.next_value_seed(ScalarSeed(self.context))?);
            } else {
                a.next_value::<de::IgnoredAny>()?;
            }
        }
        Ok(out)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Entry, A::Error> {
        while a.next_element::<de::IgnoredAny>()?.is_some() {}
        Ok(Entry::default())
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<Entry, E> {
        Ok(Entry::default())
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Entry, E> {
        Ok(Entry::default())
    }
    fn visit_unit<E: de::Error>(self) -> Result<Entry, E> {
        Ok(Entry::default())
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Entry, E> {
        Ok(Entry::default())
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Entry, E> {
        Ok(Entry::default())
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Entry, E> {
        Ok(Entry::default())
    }
}

impl<'de> Visitor<'de> for FieldsSeed<'de, '_> {
    type Value = BTreeMap<String, Scalar>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded fields")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        let mut out = BTreeMap::new();
        while let Some(key) = a.next_key::<String>()? {
            if matches!(key.as_str(), "public-key" | "short-id" | "enhanced-mode") {
                out.insert(key, a.next_value_seed(ScalarSeed(self.0))?);
            } else {
                a.next_value::<de::IgnoredAny>()?;
            }
        }
        Ok(out)
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        while a.next_element::<de::IgnoredAny>()?.is_some() {}
        Ok(BTreeMap::new())
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
        Ok(BTreeMap::new())
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok(BTreeMap::new())
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(BTreeMap::new())
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok(BTreeMap::new())
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok(BTreeMap::new())
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok(BTreeMap::new())
    }
}

impl<'de> Visitor<'de> for MembersSeed<'de, '_> {
    type Value = Vec<String>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded group members")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        let mut out = Vec::new();
        loop {
            // Probe with IgnoredAny at capacity; never allocate member N+1.
            if out.len() == MAX_NODES {
                if a.next_element::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::custom("group member limit exceeded"));
                }
                break;
            }
            let Some(s) = a.next_element_seed(ScalarSeed(self.0))? else {
                break;
            };
            out.push(s.text);
        }
        Ok(out)
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        while a.next_entry::<de::IgnoredAny, de::IgnoredAny>()?.is_some() {}
        Ok(Vec::new())
    }
    fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
        Ok(Vec::new())
    }
    fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok(Vec::new())
    }
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(Vec::new())
    }
    fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok(Vec::new())
    }
    fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok(Vec::new())
    }
    fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok(Vec::new())
    }
}

impl<'de> Visitor<'de> for EntriesSeed<'de, '_> {
    type Value = Vec<Entry>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded entry sequence")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        let limit = if self.group { MAX_GROUPS } else { MAX_NODES };
        let mut out = Vec::new();
        loop {
            if out.len() == limit {
                if a.next_element::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::custom("entry limit exceeded"));
                }
                break;
            }
            let Some(entry) = a.next_element_seed(EntrySeed {
                context: self.context,
                group: self.group,
            })?
            else {
                break;
            };
            out.push(entry);
        }
        Ok(out)
    }
}

impl<'de> Visitor<'de> for RulesSeed<'de, '_> {
    type Value = Vec<String>;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("bounded string rule sequence")
    }
    fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
        let mut out = Vec::new();
        loop {
            if out.len() == MAX_RULES {
                if a.next_element::<de::IgnoredAny>()?.is_some() {
                    return Err(de::Error::custom("rule limit exceeded"));
                }
                break;
            }
            let Some(s) = a.next_element_seed(ScalarSeed(self.0))? else {
                break;
            };
            if !s.string {
                return Err(de::Error::custom("rule must be a string"));
            }
            out.push(s.text);
        }
        Ok(out)
    }
}

impl<'de> Visitor<'de> for RootSeed<'de, '_> {
    type Value = Parsed;
    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("subscription mapping")
    }
    fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Parsed, A::Error> {
        let mut out = Parsed::default();
        let mut proxies = false;
        while let Some(key) = a.next_key::<String>()? {
            match key.as_str() {
                "proxies" => {
                    proxies = true;
                    out.nodes = a.next_value_seed(EntriesSeed {
                        context: self.0,
                        group: false,
                    })?;
                }
                "proxy-groups" => {
                    out.groups = a.next_value_seed(EntriesSeed {
                        context: self.0,
                        group: true,
                    })?
                }
                "rules" => out.rules = a.next_value_seed(RulesSeed(self.0))?,
                "dns" => {
                    out.fake_ip = a
                        .next_value_seed(FieldsSeed(self.0))?
                        .get("enhanced-mode")
                        .is_some_and(|s| s.text == "fake-ip")
                }
                _ => {
                    a.next_value::<de::IgnoredAny>()?;
                }
            }
        }
        if !proxies {
            return Err(de::Error::custom("proxies sequence is required"));
        }
        Ok(out)
    }
}
