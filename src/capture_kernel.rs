//! Read-only fixed capture reservation and installed-state proof. A query can
//! only originate from an internally regenerated plan. No mutation or repair.
use crate::capture_plan::{
    CAPTURE_MARK, CAPTURE_MASK, CAPTURE_PRIORITY, CAPTURE_TABLE, OwnedChain, OwnedRulesPlan,
    RulesPlanInput, plan_owned_rules,
};
use crate::capture_state::{CommandError, CommandResult, MAX_OUTPUT_BYTES};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    net::Ipv4Addr,
    time::Instant,
};
pub type TableNames = BTreeMap<String, u32>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelError {
    Invalid,
    Occupied,
    Missing,
    Unavailable,
    Deadline,
}
impl fmt::Display for KernelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Invalid => "capture observation invalid",
            Self::Occupied => "capture reserved resource is occupied",
            Self::Missing => "owned capture resource missing or changed",
            Self::Unavailable => "capture observation unavailable",
            Self::Deadline => "capture observation deadline exceeded",
        })
    }
}
impl std::error::Error for KernelError {}
type Result<T> = std::result::Result<T, KernelError>;
fn text(raw: &[u8]) -> Result<&str> {
    if raw.len() > MAX_OUTPUT_BYTES {
        return Err(KernelError::Invalid);
    }
    std::str::from_utf8(raw).map_err(|_| KernelError::Invalid)
}
pub fn table_names(raw: &[u8]) -> Result<TableNames> {
    let mut result = TableNames::from([
        ("local".into(), 255),
        ("main".into(), 254),
        ("default".into(), 253),
        ("unspec".into(), 0),
    ]);
    for line in text(raw)?.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 2 || parts[1].len() > 64 {
            return Err(KernelError::Invalid);
        }
        let id = parts[0].parse().map_err(|_| KernelError::Invalid)?;
        if result.get(parts[1]).is_some_and(|old| *old != id) {
            return Err(KernelError::Invalid);
        }
        result.insert(parts[1].into(), id);
        if result.len() > 1024 {
            return Err(KernelError::Invalid);
        }
    }
    Ok(result)
}
fn table(value: &str, names: &TableNames) -> Result<u32> {
    value
        .parse()
        .ok()
        .or_else(|| names.get(value).copied())
        .ok_or(KernelError::Invalid)
}
fn number(value: &str) -> Result<u32> {
    if let Some(value) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        u32::from_str_radix(value, 16).map_err(|_| KernelError::Invalid)
    } else {
        value.parse().map_err(|_| KernelError::Invalid)
    }
}
fn mark(value: &str) -> Result<(u32, u32)> {
    let mut values = value.split('/');
    let value = number(values.next().ok_or(KernelError::Invalid)?)?;
    let mask = values.next().map(number).transpose()?.unwrap_or(u32::MAX);
    if values.next().is_some() {
        return Err(KernelError::Invalid);
    }
    Ok((value, mask))
}
fn option<'a>(args: &'a [String], key: &str) -> Result<Option<&'a str>> {
    let mut found = None;
    for (i, arg) in args.iter().enumerate() {
        if arg == key {
            if found.is_some() {
                return Err(KernelError::Invalid);
            }
            found = Some(args.get(i + 1).ok_or(KernelError::Invalid)?.as_str())
        }
    }
    Ok(found)
}
pub fn split_args(line: &str) -> Result<Vec<String>> {
    if line.len() > 8192 {
        return Err(KernelError::Invalid);
    }
    let mut args = Vec::new();
    let (mut current, mut quote, mut escaped, mut started) = (String::new(), None, false, false);
    for c in line.chars() {
        if c == '\0' || c == '\r' || c == '\n' {
            return Err(KernelError::Invalid);
        }
        if escaped {
            current.push(c);
            escaped = false;
            started = true;
        } else if c == '\\' && quote != Some('\'') {
            escaped = true;
            started = true;
        } else if let Some(q) = quote {
            if c == q {
                quote = None
            } else {
                current.push(c)
            }
            started = true;
        } else if c == '\'' || c == '"' {
            quote = Some(c);
            started = true;
        } else if c == ' ' || c == '\t' {
            if started {
                args.push(std::mem::take(&mut current));
                started = false;
            }
        } else {
            current.push(c);
            started = true;
        }
        if current.len() > 256 || args.len() > 128 {
            return Err(KernelError::Invalid);
        }
    }
    if quote.is_some() || escaped {
        return Err(KernelError::Invalid);
    }
    if started {
        args.push(current)
    }
    Ok(args)
}
fn lines(raw: &[u8]) -> Result<Vec<Vec<String>>> {
    let mut rows = Vec::new();
    for line in text(raw)?.lines() {
        let row = split_args(line)?;
        if !row.is_empty() {
            rows.push(row)
        }
        if rows.len() > 8192 {
            return Err(KernelError::Invalid);
        }
    }
    Ok(rows)
}
fn prefix(value: &str) -> Result<(u32, u8)> {
    let (address, bits) = value.split_once('/').map_or((value, "32"), |pair| pair);
    let address = address
        .parse::<Ipv4Addr>()
        .map_err(|_| KernelError::Invalid)?;
    let bits = bits.parse::<u8>().map_err(|_| KernelError::Invalid)?;
    if bits > 32 {
        return Err(KernelError::Invalid);
    }
    let raw = u32::from(address);
    let mask = if bits == 0 {
        0
    } else {
        u32::MAX << (32 - bits)
    };
    Ok((raw & mask, bits))
}
fn canonical(value: &str) -> Result<String> {
    let (address, bits) = prefix(value)?;
    Ok(format!("{}/{bits}", Ipv4Addr::from(address)))
}
fn overlaps(first: (u32, u8), second: (u32, u8)) -> bool {
    let bits = first.1.min(second.1);
    let mask = if bits == 0 {
        0
    } else {
        u32::MAX << (32 - bits)
    };
    first.0 & mask == second.0 & mask
}
fn destination(args: &[String]) -> Result<(usize, &str, &str)> {
    let first = args.first().ok_or(KernelError::Invalid)?;
    let (kind, index) = if matches!(
        first.as_str(),
        "unicast"
            | "local"
            | "broadcast"
            | "multicast"
            | "anycast"
            | "throw"
            | "unreachable"
            | "prohibit"
            | "blackhole"
            | "nat"
    ) {
        (first.as_str(), 1)
    } else {
        ("unicast", 0)
    };
    let target = args.get(index).ok_or(KernelError::Invalid)?;
    if target != "default" {
        let p = prefix(target)?;
        if target.contains('/') && canonical(target)? != *target {
            return Err(KernelError::Invalid);
        }
        if p.1 == 0 && target != "0.0.0.0/0" {
            return Err(KernelError::Invalid);
        }
    }
    Ok((index, target.as_str(), kind))
}
fn all_routes_absent(raw: &[u8], names: &TableNames) -> Result<()> {
    let mut prior = false;
    for row in lines(raw)? {
        if row[0] == "nexthop" && prior {
            continue;
        }
        destination(&row)?;
        prior = true;
        if let Some(name) = option(&row, "table")?
            && table(name, names)? == CAPTURE_TABLE
        {
            return Err(KernelError::Occupied);
        }
    }
    Ok(())
}
fn policy_unoccupied(raw: &[u8], names: &TableNames) -> Result<()> {
    for row in lines(raw)? {
        let priority = row[0]
            .strip_suffix(':')
            .ok_or(KernelError::Invalid)?
            .parse::<u32>()
            .map_err(|_| KernelError::Invalid)?;
        if priority == CAPTURE_PRIORITY {
            return Err(KernelError::Occupied);
        }
        for key in ["lookup", "table"] {
            if let Some(value) = option(&row, key)?
                && table(value, names)? == CAPTURE_TABLE
            {
                return Err(KernelError::Occupied);
            }
        }
        if let Some(value) = option(&row, "fwmark")?
            && mark(value)?.1 & CAPTURE_MASK != 0
        {
            return Err(KernelError::Occupied);
        }
    }
    Ok(())
}
fn marks_unoccupied(raw: &[u8]) -> Result<()> {
    for row in lines(raw)? {
        let target = option(&row, "-j")?.or(option(&row, "-g")?);
        let Some(target) = target else { continue };
        if !matches!(target, "MARK" | "CONNMARK" | "TPROXY") {
            continue;
        }
        let (mut writes, mut recognized) = (0u32, false);
        for key in [
            "--set-xmark",
            "--set-mark",
            "--tproxy-mark",
            "--or-mark",
            "--xor-mark",
            "--and-mark",
        ] {
            if let Some(raw) = option(&row, key)? {
                let (value, mask) = mark(raw)?;
                recognized = true;
                writes |= match key {
                    "--or-mark" | "--xor-mark" => value,
                    "--and-mark" => !value,
                    _ => mask | value,
                };
            }
        }
        if target == "CONNMARK"
            && row
                .iter()
                .any(|item| item == "--save-mark" || item == "--restore-mark")
        {
            recognized = true;
            for key in ["--nfmask", "--ctmask"] {
                writes |= option(&row, key)?
                    .map(number)
                    .transpose()?
                    .unwrap_or(u32::MAX);
            }
        }
        if !recognized && target != "TPROXY" {
            return Err(KernelError::Invalid);
        }
        if writes & CAPTURE_MASK != 0 {
            return Err(KernelError::Occupied);
        }
    }
    Ok(())
}
fn tun_routes_unoccupied(raw: &[u8], plan: &OwnedRulesPlan, names: &TableNames) -> Result<()> {
    let own = &plan.ownership;
    let network = prefix(&own.tun_address)?;
    let local = own
        .tun_address
        .split('/')
        .next()
        .ok_or(KernelError::Invalid)?
        .parse::<Ipv4Addr>()
        .map_err(|_| KernelError::Invalid)?;
    let local = u32::from(local);
    for row in lines(raw)? {
        if row[0] == "nexthop" {
            continue;
        }
        let (_, dst, kind) = destination(&row)?;
        let device = option(&row, "dev")?;
        if dst == "default" || dst == "0.0.0.0/0" {
            if device == Some(own.tun_interface.as_str()) {
                return Err(KernelError::Occupied);
            }
            continue;
        }
        let p = prefix(dst)?;
        if !overlaps(network, p) {
            continue;
        }
        let route_table = option(&row, "table")?
            .map(|v| table(v, names))
            .transpose()?
            .unwrap_or(254);
        let scope = option(&row, "scope")?;
        if device != Some(own.tun_interface.as_str())
            || option(&row, "proto")? != Some("kernel")
            || option(&row, "via")?.is_some()
            || option(&row, "src")?.is_some_and(|v| v != Ipv4Addr::from(local).to_string())
        {
            return Err(KernelError::Occupied);
        }
        let allowed = (kind == "unicast"
            && p == network
            && scope == Some("link")
            && route_table == 254)
            || (kind == "local" && p == (local, 32) && scope == Some("host") && route_table == 255)
            || (kind == "broadcast"
                && p.1 == 32
                && (p.0 == network.0 || p.0 == network.0 + 3)
                && scope == Some("link")
                && route_table == 255);
        if !allowed {
            return Err(KernelError::Occupied);
        }
    }
    Ok(())
}
fn query<R>(runner: &mut R, args: &[String], deadline: Instant) -> Result<CommandResult>
where
    R: FnMut(&[String], Instant) -> std::result::Result<CommandResult, CommandError>,
{
    if Instant::now() >= deadline {
        return Err(KernelError::Deadline);
    }
    let result = runner(args, deadline).map_err(|error| match error {
        CommandError::Timeout | CommandError::Cancelled => KernelError::Deadline,
        _ => KernelError::Unavailable,
    })?;
    if Instant::now() >= deadline {
        return Err(KernelError::Deadline);
    }
    text(&result.output)?;
    Ok(result)
}
fn cmd(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}
fn chain_query(chain: &OwnedChain, hook: bool) -> Vec<String> {
    cmd(&[
        "iptables",
        "-w",
        "5",
        "-t",
        &chain.table,
        "-S",
        if hook { &chain.hook } else { &chain.name },
    ])
}
fn absent_chain(result: &CommandResult) -> bool {
    !result.success
        && std::str::from_utf8(&result.output)
            .ok()
            .is_some_and(|text| {
                matches!(
                    text.trim(),
                    "iptables: No chain/target/match by that name."
                        | "No chain/target/match by that name."
                )
            })
}
fn capture_table<R>(runner: &mut R, deadline: Instant, names: &TableNames) -> Result<Vec<u8>>
where
    R: FnMut(&[String], Instant) -> std::result::Result<CommandResult, CommandError>,
{
    let result = query(
        runner,
        &cmd(&["ip", "-4", "route", "show", "table", "16500"]),
        deadline,
    )?;
    if result.success {
        return Ok(result.output);
    }
    let message = std::str::from_utf8(&result.output)
        .map_err(|_| KernelError::Invalid)?
        .trim();
    if matches!(
        message,
        "Error: ipv4: FIB table does not exist."
            | "Error: ipv4: FIB table does not exist.\nDump terminated"
    ) {
        return Ok(vec![]);
    }
    if message == "Dump terminated" {
        let all = query(
            runner,
            &cmd(&["ip", "-4", "route", "show", "table", "all"]),
            deadline,
        )?;
        if !all.success {
            return Err(KernelError::Unavailable);
        }
        all_routes_absent(&all.output, names)?;
        return Ok(vec![]);
    }
    Err(KernelError::Unavailable)
}
/// Caller must separately establish actual retained core/TUN readiness. This
/// function proves only fixed mark/table/chain reservations and route collision.
pub fn preflight<R>(
    input: &RulesPlanInput,
    names: &TableNames,
    mut runner: R,
    deadline: Instant,
) -> Result<()>
where
    R: FnMut(&[String], Instant) -> std::result::Result<CommandResult, CommandError>,
{
    let plan = plan_owned_rules(input).map_err(|_| KernelError::Invalid)?;
    let routes = query(
        &mut runner,
        &cmd(&["ip", "-4", "route", "show", "table", "all"]),
        deadline,
    )?;
    if !routes.success {
        return Err(KernelError::Unavailable);
    }
    tun_routes_unoccupied(&routes.output, &plan, names)?;
    if !text(&capture_table(&mut runner, deadline, names)?)?
        .trim()
        .is_empty()
    {
        return Err(KernelError::Occupied);
    }
    let rules = query(&mut runner, &cmd(&["ip", "-4", "rule", "show"]), deadline)?;
    if !rules.success {
        return Err(KernelError::Unavailable);
    }
    policy_unoccupied(&rules.output, names)?;
    let marks = query(
        &mut runner,
        &cmd(&["iptables", "-w", "5", "-t", "mangle", "-S"]),
        deadline,
    )?;
    if !marks.success {
        return Err(KernelError::Unavailable);
    }
    marks_unoccupied(&marks.output)?;
    for chain in &plan.ownership.chains {
        let result = query(&mut runner, &chain_query(chain, false), deadline)?;
        if result.success {
            return Err(KernelError::Occupied);
        }
        if !absent_chain(&result) {
            return Err(KernelError::Unavailable);
        }
    }
    Ok(())
}
fn normalize_rule(args: &[String]) -> Result<String> {
    if args.len() < 4 || args[0] != "-A" {
        return Err(KernelError::Invalid);
    }
    let mut values = BTreeMap::new();
    let mut modules = BTreeSet::new();
    let protocol = option(args, "-p")?;
    for pair in args[2..].chunks(2) {
        if pair.len() != 2 {
            return Err(KernelError::Invalid);
        }
        let (key, value) = (pair[0].as_str(), pair[1].as_str());
        if key == "-m" {
            if Some(value) == protocol && matches!(value, "tcp" | "udp") {
                continue;
            }
            if !matches!(value, "addrtype" | "mac" | "mark") || !modules.insert(value) {
                return Err(KernelError::Invalid);
            }
            continue;
        }
        if values.contains_key(key) {
            return Err(KernelError::Invalid);
        }
        let normalized = match key {
            "-s" | "-d" => canonical(value)?,
            "--set-xmark" | "--mark" => {
                let (v, m) = mark(value)?;
                format!("0x{v:x}/0x{m:x}")
            }
            "--dport" | "--to-ports" => {
                let p = value.parse::<u16>().map_err(|_| KernelError::Invalid)?;
                if p == 0 {
                    return Err(KernelError::Invalid);
                }
                p.to_string()
            }
            "-p" if matches!(value, "tcp" | "udp") => value.into(),
            "--dst-type" if value == "LOCAL" => value.into(),
            "-j" | "-i" | "-o" => value.into(),
            "--mac-source" => value.to_ascii_lowercase(),
            _ => return Err(KernelError::Invalid),
        };
        values.insert(key, normalized);
    }
    Ok(format!("{} {:?} {:?}", args[1], modules, values))
}
fn verify_chain(raw: &[u8], chain: &OwnedChain, plan: &OwnedRulesPlan) -> Result<()> {
    let mut expected = Vec::new();
    for argv in &plan.apply {
        if argv.len() > 6
            && argv[0] == "iptables"
            && argv[4] == chain.table
            && argv[5] == "-A"
            && argv[6] == chain.name
        {
            expected.push(normalize_rule(&argv[5..])?)
        }
    }
    let mut actual = Vec::new();
    let mut declared = false;
    for row in lines(raw)? {
        if row == ["-N".to_owned(), chain.name.clone()] && !declared && actual.is_empty() {
            declared = true;
            continue;
        }
        if row.get(1) != Some(&chain.name) {
            return Err(KernelError::Missing);
        }
        actual.push(normalize_rule(&row)?)
    }
    if !declared || actual != expected {
        return Err(KernelError::Missing);
    }
    Ok(())
}
fn verify_hooks(raw: &[u8], chain: &OwnedChain, plan: &OwnedRulesPlan) -> Result<()> {
    let mut expected = Vec::new();
    for argv in plan.apply.iter().rev() {
        if argv.len() > 8
            && argv[0] == "iptables"
            && argv[4] == chain.table
            && argv[5] == "-I"
            && argv[6] == chain.hook
        {
            let mut row = argv[5..].to_vec();
            row[0] = "-A".into();
            row.remove(2);
            expected.push(normalize_rule(&row)?);
        }
    }
    let reserved = plan
        .ownership
        .chains
        .iter()
        .map(|owned| owned.name.as_str())
        .collect::<BTreeSet<_>>();
    let mut actual = Vec::new();
    for row in lines(raw)? {
        if row.first().is_some_and(|op| op == "-A")
            && row.get(1) == Some(&chain.hook)
            && option(&row, "-j")?
                .or(option(&row, "-g")?)
                .is_some_and(|target| reserved.contains(target))
        {
            actual.push(normalize_rule(&row)?);
        }
    }
    if actual != expected {
        return Err(KernelError::Missing);
    }
    Ok(())
}
fn verify_route(raw: &[u8], plan: &OwnedRulesPlan, names: &TableNames) -> Result<()> {
    let rows = lines(raw)?;
    if rows.len() != 1 {
        return Err(KernelError::Missing);
    }
    let row = &rows[0];
    if row.len() < 3
        || !matches!(row[0].as_str(), "default" | "0.0.0.0/0")
        || row[1] != "dev"
        || row[2] != plan.ownership.tun_interface
    {
        return Err(KernelError::Missing);
    }
    for pair in row[3..].chunks(2) {
        if pair.len() != 2 {
            return Err(KernelError::Invalid);
        }
        match pair[0].as_str() {
            "scope" if pair[1] == "link" => {}
            "table" if table(&pair[1], names)? == CAPTURE_TABLE => {}
            "proto" if matches!(pair[1].as_str(), "boot" | "static") => {}
            _ => return Err(KernelError::Missing),
        }
    }
    Ok(())
}
fn verify_rules(raw: &[u8], plan: &OwnedRulesPlan, names: &TableNames) -> Result<()> {
    let own = &plan.ownership;
    let mut remaining = if own.scope == "gateway" {
        own.lan_ipv4_prefixes
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
    } else {
        own.client_ipv4s
            .iter()
            .chain(if own.client_ipv4.is_empty() {
                None
            } else {
                Some(&own.client_ipv4)
            })
            .map(|value| canonical(value))
            .collect::<Result<BTreeSet<_>>>()?
    };
    if remaining.is_empty() {
        return Err(KernelError::Missing);
    }
    for row in lines(raw)? {
        let priority = row[0]
            .strip_suffix(':')
            .ok_or(KernelError::Invalid)?
            .parse::<u32>()
            .map_err(|_| KernelError::Invalid)?;
        let is_candidate = priority == CAPTURE_PRIORITY
            || option(&row, "lookup")?
                .or(option(&row, "table")?)
                .map(|value| table(value, names))
                .transpose()?
                == Some(CAPTURE_TABLE)
            || option(&row, "fwmark")?
                .map(mark)
                .transpose()?
                .is_some_and(|(_, mask)| mask & CAPTURE_MASK != 0);
        if !is_candidate {
            continue;
        }
        if priority != CAPTURE_PRIORITY || row.len() != 9 {
            return Err(KernelError::Missing);
        }
        let mut values = BTreeMap::new();
        for pair in row[1..].chunks(2) {
            let key = if pair[0] == "table" {
                "lookup"
            } else {
                pair[0].as_str()
            };
            if !matches!(key, "from" | "iif" | "fwmark" | "lookup")
                || values.insert(key, pair[1].as_str()).is_some()
            {
                return Err(KernelError::Missing);
            }
        }
        let source = canonical(values.get("from").ok_or(KernelError::Missing)?)?;
        if values.get("iif") != Some(&own.lan_interface.as_str())
            || mark(values.get("fwmark").ok_or(KernelError::Missing)?)?
                != (CAPTURE_MARK, CAPTURE_MASK)
            || table(values.get("lookup").ok_or(KernelError::Missing)?, names)? != CAPTURE_TABLE
            || !remaining.remove(&source)
        {
            return Err(KernelError::Missing);
        }
    }
    if !remaining.is_empty() {
        return Err(KernelError::Missing);
    }
    Ok(())
}
/// One read-only sample. Missing/error remains uncertainty, not automatic repair
/// or proof that a retained cleanup journal is empty.
pub fn observe_installed<R>(
    input: &RulesPlanInput,
    names: &TableNames,
    mut runner: R,
    deadline: Instant,
) -> Result<()>
where
    R: FnMut(&[String], Instant) -> std::result::Result<CommandResult, CommandError>,
{
    let plan = plan_owned_rules(input).map_err(|_| KernelError::Invalid)?;
    for chain in &plan.ownership.chains {
        let result = query(&mut runner, &chain_query(chain, false), deadline)?;
        if !result.success {
            return Err(KernelError::Missing);
        }
        verify_chain(&result.output, chain, &plan)?;
    }
    let mut done = BTreeSet::new();
    for chain in &plan.ownership.chains {
        if done.insert((chain.table.clone(), chain.hook.clone())) {
            let result = query(&mut runner, &chain_query(chain, true), deadline)?;
            if !result.success {
                return Err(KernelError::Missing);
            }
            verify_hooks(&result.output, chain, &plan)?;
        }
    }
    verify_route(&capture_table(&mut runner, deadline, names)?, &plan, names)?;
    let rules = query(&mut runner, &cmd(&["ip", "-4", "rule", "show"]), deadline)?;
    if !rules.success {
        return Err(KernelError::Unavailable);
    }
    verify_rules(&rules.output, &plan, names)
}
