//! Request-invoked, bounded native product observations. No sampler, worker,
//! process adoption, configuration mutation, cached successful DTO, or shell.
//! Public DTOs retain the mature product wire fields. Additional availability
//! fields distinguish empty observations from missing sources and counter gaps.
use crate::product_io::{Backend, Error, Program, timestamp};
use crate::readiness_tun::Budget;
use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json, value::RawValue};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

const FILE_BYTES: usize = 64 << 10;
const SOURCE_BYTES: usize = 256 << 10;
const SMALL_BYTES: usize = 4096;
const MAX_INTERFACES: usize = 128;
const MAX_ROUTES: usize = 1024;
const MAX_DEVICES: usize = 256;
const MAX_SERVICES: usize = 128;
const MAX_SECTIONS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApiError {
    pub status: u16,
    pub code: &'static str,
    pub message: &'static str,
}
impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.message) }
}
impl std::error::Error for ApiError {}
fn api_error(e: Error) -> ApiError {
    ApiError { status: 503, code: match e {
        Error::Cancelled => "observation_cancelled", Error::Deadline => "observation_timeout",
        Error::Limit => "observation_too_large", Error::Invalid => "observation_invalid",
        _ => "observation_unavailable",
    }, message: "Native observation could not be completed." }
}
fn source_code(e: Error) -> &'static str {
    match e { Error::Unavailable => "unavailable", Error::Invalid => "invalid", Error::Limit => "too_large",
        Error::Deadline => "timeout", Error::Cancelled => "canceled", Error::Failed => "read_failed" }
}
fn diagnostic(module: &str, code: &str) -> Value {
    json!({"module":module,"code":code,"message":match code {
        "invalid" => "Observation contains malformed or unsupported data; valid rows were kept.",
        "too_large" => "Observation exceeded its size or row limit.",
        "counter_gap" => "No comparable previous counter sample is available.",
        "partial" => "Only part of the observation source was available.",
        _ => "Observation source is unavailable or could not be read.",
    }})
}
fn safe(s: &str, limit: usize) -> bool {
    s.len() <= limit && !s.chars().any(|c| c.is_control())
}
fn name(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s != "." && s != ".." &&
        s.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c,b'_'|b'-'|b'.'))
}
fn interface(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c,b'_'|b'-'|b'.'|b':'))
}
fn nonnegative(s: &str) -> Result<f64, Error> {
    let n: f64 = s.parse().map_err(|_| Error::Invalid)?;
    if !n.is_finite() || n < 0.0 { Err(Error::Invalid) } else { Ok(n) }
}
fn text(bytes: Vec<u8>) -> Result<String, Error> { String::from_utf8(bytes).map_err(|_| Error::Invalid) }

struct Request<'a, 'b, B: Backend> {
    io: &'a mut B,
    budget: &'a Budget<'b>,
    errors: Vec<Value>,
    availability: BTreeMap<String, Value>,
}
impl<B: Backend> Request<'_, '_, B> {
    fn check(&self) -> Result<(), Error> {
        self.budget.check().map_err(|e| match e {
            crate::readiness_tun::TunError::Cancelled => Error::Cancelled, _ => Error::Deadline,
        })
    }
    fn read(&mut self, path: &str, limit: usize) -> Result<String, Error> {
        self.check()?;
        // The native byte reader refuses leaf symlinks. Resolve only an
        // observed symlink through the bounded metadata seam, as Go's source
        // reader did for generated resolver/version files.
        let selected=match self.io.metadata(Path::new(path),false,self.budget) {
            Ok(meta) if meta.symlink=>resolve(self,Path::new(path))?,
            Ok(_)|Err(Error::Unavailable)=>PathBuf::from(path),Err(e)=>return Err(e),
        };
        let bytes = self.io.read(&selected, limit, self.budget)?;
        self.check()?;
        if bytes.len() > limit { return Err(Error::Limit); }
        text(bytes)
    }
    fn run(&mut self, program: Program, args: &[&str], limit: usize) -> Result<String, Error> {
        self.check()?;
        let args: Vec<String> = args.iter().map(|s| (*s).to_owned()).collect();
        let output = self.io.run(program, &args, None, limit, self.budget)?;
        self.check()?;
        if output.stdout.len() > limit || output.stderr.len() > limit { return Err(Error::Limit); }
        if output.code != 0 { return Err(Error::Failed); }
        text(output.stdout)
    }
    fn list(&mut self, path: &str, limit: usize) -> Result<Vec<String>, Error> {
        self.check()?;
        let rows = self.io.list(Path::new(path), limit, self.budget)?;
        self.check()?;
        if rows.len() > limit { return Err(Error::Limit); }
        if rows.iter().any(|s| !name(s) && !interface(s)) { return Err(Error::Invalid); }
        Ok(rows)
    }
    fn raw(&mut self, path: &str, limit: usize) -> Result<Vec<u8>, Error> {
        self.check()?;
        let bytes = self.io.read(Path::new(path), limit, self.budget)?;
        self.check()?;
        if bytes.len() > limit { return Err(Error::Limit); }
        Ok(bytes)
    }
    fn first(&mut self, paths: &[&str], limit: usize) -> Result<(String, String), Error> {
        for path in paths {
            match self.read(path, limit) {
                Ok(data) => return Ok((data, (*path).to_owned())),
                Err(Error::Unavailable) => (), Err(e) => return Err(e),
            }
        }
        Err(Error::Unavailable)
    }
    fn gap(&mut self, module: &str, source: &str, e: Error) -> Result<(), ApiError> {
        if matches!(e, Error::Deadline | Error::Cancelled) { return Err(api_error(e)); }
        self.errors.push(diagnostic(module, source_code(e)));
        self.availability.insert(module.to_owned(), json!({"source":source,"available":false,"code":source_code(e)}));
        Ok(())
    }
    fn available(&mut self, module: &str, source: &str, bad: Option<Error>) -> Result<(), ApiError> {
        self.availability.insert(module.to_owned(), json!({"source":source,"available":true,"complete":bad.is_none()}));
        if let Some(e) = bad { self.errors.push(diagnostic(module, source_code(e))); }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Counter { rx: u64, tx: u64 }
#[derive(Clone, Copy)]
struct Identity { pid: u32, start: u64 }
/// Retains only one bounded traffic baseline and service PID start identities.
/// Neither map is a public snapshot cache; every GET reads current sources.
pub struct Observations {
    previous: Option<(Instant, BTreeMap<String, Counter>)>,
    identities: BTreeMap<String, Identity>,
}
impl Default for Observations { fn default() -> Self { Self::new() } }
impl Observations {
    pub fn new() -> Self { Self { previous: None, identities: BTreeMap::new() } }
    pub fn get(&mut self, path: &str, query: &str, peer_ip: Option<IpAddr>, io: &mut impl Backend,
        budget: &Budget<'_>) -> Result<Value, ApiError> {
        if !query.is_empty() { return Err(ApiError {status:400,code:"invalid_input",message:"This observation accepts no query parameters."}); }
        let mut r = Request { io, budget, errors: Vec::new(), availability: BTreeMap::new() };
        r.check().map_err(api_error)?;
        let result = match path {
            "/api/system" => system(&mut r),
            "/api/network" => network(&mut r),
            "/api/devices" => {
                let routes = routes(&mut r)?;
                let interfaces = interfaces(&mut r)?;
                let (devices, _) = devices(&mut r, &routes, &interfaces)?;
                let supported = r.availability.get("devices.leases").and_then(|v| v["available"].as_bool()) == Some(true)
                    || r.availability.get("devices.arp").and_then(|v| v["available"].as_bool()) == Some(true);
                Ok(json!({"devices":devices,"supported":supported,"reason":if supported {""} else {"Native lease and ARP sources are unavailable."},
                    "sampledAt":timestamp(r.io.now_unix()),"errors":r.errors,"availability":r.availability}))
            },
            "/api/router" => self.router(&mut r, peer_ip),
            "/api/system/services" => self.services(&mut r),
            "/api/frpc" => self.frpc(&mut r),
            "/api/modules" => self.modules(&mut r),
            _ => Err(ApiError {status:404,code:"not_found",message:"Observation endpoint not found."}),
        }?;
        r.check().map_err(api_error)?;
        Ok(result)
    }
    fn router<B: Backend>(&mut self, r: &mut Request<'_, '_, B>, peer: Option<IpAddr>) -> Result<Value, ApiError> {
        let platform = platform(r)?;
        let routes = routes(r)?;
        let interfaces = interfaces(r)?;
        let (devices, lease_count) = devices(r, &routes, &interfaces)?;
        let wifi = wifi(r)?;
        let resolvers = resolvers(r)?;
        let firewall4 = firewall(r, false)?;
        let firewall6 = firewall(r, true)?;
        let traffic = self.traffic(r)?;
        let wan = wan(r)?;
        let mut result = json!({"platform":platform,"devices":devices,"wifi":wifi,
            "dns":{"resolvers":resolvers,"leaseCount":lease_count},"firewall":{"ipv4":firewall4,"ipv6":firewall6},
            "traffic":traffic,"routes":routes,"interfaces":interfaces,"wan":wan,
            "sampledAt":timestamp(r.io.now_unix()),"errors":r.errors,"availability":r.availability});
        if let Some(ip) = peer { result["currentClientIP"] = json!(ip.to_string()); }
        Ok(result)
    }
    fn traffic<B: Backend>(&mut self, r: &mut Request<'_, '_, B>) -> Result<Vec<Value>, ApiError> {
        let data = match r.read("/proc/net/dev", SOURCE_BYTES) {
            Ok(v) => v, Err(e) => { r.gap("traffic", "/proc/net/dev", e)?; self.previous = None; return Ok(vec![]); }
        };
        let (rows, bad) = parse_counters(&data);
        r.available("traffic", "/proc/net/dev", bad)?;
        let at = Instant::now();
        let elapsed = self.previous.as_ref().map(|p| at.duration_since(p.0).as_secs_f64());
        let mut out = Vec::with_capacity(rows.len());
        for (iface, counter) in &rows {
            let previous = self.previous.as_ref().and_then(|p| p.1.get(iface));
            let rate = match (previous, elapsed) {
                (Some(p), Some(dt)) if dt > 0.0 && counter.rx >= p.rx && counter.tx >= p.tx && bad.is_none() =>
                    Some(((counter.rx-p.rx) as f64/dt,(counter.tx-p.tx) as f64/dt)),
                _ => None,
            };
            let (rx,tx) = rate.unwrap_or((0.0,0.0));
            out.push(json!({"interface":iface,"rxBytes":counter.rx,"txBytes":counter.tx,
                "rxBytesPerSecond":rx,"txBytesPerSecond":tx,"rateAvailable":rate.is_some(),
                "rateIntervalSeconds":if rate.is_some() {elapsed} else {None},"source":"/proc/net/dev"}));
        }
        // Invalid/truncated samples cannot become a baseline for later rates.
        self.previous = if bad.is_none() { Some((at,rows)) } else { None };
        Ok(out)
    }
}

fn system<B: Backend>(r: &mut Request<'_, '_, B>) -> Result<Value, ApiError> {
    let hostname = r.read("/proc/sys/kernel/hostname", SMALL_BYTES).map_err(api_error)?.trim().to_owned();
    let kernel = r.read("/proc/sys/kernel/osrelease", SMALL_BYTES).map_err(api_error)?.trim().to_owned();
    let os = r.read("/proc/sys/kernel/ostype", SMALL_BYTES).map_err(api_error)?.trim().to_lowercase();
    if hostname.is_empty() || kernel.is_empty() || !safe(&hostname,256) || !safe(&kernel,256) || os != "linux" {
        return Err(api_error(Error::Invalid));
    }
    let uptime_text = r.read("/proc/uptime", SMALL_BYTES).map_err(api_error)?;
    let uptime_fields: Vec<_> = uptime_text.split_whitespace().collect();
    if uptime_fields.len() != 2 { return Err(api_error(Error::Invalid)); }
    let uptime = nonnegative(uptime_fields[0]).map_err(api_error)?;
    nonnegative(uptime_fields[1]).map_err(api_error)?;
    let load_text = r.read("/proc/loadavg", SMALL_BYTES).map_err(api_error)?;
    let load_fields: Vec<_> = load_text.split_whitespace().collect();
    if load_fields.len() < 3 { return Err(api_error(Error::Invalid)); }
    let load = [nonnegative(load_fields[0]),nonnegative(load_fields[1]),nonnegative(load_fields[2])];
    let load: Vec<f64> = load.into_iter().collect::<Result<_,_>>().map_err(api_error)?;
    let memory = parse_memory(&r.read("/proc/meminfo", FILE_BYTES).map_err(api_error)?).map_err(api_error)?;
    let stat = r.read("/proc/stat", FILE_BYTES).map_err(api_error)?;
    let mut cpus = BTreeSet::new();
    for line in stat.lines() {
        if let Some(key) = line.split_whitespace().next() {
            if let Some(number) = key.strip_prefix("cpu") {
                if !number.is_empty() && number.bytes().all(|c| c.is_ascii_digit()) {
                    let cpu: u16 = number.parse().map_err(|_| api_error(Error::Invalid))?;
                    if !cpus.insert(cpu) || cpus.len() > 256 { return Err(api_error(Error::Limit)); }
                }
            }
        }
    }
    if cpus.is_empty() { return Err(api_error(Error::Invalid)); }
    let arch = match r.read("/etc/openwrt_release", FILE_BYTES) {
        Ok(data) => assignments(&data, &["DISTRIB_ARCH"]).0.remove("DISTRIB_ARCH").filter(|s| !s.is_empty())
            .unwrap_or_else(|| std::env::consts::ARCH.to_owned()),
        Err(Error::Unavailable) => std::env::consts::ARCH.to_owned(), Err(e) => return Err(api_error(e)),
    };
    Ok(json!({"mode":"host","hostname":hostname,"os":os,"arch":arch,"kernel":kernel,
        "uptimeSeconds":uptime,"cpuCount":cpus.len(),"memory":{"totalBytes":memory.0,"availableBytes":memory.1},
        "load":load,"sampledAt":timestamp(r.io.now_unix()),"source":"Linux procfs"}))
}
fn parse_memory(data: &str) -> Result<(u64,u64),Error> {
    let mut values = BTreeMap::new();
    for line in data.lines() {
        let f: Vec<_> = line.split_whitespace().collect();
        if f.is_empty() || !matches!(f[0],"MemTotal:"|"MemAvailable:") { continue; }
        if f.len() != 3 || f[2] != "kB" { return Err(Error::Invalid); }
        let n: u64 = f[1].parse().map_err(|_| Error::Invalid)?;
        let n = n.checked_mul(1024).ok_or(Error::Invalid)?;
        if values.insert(f[0],n).is_some() { return Err(Error::Invalid); }
    }
    let total = *values.get("MemTotal:").ok_or(Error::Unavailable)?;
    let available = *values.get("MemAvailable:").ok_or(Error::Unavailable)?;
    if total == 0 || available > total { return Err(Error::Invalid); }
    Ok((total,available))
}

#[derive(Default)]
struct Section { kind: String, name: String, options: BTreeMap<String,String>, lists: BTreeMap<String,Vec<String>> }
// UCI files are parsed only as data. Credentials and arbitrary options are not
// retained. No eval/source/shell, and no private bytes enter a public error.
fn words(line: &str) -> Result<Vec<String>,Error> {
    let mut out = Vec::new(); let mut token = String::new(); let mut active=false; let mut quote=None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if let Some(q) = quote {
            if c == q { quote=None; } else if c == '\\' && q == '"' {
                token.push(chars.next().ok_or(Error::Invalid)?);
            } else { token.push(c); }
            continue;
        }
        match c {
            '#' => break, '\''|'"' => { quote=Some(c); active=true; },
            '\\' => { token.push(chars.next().ok_or(Error::Invalid)?); active=true; },
            ' '| '\t' | '\r' => { if active { out.push(std::mem::take(&mut token)); active=false; } },
            _ => { token.push(c); active=true; },
        }
        if token.len() > 4096 || out.len() > 16 { return Err(Error::Limit); }
    }
    if quote.is_some() { return Err(Error::Invalid); }
    if active { out.push(token); }
    Ok(out)
}
fn uci(data: &str, allowed: &[&str]) -> (Vec<Section>,Option<Error>) {
    let mut out: Vec<Section> = Vec::new(); let mut bad=None; let mut counts=BTreeMap::new();
    for line in data.lines() {
        let w = match words(line) { Ok(w) => w, Err(e) => { bad=Some(e);continue; } };
        if w.is_empty() { continue; }
        match w[0].as_str() {
            "config" if (2..=3).contains(&w.len()) && safe(&w[1],64) => {
                if out.len() >= MAX_SECTIONS { bad=Some(Error::Limit);break; }
                let count=counts.entry(w[1].clone()).or_insert(0usize);
                let section_name=if w.len()==3 { w[2].clone() } else {format!("@{}[{}]",w[1],count)}; *count+=1;
                if !safe(&section_name,128) { bad=Some(Error::Invalid);continue; }
                out.push(Section {kind:w[1].clone(),name:section_name,..Section::default()});
            },
            "option"|"list" if w.len()==3 && !out.is_empty() => {
                if !allowed.contains(&w[1].as_str()) { continue; }
                if !safe(&w[2],256) { bad=Some(Error::Invalid);continue; }
                let s=out.last_mut().expect("checked nonempty");
                if w[0]=="option" { s.options.insert(w[1].clone(),w[2].clone()); }
                else { let values=s.lists.entry(w[1].clone()).or_default();
                    if values.len()>=64 {bad=Some(Error::Limit);} else {values.push(w[2].clone());} }
            }, _ => { bad=Some(Error::Invalid); },
        }
    }
    if out.is_empty() {bad=Some(Error::Invalid);}
    (out,bad)
}
fn option<'a>(s: &'a Section, key: &str) -> &'a str { s.options.get(key).map(String::as_str).unwrap_or("") }
fn assignments(data: &str, allowed: &[&str]) -> (BTreeMap<String,String>,Option<Error>) {
    let mut out=BTreeMap::new();let mut bad=None;
    for line in data.lines() {
        let line=line.trim(); if line.is_empty() || line.starts_with('#') {continue;}
        let Some((key,value))=line.split_once('=') else {bad=Some(Error::Invalid);continue;};
        if !allowed.contains(&key.trim()) {continue;}
        match words(value.trim()) {
            Ok(w) if w.len()==1 && safe(&w[0],256) => {out.insert(key.trim().to_owned(),w[0].clone());},
            _=>bad=Some(Error::Invalid),
        }
    }
    (out,bad)
}
fn platform<B: Backend>(r: &mut Request<'_, '_, B>) -> Result<Value,ApiError> {
    let mut model=String::new();let mut firmware=String::new();let mut arch=String::new();
    match r.first(&["/usr/share/xiaoqiang/xiaoqiang_version","/etc/config/version"], FILE_BYTES) {
        Ok((data,source)) => {let (sections,bad)=uci(&data,&["HARDWARE","ROM"]);
            if let Some(s)=sections.iter().find(|s| s.name=="version" || s.kind=="core") {
                model=option(s,"HARDWARE").to_owned(); firmware=option(s,"ROM").to_owned();
            } r.available("platform.version.uci",&source,bad)?;},
        Err(Error::Unavailable)=>(),Err(e)=>r.gap("platform.version.uci","Xiaomi version UCI",e)?,
    }
    if model.is_empty() || firmware.is_empty() {
        match r.first(&["/etc/miwifi_version","/etc/xiaoqiang_version"],FILE_BYTES) {
            Ok((data,source))=>{let (values,bad)=assignments(&data,&["HARDWARE","ROM"]);
                if model.is_empty() {model=values.get("HARDWARE").cloned().unwrap_or_default();}
                if firmware.is_empty() {firmware=values.get("ROM").cloned().unwrap_or_default();}
                r.available("platform.version.assignments",&source,bad)?;},
            Err(Error::Unavailable)=>(),Err(e)=>r.gap("platform.version.assignments","Xiaomi version assignments",e)?,
        }
    }
    if model.is_empty() { match r.read("/tmp/sysinfo/model", SMALL_BYTES) {
        Ok(data) if safe(data.trim(),256) => model=data.trim().to_owned(),
        Ok(_)=>r.gap("platform.model","/tmp/sysinfo/model",Error::Invalid)?,
        Err(Error::Unavailable)=>(),Err(e)=>r.gap("platform.model","/tmp/sysinfo/model",e)?,
    }}
    match r.read("/etc/openwrt_release", FILE_BYTES) {
        Ok(data)=>{let (values,bad)=assignments(&data,&["DISTRIB_ARCH","DISTRIB_RELEASE"]);
            arch=values.get("DISTRIB_ARCH").cloned().unwrap_or_default();
            if firmware.is_empty() {firmware=values.get("DISTRIB_RELEASE").cloned().unwrap_or_default();}
            r.available("platform.release","/etc/openwrt_release",bad)?;},
        Err(Error::Unavailable)=>(),Err(e)=>r.gap("platform.release","/etc/openwrt_release",e)?,
    }
    if model.is_empty() || firmware.is_empty() || arch.is_empty() {
        match r.run(Program::Ubus,&["call","system","board","{}"],FILE_BYTES) {
            Ok(data)=> match raw_object(&data,64) {
                Ok(mut board)=>{
                    if model.is_empty() {model=raw_string(board.remove("model")).unwrap_or_default();}
                    if arch.is_empty() {arch=raw_string(board.remove("system")).unwrap_or_default();}
                    if firmware.is_empty() {
                        if let Some(release)=board.remove("release") {
                            if let Ok(mut release)=raw_object(release.get(),32) {
                                firmware=raw_string(release.remove("version")).unwrap_or_default();
                            }
                        }
                    } r.available("platform.board","ubus system board",None)?;
                },Err(e)=>r.gap("platform.board","ubus system board",e)?,
            },Err(e)=>r.gap("platform.board","ubus system board",e)?,
        }
    }
    if model.is_empty() || firmware.is_empty() {r.gap("platform.version","Xiaomi/OpenWrt platform sources",Error::Unavailable)?;}
    if arch.is_empty() {arch=std::env::consts::ARCH.to_owned();r.available("platform.architecture","native process architecture",None)?;}
    let kernel=match r.read("/proc/sys/kernel/osrelease",SMALL_BYTES) {
        Ok(data) if !data.trim().is_empty() && safe(data.trim(),256)=>{r.available("platform.kernel","/proc/sys/kernel/osrelease",None)?;data.trim().to_owned()},
        Ok(_)=>{r.gap("platform.kernel","/proc/sys/kernel/osrelease",Error::Invalid)?;String::new()},
        Err(e)=>{r.gap("platform.kernel","/proc/sys/kernel/osrelease",e)?;String::new()},
    };
    Ok(json!({"model":model,"firmware":firmware,"kernel":kernel,"architecture":arch}))
}
fn wifi<B: Backend>(r: &mut Request<'_, '_, B>) -> Result<Vec<Value>,ApiError> {
    let data=match r.read("/etc/config/wireless",FILE_BYTES) {Ok(v)=>v,Err(e)=>{r.gap("wifi","/etc/config/wireless",e)?;return Ok(vec![]);}};
    let (sections,mut bad)=uci(&data,&["device","ifname","ssid","band","hwmode","channel","htmode","bw","disabled","encryption"]);
    let radios:BTreeMap<_,_>=sections.iter().filter(|s| s.kind=="wifi-device").map(|s|(s.name.as_str(),s)).collect();
    let mut out=Vec::new();
    for s in sections.iter().filter(|s| s.kind=="wifi-iface") {
        let radio=radios.get(option(s,"device")).copied();
        if radio.is_none() {bad=Some(Error::Invalid);}
        let ro=|key| radio.map(|s| option(s,key)).unwrap_or("");
        let band=match ro("band").to_lowercase().as_str() {
            "2g"|"2.4g"|"2.4ghz"=>"2.4GHz", "5g"|"5ghz"=>"5GHz", "6g"|"6ghz"=>"6GHz",
            _=>{let mode=ro("hwmode").to_lowercase();if mode.ends_with('g') || mode=="11b" {"2.4GHz"} else if mode.ends_with('a') {"5GHz"} else {""}},
        };
        let channel=match ro("channel") {""|"auto"=>0,_=>match ro("channel").parse::<u16>() {Ok(n) if n<=233=>n,_=>{bad=Some(Error::Invalid);0}}};
        let mut disabled=false;
        for value in [option(s,"disabled"),ro("disabled")] {match value {"1"=>disabled=true,""|"0"=>(),_=>bad=Some(Error::Invalid)}}
        let bandwidth=match ro("bw") {""|"0"=>ro("htmode"),v=>v};
        let iface=if option(s,"ifname").is_empty() {s.name.as_str()} else {option(s,"ifname")};
        out.push(json!({"name":iface,"ssid":option(s,"ssid"),"band":band,"channel":channel,
            "bandwidth":bandwidth,"disabled":disabled,"encryption":option(s,"encryption"),
            "source":"/etc/config/wireless","configured":true,"wifi6":bandwidth.starts_with("HE") || bandwidth.starts_with("EHT")}));
    }
    r.available("wifi","/etc/config/wireless",bad)?; Ok(out)
}
fn resolvers<B: Backend>(r: &mut Request<'_, '_, B>) -> Result<Vec<String>,ApiError> {
    let (data,source)=match r.first(&["/tmp/resolv.conf.d/resolv.conf.auto","/tmp/resolv.conf.auto","/etc/resolv.conf"],FILE_BYTES) {
        Ok(v)=>v,Err(e)=>{r.gap("dns","generated upstream resolver files",e)?;return Ok(vec![]);}
    };
    let mut out=BTreeSet::new();let mut bad=None;
    for line in data.lines() {
        let f:Vec<_>=line.split(['#',';']).next().unwrap_or("").split_whitespace().collect();
        if f.first()!=Some(&"nameserver") {continue;}
        if f.len()!=2 {bad=Some(Error::Invalid);continue;}
        match f[1].parse::<IpAddr>() {Ok(ip) if !ip.is_unspecified() && !ip.is_multicast()=>{out.insert(ip.to_string());},_=>bad=Some(Error::Invalid)}
        if out.len()>64 {bad=Some(Error::Limit);break;}
    }
    r.available("dns",&source,bad)?;Ok(out.into_iter().take(64).collect())
}

#[derive(Clone)]
struct Route { family: &'static str, destination: String, gateway: String, iface: String, metric: u64 }
impl Route {
    fn value(&self)->Value {json!({"family":self.family,"destination":self.destination,"gateway":self.gateway,"interface":self.iface,"metric":self.metric})}
}
fn mask4(ip: Ipv4Addr,bits:u8)->Ipv4Addr {
    let mask=if bits==0 {0} else {u32::MAX << (32-bits)};Ipv4Addr::from(u32::from(ip)&mask)
}
fn mask6(ip: Ipv6Addr,bits:u8)->Ipv6Addr {
    let mask=if bits==0 {0} else {u128::MAX << (128-bits)};Ipv6Addr::from(u128::from(ip)&mask)
}
fn v4hex(s:&str)->Result<Ipv4Addr,Error> {
    if s.len()!=8 {return Err(Error::Invalid);} let v=u32::from_str_radix(s,16).map_err(|_|Error::Invalid)?;
    Ok(Ipv4Addr::from(v.to_le_bytes()))
}
fn v6hex(s:&str)->Result<Ipv6Addr,Error> {
    if s.len()!=32 {return Err(Error::Invalid);}let v=u128::from_str_radix(s,16).map_err(|_|Error::Invalid)?;Ok(Ipv6Addr::from(v))
}
fn mask_bits(mask:Ipv4Addr)->Result<u8,Error> {
    let n=u32::from(mask);let bits=n.leading_ones() as u8;
    if n != if bits==0 {0} else {u32::MAX << (32-bits)} {return Err(Error::Invalid);}Ok(bits)
}
fn route4(data:&str)->(Vec<Route>,Option<Error>) {
    let mut lines=data.lines();let header:Vec<_>=lines.next().unwrap_or("").split_whitespace().collect();
    if header.len()!=11 || header[0]!="Iface" || header[1]!="Destination" || header[7]!="Mask" {return (vec![],Some(Error::Invalid));}
    let mut out=Vec::new();let mut bad=None;
    for line in lines {
        let f:Vec<_>=line.split_whitespace().collect();if f.is_empty() {continue;}
        if out.len()>=MAX_ROUTES {bad=Some(Error::Limit);break;}
        let parsed=(||->Result<Option<Route>,Error>{
            if f.len()!=11 || !interface(f[0]) {return Err(Error::Invalid);}
            let dst=v4hex(f[1])?;let gw=v4hex(f[2])?;let bits=mask_bits(v4hex(f[7])?)?;
            let flags=u32::from_str_radix(f[3],16).map_err(|_|Error::Invalid)?;
            let metric=f[6].parse::<u64>().map_err(|_|Error::Invalid)?;
            for i in [4,5,8,9,10] {f[i].parse::<u64>().map_err(|_|Error::Invalid)?;}
            if flags&1==0 || flags&0x200!=0 {return Ok(None);}
            let ip=mask4(dst,bits);Ok(Some(Route {family:"ipv4",destination:format!("{ip}/{bits}"),gateway:gw.to_string(),iface:f[0].to_owned(),metric}))
        })();
        match parsed {Ok(Some(row))=>out.push(row),Ok(None)=>(),Err(e)=>bad=Some(e)}
    }
    (out,bad)
}
fn route6(data:&str)->(Vec<Route>,Option<Error>) {
    let mut out=Vec::new();let mut bad=None;
    for line in data.lines() {
        let f:Vec<_>=line.split_whitespace().collect();if f.is_empty(){continue;}
        if out.len()>=MAX_ROUTES {bad=Some(Error::Limit);break;}
        let parsed=(||->Result<Option<Route>,Error>{
            if f.len()!=10 || !interface(f[9]) {return Err(Error::Invalid);}
            let dst=v6hex(f[0])?;v6hex(f[2])?;let gw=v6hex(f[4])?;
            let bits=u8::from_str_radix(f[1],16).map_err(|_|Error::Invalid)?;
            let srcbits=u8::from_str_radix(f[3],16).map_err(|_|Error::Invalid)?;
            if bits>128 || srcbits>128 {return Err(Error::Invalid);}
            let metric=u64::from_str_radix(f[5],16).map_err(|_|Error::Invalid)?;
            for i in [6,7] {u64::from_str_radix(f[i],16).map_err(|_|Error::Invalid)?;}
            let flags=u32::from_str_radix(f[8],16).map_err(|_|Error::Invalid)?;
            if flags&1==0 || flags&0x200!=0 {return Ok(None);}
            let ip=mask6(dst,bits);Ok(Some(Route {family:"ipv6",destination:format!("{ip}/{bits}"),gateway:gw.to_string(),iface:f[9].to_owned(),metric}))
        })();
        match parsed {Ok(Some(row))=>out.push(row),Ok(None)=>(),Err(e)=>bad=Some(e)}
    }
    (out,bad)
}
fn routes<B: Backend>(r:&mut Request<'_, '_,B>)->Result<Vec<Value>,ApiError> {
    let mut out=Vec::new();
    for (module,path,ipv6) in [("routes.ipv4","/proc/net/route",false),("routes.ipv6","/proc/net/ipv6_route",true)] {
        match r.read(path,SOURCE_BYTES) {
            Ok(data)=>{let (rows,bad)=if ipv6 {route6(&data)} else {route4(&data)};
                out.extend(rows.into_iter().map(|v|v.value()));r.available(module,path,bad)?;},
            Err(e)=>r.gap(module,path,e)?,
        }
    }
    out.sort_by(|a,b|a["family"].as_str().cmp(&b["family"].as_str()).then(a["destination"].as_str().cmp(&b["destination"].as_str()))
        .then(a["interface"].as_str().cmp(&b["interface"].as_str())).then(a["metric"].as_u64().cmp(&b["metric"].as_u64())));
    Ok(out)
}
fn parse_counters(data:&str)->(BTreeMap<String,Counter>,Option<Error>) {
    let mut lines=data.lines();let mut out=BTreeMap::new();let mut bad=None;
    if !lines.next().unwrap_or("").contains("Inter-") || !lines.next().unwrap_or("").contains("bytes") {return (out,Some(Error::Invalid));}
    for line in lines {
        if line.trim().is_empty(){continue;}
        if out.len()>=MAX_INTERFACES {bad=Some(Error::Limit);break;}
        let Some((iface,fields))=line.rsplit_once(':') else {bad=Some(Error::Invalid);continue;};
        let iface=iface.trim();let f:Vec<_>=fields.split_whitespace().collect();
        if !interface(iface) || f.len()!=16 || out.contains_key(iface) {bad=Some(Error::Invalid);continue;}
        let values=f.iter().map(|s|s.parse::<u64>()).collect::<Result<Vec<_>,_>>();
        match values {Ok(v)=>{out.insert(iface.to_owned(),Counter {rx:v[0],tx:v[8]});},Err(_)=>bad=Some(Error::Invalid)}
    }
    (out,bad)
}
fn parse_addresses(data:&str)->(BTreeMap<String,Vec<String>>,Option<Error>) {
    let mut out:BTreeMap<String,Vec<String>>=BTreeMap::new();let mut bad=None;let mut count=0;
    for line in data.lines() {
        let f:Vec<_>=line.split_whitespace().collect();if f.is_empty(){continue;}
        count+=1;if count>MAX_INTERFACES*16 {bad=Some(Error::Limit);break;}
        let parsed=(||->Result<(&str,String),Error>{
            if f.len()<4 || f[0].trim_end_matches(':').parse::<u32>().is_err() {return Err(Error::Invalid);}
            let iface=f[1].split('@').next().unwrap_or("");if !interface(iface) {return Err(Error::Invalid);}
            if f[2]!="inet" && f[2]!="inet6" {return Err(Error::Invalid);}
            let (ip,bits)=f[3].split_once('/').ok_or(Error::Invalid)?;
            let ip:IpAddr=ip.parse().map_err(|_|Error::Invalid)?;let bits:u8=bits.parse().map_err(|_|Error::Invalid)?;
            if (ip.is_ipv4() && (f[2]!="inet" || bits>32)) || (ip.is_ipv6() && (f[2]!="inet6" || bits>128)) {return Err(Error::Invalid);}
            Ok((iface,format!("{ip}/{bits}")))
        })();
        match parsed {Ok((iface,address))=>{
            if !out.contains_key(iface) && out.len()>=MAX_INTERFACES {bad=Some(Error::Limit);break;}
            let addresses=out.entry(iface.to_owned()).or_default();if !addresses.contains(&address) {addresses.push(address);}
        },Err(e)=>bad=Some(e)}
    }
    for addresses in out.values_mut(){addresses.sort();}
    (out,bad)
}
fn interfaces<B: Backend>(r:&mut Request<'_, '_,B>)->Result<Vec<Value>,ApiError> {
    let mut names=BTreeSet::new();let mut addresses=BTreeMap::new();
    match r.list("/sys/class/net",MAX_INTERFACES) {
        Ok(rows)=>{names.extend(rows);r.available("network.interfaces","/sys/class/net",None)?;},
        Err(e)=>r.gap("network.interfaces","/sys/class/net",e)?,
    }
    match r.run(Program::Ip,&["-o","address","show"],SOURCE_BYTES) {
        Ok(data)=>{let (rows,bad)=parse_addresses(&data);names.extend(rows.keys().cloned());addresses=rows;
            r.available("network.addresses","ip -o address show",bad)?;},
        Err(e)=>r.gap("network.addresses","ip -o address show",e)?,
    }
    if names.len()>MAX_INTERFACES {r.gap("network.interfaces","native interface union",Error::Limit)?;}
    let mut out=Vec::new();
    for iface in names.into_iter().take(MAX_INTERFACES) {
        if !interface(&iface) {r.gap("network.interfaces","/sys/class/net",Error::Invalid)?;continue;}
        let (mut up,mut mtu)=(false,0u32);let mut up_available=false;let mut mtu_available=false;
        let prefix=format!("/sys/class/net/{iface}");
        match r.read(&format!("{prefix}/flags"),SMALL_BYTES) {
            Ok(data)=>match u32::from_str_radix(data.trim().trim_start_matches("0x"),16) {
                Ok(flags)=>{up=flags&1!=0;up_available=true;},Err(_)=>r.gap("network.flags",&format!("{prefix}/flags"),Error::Invalid)?,
            },Err(e)=>r.gap("network.flags",&format!("{prefix}/flags"),e)?,
        }
        match r.read(&format!("{prefix}/mtu"),SMALL_BYTES) {
            Ok(data)=>match data.trim().parse::<u32>() {Ok(n) if n>0=>{mtu=n;mtu_available=true;},_=>r.gap("network.mtu",&format!("{prefix}/mtu"),Error::Invalid)?},
            Err(e)=>r.gap("network.mtu",&format!("{prefix}/mtu"),e)?,
        }
        out.push(json!({"name":iface,"addresses":addresses.remove(&iface).unwrap_or_default(),"up":up,"mtu":mtu,
            "upAvailable":up_available,"mtuAvailable":mtu_available,"source":"sysfs + ip address"}));
    }
    Ok(out)
}
fn network<B: Backend>(r:&mut Request<'_, '_,B>)->Result<Value,ApiError> {
    let interfaces=interfaces(r)?;let routes=routes(r)?;let wan=wan(r)?;
    let supported=r.availability.get("routes.ipv4").is_some_and(|v|v["available"]==true)
        || r.availability.get("routes.ipv6").is_some_and(|v|v["available"]==true);
    Ok(json!({"interfaces":interfaces,"routes":routes,"routeObservationSupported":supported,"wan":wan,
        "sampledAt":timestamp(r.io.now_unix()),"errors":r.errors,"availability":r.availability}))
}
fn wan<B: Backend>(r:&mut Request<'_, '_,B>)->Result<Vec<Value>,ApiError> {
    let mut out=Vec::new();
    for iface in ["wan","wan6"] {
        let object=format!("network.interface.{iface}");
        match r.run(Program::Ubus,&["call",&object,"status","{}"],FILE_BYTES) {
            Ok(data)=>match raw_object(&data,64) {
                Ok(mut fields)=>{
                    let up=fields.remove("up").and_then(|v|serde_json::from_str::<bool>(v.get()).ok());
                    let device=raw_string(fields.remove("l3_device")).or_else(||raw_string(fields.remove("device"))).unwrap_or_default();
                    let protocol=raw_string(fields.remove("proto")).unwrap_or_default();
                    if up.is_none() || (!device.is_empty() && !interface(&device)) {r.gap(&format!("wan.{iface}"),&object,Error::Invalid)?;continue;}
                    let uptime=fields.remove("uptime").and_then(|v|serde_json::from_str::<u64>(v.get()).ok());
                    let mut dns=Vec::new();
                    if let Some(raw)=fields.remove("dns-server") {
                        if let Ok(values)=string_array(raw.get()) {
                            if values.len()>64 {r.gap(&format!("wan.{iface}.dns"),&object,Error::Limit)?;}
                            for value in values.into_iter().take(64) {
                                if let Ok(ip)=value.parse::<IpAddr>() {if !ip.is_unspecified() && !ip.is_multicast() {dns.push(ip.to_string());}}
                            }
                        } else {r.gap(&format!("wan.{iface}.dns"),&object,Error::Invalid)?;}
                    }
                    out.push(json!({"interface":iface,"up":up,"device":device,"protocol":protocol,"uptimeSeconds":uptime,
                        "dnsServers":dns,"source":"ubus network.interface status","reachabilityVerified":false}));
                    r.available(&format!("wan.{iface}"),&object,None)?;
                },Err(e)=>r.gap(&format!("wan.{iface}"),&object,e)?,
            },Err(e)=>r.gap(&format!("wan.{iface}"),&object,e)?,
        }
    }
    Ok(out)
}

#[derive(Clone)]
struct Device { ip:Ipv4Addr,mac:String,hostname:String,expiry:Option<u64>,lease:bool,online:bool,iface:String }
fn mac(s:&str)->Result<String,Error> {
    let fields:Vec<_>=s.split(':').collect();if fields.len()!=6 || fields.iter().any(|s|s.len()!=2) {return Err(Error::Invalid);}
    let bytes=fields.iter().map(|s|u8::from_str_radix(s,16)).collect::<Result<Vec<_>,_>>().map_err(|_|Error::Invalid)?;
    if bytes[0]&1!=0 || bytes.iter().all(|b|*b==0) {return Err(Error::Invalid);}
    Ok(bytes.iter().map(|b|format!("{b:02x}")).collect::<Vec<_>>().join(":"))
}
fn device_ip(s:&str)->Result<Ipv4Addr,Error> {
    let ip:Ipv4Addr=s.parse().map_err(|_|Error::Invalid)?;
    if ip.is_unspecified() || ip.is_multicast() || ip.is_broadcast() || ip.is_loopback() {return Err(Error::Invalid);}Ok(ip)
}
fn leases(data:&str,now:u64)->(Vec<Device>,Option<Error>) {
    let mut out=Vec::new();let mut bad=None;let mut seen=BTreeSet::new();
    for line in data.lines() {
        let f:Vec<_>=line.split_whitespace().collect();if f.is_empty(){continue;}
        if out.len()>=MAX_DEVICES {bad=Some(Error::Limit);break;}
        let parsed=(||->Result<Option<Device>,Error>{
            if f.len()!=5 || !safe(f[3],253) {return Err(Error::Invalid);}
            let expiry=f[0].parse::<u64>().map_err(|_|Error::Invalid)?;
            if expiry>253402300799 {return Err(Error::Invalid);}if expiry!=0 && expiry<=now {return Ok(None);}
            let mac=mac(f[1])?;let ip=device_ip(f[2])?;
            if !seen.insert((ip,mac.clone())) {return Err(Error::Invalid);}
            Ok(Some(Device {ip,mac,hostname:if f[3]=="*" {String::new()} else {f[3].to_owned()},expiry:if expiry==0 {None}else{Some(expiry)},
                lease:true,online:false,iface:String::new()}))
        })();match parsed {Ok(Some(d))=>out.push(d),Ok(None)=>(),Err(e)=>bad=Some(e)}
    }
    (out,bad)
}
fn arp(data:&str)->(Vec<Device>,Option<Error>) {
    let mut lines=data.lines();let header=lines.next().unwrap_or("");
    if !header.starts_with("IP address") || !header.contains("HW address") {return (vec![],Some(Error::Invalid));}
    let mut out=Vec::new();let mut bad=None;
    for line in lines {
        let f:Vec<_>=line.split_whitespace().collect();if f.is_empty(){continue;}
        if out.len()>=MAX_DEVICES {bad=Some(Error::Limit);break;}
        let parsed=(||->Result<Option<Device>,Error>{
            if f.len()!=6 || !interface(f[5]) {return Err(Error::Invalid);}
            let ip=device_ip(f[0])?;u32::from_str_radix(f[1].trim_start_matches("0x"),16).map_err(|_|Error::Invalid)?;
            let flags=u32::from_str_radix(f[2].trim_start_matches("0x"),16).map_err(|_|Error::Invalid)?;
            if flags&2==0 {return Ok(None);}let mac=mac(f[3])?;
            Ok(Some(Device {ip,mac,hostname:String::new(),expiry:None,lease:false,online:true,iface:f[5].to_owned()}))
        })();match parsed {Ok(Some(d))=>out.push(d),Ok(None)=>(),Err(e)=>bad=Some(e)}
    }
    (out,bad)
}
fn prefix4(s:&str)->Option<(Ipv4Addr,u8)> {
    let (ip,bits)=s.split_once('/')?;let ip=ip.parse::<Ipv4Addr>().ok()?;let bits=bits.parse::<u8>().ok()?;
    (bits<=32).then_some((ip,bits))
}
fn lan_host(ip:Ipv4Addr,prefixes:&[(Ipv4Addr,u8)])->bool {
    prefixes.iter().any(|(base,bits)| {
        if mask4(ip,*bits)!=mask4(*base,*bits) {return false;}if *bits>=31 {return true;}
        let base=u32::from(mask4(*base,*bits));let broadcast=base|(u32::MAX>>bits);
        u32::from(ip)!=base && u32::from(ip)!=broadcast
    })
}
fn devices<B: Backend>(r:&mut Request<'_, '_,B>,routes:&[Value],interfaces:&[Value])->Result<(Vec<Value>,usize),ApiError> {
    let mut complete=true;let mut lease_rows=Vec::new();let mut arp_rows=Vec::new();
    match r.first(&["/tmp/dhcp.leases","/tmp/dnsmasq.leases","/var/lib/misc/dnsmasq.leases"],FILE_BYTES) {
        Ok((data,source))=>{let (rows,bad)=leases(&data,r.io.now_unix());complete&=bad.is_none();lease_rows=rows;r.available("devices.leases",&source,bad)?;},
        Err(e)=>{complete=false;r.gap("devices.leases","dnsmasq leases",e)?;},
    }
    match r.read("/proc/net/arp",FILE_BYTES) {
        Ok(data)=>{let (rows,bad)=arp(&data);complete&=bad.is_none();arp_rows=rows;r.available("devices.arp","/proc/net/arp",bad)?;},
        Err(e)=>{complete=false;r.gap("devices.arp","/proc/net/arp",e)?;},
    }
    complete&=["routes.ipv4","network.interfaces","network.addresses"].iter().all(|module|
        r.availability.get(*module).is_some_and(|v|v["available"]==true && v["complete"]==true));
    complete&=interfaces.iter().all(|v|v["upAvailable"]==true && v["mtuAvailable"]==true);
    let mut management=BTreeSet::new();let mut prefixes=BTreeSet::new();
    for iface in interfaces {
        if let Some(addresses)=iface["addresses"].as_array() {
            for address in addresses {
                if let Some((ip,bits))=address.as_str().and_then(prefix4) {
                    management.insert(ip);
                    if iface["name"]=="br-lan" && bits>0 && bits<32 && !ip.is_loopback() {prefixes.insert((mask4(ip,bits),bits));}
                }
            }
        }
    }
    for route in routes {
        if route["family"]=="ipv4" && route["interface"]=="br-lan" && route["gateway"]=="0.0.0.0" {
            if let Some((ip,bits))=route["destination"].as_str().and_then(prefix4) {if bits>0 && bits<32 && !ip.is_loopback() {prefixes.insert((mask4(ip,bits),bits));}}
        }
    }
    if prefixes.is_empty() {complete=false;r.gap("devices.scope","actual br-lan IPv4 addresses and connected routes",Error::Unavailable)?;}
    let prefixes:Vec<_>=prefixes.into_iter().collect();
    let mut owners:BTreeMap<Ipv4Addr,BTreeSet<String>>=BTreeMap::new();
    let mut mac_ips:BTreeMap<String,BTreeSet<Ipv4Addr>>=BTreeMap::new();
    let mut lease_counts:BTreeMap<String,usize>=BTreeMap::new();
    let mut arp_interfaces:BTreeMap<(Ipv4Addr,String),BTreeSet<String>>=BTreeMap::new();
    for d in lease_rows.iter().chain(&arp_rows) {owners.entry(d.ip).or_default().insert(d.mac.clone());}
    for d in &lease_rows {*lease_counts.entry(d.mac.clone()).or_default()+=1;}
    for d in &arp_rows {arp_interfaces.entry((d.ip,d.mac.clone())).or_default().insert(d.iface.clone());}
    let lease_count=lease_rows.len();let mut merged:BTreeMap<(Ipv4Addr,String),Device>=BTreeMap::new();
    for d in lease_rows {merged.insert((d.ip,d.mac.clone()),d);}
    for d in arp_rows {
        if let Some(lease)=merged.get_mut(&(d.ip,d.mac.clone())) {lease.online=true;lease.iface=d.iface;}
        else if !lease_counts.contains_key(&d.mac) {
            if merged.len()>=MAX_DEVICES {complete=false;r.gap("devices","lease/ARP merged rows",Error::Limit)?;break;}
            merged.insert((d.ip,d.mac.clone()),d);
        }
    }
    for d in merged.values(){mac_ips.entry(d.mac.clone()).or_default().insert(d.ip);}
    let mut out=Vec::new();
    for (_,mut d) in merged {
        let ifaces=arp_interfaces.get(&(d.ip,d.mac.clone()));
        let eligible=complete && !management.contains(&d.ip) && lan_host(d.ip,&prefixes)
            && owners.get(&d.ip).is_some_and(|s|s.len()==1) && mac_ips.get(&d.mac).is_some_and(|s|s.len()==1)
            && lease_counts.get(&d.mac).copied().unwrap_or(0)<=1
            && ifaces.is_none_or(|s|s.len()==1 && s.contains("br-lan"))
            && (d.lease || (d.iface=="br-lan" && d.online));
        if eligible && d.iface.is_empty() {d.iface="br-lan".to_owned();}
        out.push(json!({"ip":d.ip.to_string(),"mac":d.mac,"hostname":d.hostname,"expiresAt":d.expiry.map(timestamp),
            "online":d.online,"eligible":eligible,"interface":d.iface,"source":if d.lease {"dnsmasq lease + proc ARP"} else {"proc ARP"},
            "onlineSource":"complete /proc/net/arp entry, not a reachability probe"}));
    }
    Ok((out,lease_count))
}

fn firewall<B: Backend>(r:&mut Request<'_, '_,B>,ipv6:bool)->Result<Value,ApiError> {
    let module=if ipv6 {"firewall.ipv6"} else {"firewall.ipv4"};
    let path=if ipv6 {"/proc/net/ip6_tables_names"} else {"/proc/net/ip_tables_names"};
    let program=if ipv6 {Program::Ip6tables} else {Program::Iptables};
    let mut tables=match r.read(path,SMALL_BYTES) {
        Ok(data)=>{let tables:Vec<_>=data.lines().map(str::trim).filter(|s|!s.is_empty()).map(str::to_owned).collect();
            if tables.len()>16 || tables.iter().any(|s|!name(s)) {r.gap(module,path,Error::Invalid)?;vec!["filter".to_owned()]}
            else {tables}},
        Err(Error::Unavailable)=>vec!["filter".to_owned(),"nat".to_owned(),"mangle".to_owned(),"raw".to_owned()],
        Err(e)=>{r.gap(module,path,e)?;vec!["filter".to_owned()]},
    };
    // A loaded table set without filter cannot prove filter policies.
    if !tables.iter().any(|s|s=="filter") {tables.push("filter".to_owned());}
    tables.sort();tables.dedup();
    let mut policies:BTreeMap<String,String>=BTreeMap::new();let mut count=0usize;let mut available=false;let mut complete=true;
    for table in tables {
        match r.run(program,&["-t",&table,"-S"],FILE_BYTES) {
            Ok(data)=>{
                available=true;let mut bad=None;
                for line in data.lines() {
                    let f=match words(line) {Ok(f)=>f,Err(e)=>{bad=Some(e);continue;}};
                    if f.is_empty() || f[0].starts_with('#') {continue;}
                    match f[0].as_str() {
                        "-P" if f.len()==3 && table=="filter" && ["INPUT","FORWARD","OUTPUT"].contains(&f[1].as_str()) => {
                            if ["ACCEPT","DROP"].contains(&f[2].as_str()) {policies.insert(f[1].clone(),f[2].clone());}else{bad=Some(Error::Invalid);}
                        },
                        "-P" if f.len()==3 => (), "-N" if f.len()==2 => (),
                        "-A" if f.len()>=3 => {if count>=4096 {bad=Some(Error::Limit);break;}count+=1;},
                        _=>bad=Some(Error::Invalid),
                    }
                }
                complete&=bad.is_none();r.available(&format!("{module}.{table}"),"native iptables -S",bad)?;
            },Err(e)=>{complete=false;r.gap(&format!("{module}.{table}"),"native iptables -S",e)?;},
        }
    }
    let filter_available=["INPUT","FORWARD","OUTPUT"].iter().all(|s|policies.contains_key(*s));
    if !filter_available {complete=false;r.gap(module,"native filter policies",Error::Unavailable)?;}
    r.availability.insert(module.to_owned(),json!({"source":"native iptables -S","available":available,"complete":complete}));
    Ok(json!({"input":policies.remove("INPUT").unwrap_or_default(),"forward":policies.remove("FORWARD").unwrap_or_default(),
        "output":policies.remove("OUTPUT").unwrap_or_default(),"rules":count,"available":available,
        "ruleCountAvailable":complete,"policyAvailable":filter_available,"source":"native iptables -S"}))
}

// Raw JSON maps reject duplicate keys and retain raw ignored values instead of
// materialising an unbounded private DTO tree. Every source has a byte cap and
// each selected map has a key cap. Private fields are dropped, never debugged.
struct LimitedArray<T,const N:usize>(Vec<T>);
impl<'de,T:Deserialize<'de>,const N:usize> Deserialize<'de> for LimitedArray<T,N> {
    fn deserialize<D:Deserializer<'de>>(deserializer:D)->Result<Self,D::Error> {
        struct ArrayVisitor<T,const N:usize>(std::marker::PhantomData<T>);
        impl<'de,T:Deserialize<'de>,const N:usize> Visitor<'de> for ArrayVisitor<T,N> {
            type Value=LimitedArray<T,N>;
            fn expecting(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {f.write_str("bounded JSON array")}
            fn visit_seq<S:SeqAccess<'de>>(self,mut sequence:S)->Result<Self::Value,S::Error> {
                let mut out=Vec::new();
                while let Some(value)=sequence.next_element::<T>()? {
                    if out.len()>=N {return Err(de::Error::custom("array row limit"));}out.push(value);
                }
                Ok(LimitedArray(out))
            }
        }
        deserializer.deserialize_seq(ArrayVisitor::<T,N>(std::marker::PhantomData))
    }
}
fn string_array(data:&str)->Result<Vec<String>,Error> {
    let LimitedArray(values)=serde_json::from_str::<LimitedArray<String,64>>(data).map_err(|_|Error::Invalid)?;
    if values.iter().any(|s|!safe(s,4096)) {return Err(Error::Invalid);}Ok(values)
}
struct RawObject(BTreeMap<String,Box<RawValue>>);
impl<'de> Deserialize<'de> for RawObject {
    fn deserialize<D:Deserializer<'de>>(deserializer:D)->Result<Self,D::Error> {
        struct ObjectVisitor;
        impl<'de> Visitor<'de> for ObjectVisitor {
            type Value=RawObject;
            fn expecting(&self,f:&mut fmt::Formatter<'_>)->fmt::Result {f.write_str("bounded JSON object")}
            fn visit_map<M:MapAccess<'de>>(self,mut map:M)->Result<Self::Value,M::Error> {
                let mut out=BTreeMap::new();
                while let Some(key)=map.next_key::<String>()? {
                    if out.len()>=MAX_SECTIONS || key.len()>256 || out.contains_key(&key) {return Err(de::Error::custom("invalid bounded object"));}
                    let value=map.next_value::<Box<RawValue>>()?;out.insert(key,value);
                }
                Ok(RawObject(out))
            }
        }
        deserializer.deserialize_map(ObjectVisitor)
    }
}
fn raw_object(data:&str,cap:usize)->Result<BTreeMap<String,Box<RawValue>>,Error> {
    if data.len()>SOURCE_BYTES {return Err(Error::Limit);}
    let RawObject(map)=serde_json::from_str(data).map_err(|_|Error::Invalid)?;
    if map.len()>cap {return Err(Error::Limit);}Ok(map)
}
fn raw_string(raw:Option<Box<RawValue>>)->Option<String> {
    let s=serde_json::from_str::<String>(raw?.get()).ok()?;
    safe(&s,512).then_some(s)
}

struct ServiceInstance { name:String, instance:String, command:String, running:Option<bool>, pid:u32, exit:Option<u8> }
fn service_list(data:&str)->Result<Vec<ServiceInstance>,Error> {
    let services=raw_object(data,MAX_SERVICES)?;let mut out=Vec::new();
    for (service,raw) in services {
        if !name(&service) {return Err(Error::Invalid);}
        let mut fields=raw_object(raw.get(),64)?;
        let instances=match fields.remove("instances") {Some(v)=>raw_object(v.get(),MAX_SERVICES)?,None=>BTreeMap::new()};
        if instances.is_empty() {
            if out.len()>=MAX_SERVICES {return Err(Error::Limit);}
            out.push(ServiceInstance {name:service,instance:String::new(),command:String::new(),running:None,pid:0,exit:None});continue;
        }
        for (instance,raw) in instances {
            if !name(&instance) || out.len()>=MAX_SERVICES {return Err(if out.len()>=MAX_SERVICES {Error::Limit}else{Error::Invalid});}
            let mut fields=raw_object(raw.get(),64)?;
            let running=fields.remove("running").map(|v|serde_json::from_str::<bool>(v.get()).map_err(|_|Error::Invalid)).transpose()?;
            let pid=fields.remove("pid").map(|v|serde_json::from_str::<u32>(v.get()).map_err(|_|Error::Invalid)).transpose()?.unwrap_or(0);
            if pid>1<<22 {return Err(Error::Invalid);}
            let exit=fields.remove("exit_code").map(|v|serde_json::from_str::<u8>(v.get()).map_err(|_|Error::Invalid)).transpose()?;
            let command=match fields.remove("command") {
                Some(v)=>{let commands=string_array(v.get())?;
                    let first=commands.into_iter().next().unwrap_or_default();
                    if !first.is_empty() && (!safe(&first,512) || (!first.starts_with('/') && !name(&first))) {return Err(Error::Invalid);}first},
                None=>String::new(),
            };
            out.push(ServiceInstance {name:service.clone(),instance,command,running,pid,exit});
        }
    }
    Ok(out)
}
#[derive(Clone,Copy)]
struct ProcessStat { pid:u32,state:u8,start:u64 }
fn process_stat(data:&str)->Result<ProcessStat,Error> {
    let open=data.find('(').ok_or(Error::Invalid)?;let close=data.rfind(')').ok_or(Error::Invalid)?;
    if close<=open {return Err(Error::Invalid);}
    let pid=data[..open].trim().parse::<u32>().map_err(|_|Error::Invalid)?;
    let f:Vec<_>=data[close+1..].split_whitespace().collect();
    if pid==0 || f.len()<22 || f[0].len()!=1 || !b"RSDZTWtXxIKP".contains(&f[0].as_bytes()[0]) {return Err(Error::Invalid);}
    let start=f[19].parse::<u64>().map_err(|_|Error::Invalid)?;if start==0 {return Err(Error::Invalid);}
    Ok(ProcessStat {pid,state:f[0].as_bytes()[0],start})
}
fn process_rss(data:&str,pid:u32)->Result<u64,Error> {
    let mut observed_pid=false;let mut rss=None;
    for line in data.lines() {
        let f:Vec<_>=line.split_whitespace().collect();if f.is_empty(){continue;}
        match f[0] {
            "Pid:"=>{if observed_pid || f.len()!=2 || f[1].parse::<u32>().ok()!=Some(pid) {return Err(Error::Invalid);}observed_pid=true;},
            "VmRSS:"=>{if rss.is_some() || f.len()!=3 || f[2]!="kB" {return Err(Error::Invalid);}
                let value=f[1].parse::<u64>().map_err(|_|Error::Invalid)?.checked_mul(1024).ok_or(Error::Invalid)?;rss=Some(value);},
            _=>(),
        }
    }
    if !observed_pid {return Err(Error::Invalid);}rss.ok_or(Error::Unavailable)
}
fn clock_ticks(data:&[u8])->Result<u64,Error> {
    let word=std::mem::size_of::<usize>();
    if data.is_empty() || data.len()%(word*2)!=0 {return Err(Error::Invalid);}
    let value=|slice:&[u8]|->u64 {
        if word==8 {u64::from_ne_bytes(slice.try_into().expect("eight-byte word"))}
        else {u32::from_ne_bytes(slice.try_into().expect("four-byte word")) as u64}
    };
    let mut ticks=None;let mut terminated=false;
    for pair in data.chunks_exact(word*2) {
        let tag=value(&pair[..word]);let n=value(&pair[word..]);
        if tag==0 {terminated=true;break;}
        if tag==17 {if ticks.is_some() || n==0 || n>1<<20 {return Err(Error::Invalid);}ticks=Some(n);}
    }
    if !terminated {return Err(Error::Invalid);}ticks.ok_or(Error::Unavailable)
}
fn valid_path(path:&Path)->bool {
    path.is_absolute() && path.to_str().is_some_and(|s|safe(s,512)) && path.components().all(|c|matches!(c,Component::RootDir|Component::Normal(_)))
}
// Resolve each path component with bounded metadata/readlink. This also handles
// /bin -> /usr/bin and proc exe links, and never treats symlink text as a file.
fn resolve<B: Backend>(r:&mut Request<'_, '_,B>,path:&Path)->Result<PathBuf,Error> {
    if !valid_path(path) {return Err(Error::Invalid);}
    let mut pending=path.to_path_buf();let mut links=0;
    loop {
        let components:Vec<_>=pending.components().filter_map(|c|match c {Component::Normal(n)=>Some(n.to_os_string()),_=>None}).collect();
        let mut current=PathBuf::from("/");let mut replaced=false;
        for (index,component) in components.iter().enumerate() {
            current.push(component);r.check()?;
            let metadata=r.io.metadata(&current,false,r.budget)?;
            if metadata.symlink {
                links+=1;if links>16 {return Err(Error::Limit);}
                let target=r.io.read_link(&current,r.budget)?;
                let mut joined=if target.is_absolute() {target} else {current.parent().ok_or(Error::Invalid)?.join(target)};
                // Normalise in-system relative symlinks, not client paths.
                let mut normalized=PathBuf::from("/");
                for c in joined.components() {match c {Component::RootDir=>normalized=PathBuf::from("/"),
                    Component::CurDir=>(),Component::ParentDir=>{if !normalized.pop(){return Err(Error::Invalid);}},
                    Component::Normal(n)=>normalized.push(n),_=>return Err(Error::Invalid)}}
                joined=normalized;for rest in &components[index+1..] {joined.push(rest);}
                if !valid_path(&joined) {return Err(Error::Invalid);}pending=joined;replaced=true;break;
            }
            if index+1<components.len() && !metadata.directory {return Err(Error::Invalid);}
            if index+1==components.len() && !metadata.regular {return Err(Error::Invalid);}
        }
        if !replaced {r.check()?;return Ok(current);}
    }
}
fn reported_executable<B: Backend>(r:&mut Request<'_, '_,B>,command:&str)->Result<PathBuf,Error> {
    if command.starts_with('/') {return resolve(r,Path::new(command));}
    if !name(command) {return Err(Error::Invalid);}let mut resolved=None;
    for directory in ["/usr/sbin","/usr/bin","/sbin","/bin"] {
        match resolve(r,&Path::new(directory).join(command)) {
            Ok(path)=>{if resolved.as_ref().is_some_and(|p|*p!=path) {return Err(Error::Invalid);}resolved=Some(path);},
            Err(Error::Unavailable)=>(),Err(e)=>return Err(e),
        }
    }
    resolved.ok_or(Error::Unavailable)
}
fn protected(service:&str)->bool {
    matches!(service,"rescue"|"be6500-rescue"|"be6500panel"|"dropbear"|"network"|"wifi"|"firewall")
}
fn row_error(row:&mut Value,code:&str) {row["errorCode"]=json!(code);}
impl Observations {
    fn verify<B: Backend>(&self,r:&mut Request<'_, '_,B>,row:&mut Value,instance:&ServiceInstance,
        clock:Option<(f64,u64)>)->Result<Option<Identity>,ApiError> {
        if instance.pid==0 {
            if instance.exit.is_some_and(|code|code!=0) {row["processState"]=json!("failed");row_error(row,"procd_exit");}
            else if instance.running==Some(false) {row["processState"]=json!("not_running");}
            else {row_error(row,"pid_unavailable");}return Ok(None);
        }
        let prefix=format!("/proc/{}",instance.pid);
        let observation=(||->Result<(ProcessStat,PathBuf,Option<u64>),Error>{
            let first=process_stat(&r.read(&format!("{prefix}/stat"),SMALL_BYTES)?)?;
            if first.pid!=instance.pid {return Err(Error::Invalid);}if b"ZXx".contains(&first.state) {return Err(Error::Failed);}
            if instance.command.is_empty() {return Err(Error::Invalid);}
            let expected=reported_executable(r,&instance.command)?;
            let actual=resolve(r,Path::new(&format!("{prefix}/exe")))?;
            if actual!=expected {return Err(Error::Invalid);}
            let rss=match r.read(&format!("{prefix}/status"),FILE_BYTES) {
                Ok(data)=>process_rss(&data,instance.pid).ok(),Err(Error::Deadline)=>return Err(Error::Deadline),
                Err(Error::Cancelled)=>return Err(Error::Cancelled),Err(_)=>None,
            };
            let after=process_stat(&r.read(&format!("{prefix}/stat"),SMALL_BYTES)?)?;
            let again=resolve(r,Path::new(&format!("{prefix}/exe")))?;
            if after.pid!=first.pid || after.start!=first.start || b"ZXx".contains(&after.state) || again!=actual {return Err(Error::Invalid);}
            r.check()?;Ok((after,actual,rss))
        })();
        match observation {
            Ok((stat,executable,rss))=>{
                let key=format!("{}/{}",instance.name,instance.instance);
                if self.identities.get(&key).is_some_and(|old|old.pid==instance.pid && old.start!=stat.start) {
                    row_error(row,"pid_reused");return Ok(self.identities.get(&key).copied());
                }
                row["processState"]=json!("running");row["pid"]=json!(instance.pid);row["executable"]=json!(executable.to_str().unwrap_or(""));row["startTicks"]=json!(stat.start);
                if let Some(rss)=rss {row["rssBytes"]=json!(rss);}else{row_error(row,"rss_unavailable");}
                if let Some((uptime,ticks))=clock {
                    let seconds=uptime-stat.start as f64/ticks as f64;
                    if seconds>=0.0 && seconds.is_finite() {row["uptimeSeconds"]=json!(seconds);}else{row_error(row,"start_time_invalid");}
                }
                if instance.running==Some(false) {row_error(row,"procd_state_mismatch");}
                Ok(Some(Identity {pid:instance.pid,start:stat.start}))
            },Err(e)=>{
                if matches!(e,Error::Deadline|Error::Cancelled) {return Err(api_error(e));}
                // Loss of source/identity is unknown, never evidence of stopped.
                if e==Error::Failed {row["processState"]=json!("failed");row_error(row,"process_exited");}
                else {row_error(row,if e==Error::Invalid {"identity_mismatch"}else{source_code(e)});}
                Ok(None)
            }
        }
    }
    fn services<B: Backend>(&mut self,r:&mut Request<'_, '_,B>)->Result<Value,ApiError> {
        let checked=timestamp(r.io.now_unix());
        let mut source_error=None;
        let instances=match r.run(Program::Ubus,&["call","service","list","{}"],SOURCE_BYTES) {
            Ok(data)=>match service_list(&data) {Ok(rows)=>{r.available("services.procd","ubus service list",None)?;Some(rows)},
                Err(e)=>{source_error=Some(source_code(e));r.gap("services.procd","ubus service list",e)?;None}},
            Err(e)=>{source_error=Some(source_code(e));r.gap("services.procd","ubus service list",e)?;None},
        };
        let mut installed=BTreeMap::new();let mut install_complete=true;
        match r.list("/etc/init.d",MAX_SERVICES) {
            Ok(names)=>{
                for service in names {
                    if !name(&service) {install_complete=false;r.gap("services.installed","/etc/init.d",Error::Invalid)?;continue;}
                    match r.io.metadata(&Path::new("/etc/init.d").join(&service),true,r.budget) {
                        Ok(meta) if meta.regular=>{installed.insert(service,meta.mode);},Ok(_)=>(),
                        Err(e)=>{install_complete=false;r.gap("services.installed","/etc/init.d",e)?;},
                    }
                }
                r.available("services.installed","/etc/init.d",if install_complete {None}else{Some(Error::Invalid)})?;
            },Err(e)=>{install_complete=false;r.gap("services.installed","/etc/init.d",e)?;},
        }
        let clock=match (r.read("/proc/uptime",SMALL_BYTES),r.raw("/proc/self/auxv",SMALL_BYTES)) {
            (Ok(uptime),Ok(auxv))=>{let f:Vec<_>=uptime.split_whitespace().collect();
                if f.len()==2 {match (nonnegative(f[0]),clock_ticks(&auxv)) {
                    (Ok(uptime),Ok(ticks))=>Some((uptime,ticks)),(Err(e),_)|(_,Err(e))=>{r.gap("services.proc_timing","proc uptime + auxv CLK_TCK",e)?;None},
                }}else{r.gap("services.proc_timing","proc uptime + auxv CLK_TCK",Error::Invalid)?;None}},
            (Err(e),_)|(_,Err(e))=>{r.gap("services.proc_timing","proc uptime + auxv CLK_TCK",e)?;None},
        };
        let list_available=instances.is_some();let mut rows=Vec::new();let mut registered=BTreeSet::new();let mut next=BTreeMap::new();
        if let Some(instances)=instances {
            for instance in instances {
                r.check().map_err(api_error)?;registered.insert(instance.name.clone());
                let configured=if installed.contains_key(&instance.name) {"present"} else if install_complete {"absent"} else {"unknown"};
                let mut row=json!({"name":instance.name,"instance":instance.instance,"configured":configured,"registered":"registered",
                    "processState":"unknown","protected":protected(&instance.name)});
                if let Some(running)=instance.running {row["procdRunning"]=json!(running);}
                if instance.pid>0 {row["reportedPID"]=json!(instance.pid);}
                let identity_key=format!("{}/{}",instance.name,instance.instance);
                let identity=self.verify(r,&mut row,&instance,clock)?;
                // Do not erase a previous start token while procd repeats the
                // same PID but one proc read loses evidence. PID reuse stays
                // unknown until procd withdraws or changes the reported PID.
                if let Some(previous)=self.identities.get(&identity_key).filter(|old|old.pid==instance.pid) {
                    next.insert(identity_key,*previous);
                } else if let Some(identity)=identity {next.insert(identity_key,identity);}
                advertise_actions(&mut row,installed.get(&instance.name).copied(),list_available);
                rows.push(row);
            }
        }
        let mut names:BTreeSet<String>=installed.keys().cloned().collect();
        names.extend(["dnsmasq","dropbear","ddns","be6500panel","be6500-rescue","rescue"].iter().map(|s|(*s).to_owned()));
        let mut truncated=false;
        for service in names {
            if registered.contains(&service) {continue;}if rows.len()>=MAX_SERVICES {truncated=true;break;}
            let configured=if installed.contains_key(&service) {"present"} else if install_complete {"absent"} else {"unknown"};
            let mut row=json!({"name":service,"instance":"","configured":configured,"registered":if list_available {"unregistered"} else {"unknown"},
                "processState":"unknown","protected":protected(&service)});
            advertise_actions(&mut row,installed.get(&service).copied(),list_available);rows.push(row);
        }
        if truncated {source_error=Some("too_large");r.gap("services","procd + init.d union",Error::Limit)?;}
        rows.sort_by(|a,b|a["name"].as_str().cmp(&b["name"].as_str()).then(a["instance"].as_str().cmp(&b["instance"].as_str())));
        if list_available && !truncated {self.identities=next;}
        let mut result=json!({"source":"procd/ubus + proc","sampledAt":if list_available && !truncated {Some(timestamp(r.io.now_unix()))}else{None},
            "checkedAt":checked,"stale":!list_available || truncated,"services":rows,"errors":r.errors,"availability":r.availability});
        if let Some(code)=source_error {result["errorCode"]=json!(code);}Ok(result)
    }
}
fn advertise_actions(row:&mut Value,mode:Option<u32>,fresh:bool) {
    if !fresh || mode.is_none_or(|mode|mode&0o111==0) || row["protected"]==true {return;}
    match row["name"].as_str() {
        Some("dnsmasq")=>{row["actions"]=json!(["reload","restart"]);row["actionImpact"]=json!("DNS/DHCP 服务将短暂中断，设备解析或续租可能受影响。");},
        Some("ddns")=>{let actions=if row["processState"]=="running" {vec!["start","stop","restart","reload"]}else{vec!["start","restart","reload"]};
            row["actions"]=json!(actions);},_=>(),
    }
}

fn public_proxy(values:&BTreeMap<String,String>)->Result<Value,Error> {
    let get=|keys:&[&str]| keys.iter().find_map(|k|values.get(*k)).map(String::as_str).unwrap_or("");
    let proxy_name=get(&["name"]);let kind=get(&["type"]);
    if !name(proxy_name) || !["tcp","udp","http","https"].contains(&kind) {return Err(Error::Invalid);}
    let local=get(&["localIP","local_ip","localAddress"]);
    // frpc's documented localIP default is explicitly qualified as configured
    // default, not presented as a discovered interface or live listener.
    let local_defaulted=local.is_empty();
    let local=if local_defaulted {"127.0.0.1"}else{local};
    if !safe(local,253) || local.is_empty() || local.contains(['/',':','@',' ']) && local.parse::<IpAddr>().is_err() {return Err(Error::Invalid);}
    let port=get(&["localPort","local_port"]).parse::<u16>().map_err(|_|Error::Invalid)?;
    if port==0 {return Err(Error::Invalid);}
    let mut out=json!({"name":proxy_name,"type":kind,"localAddress":local,"localPort":port});
    if local_defaulted {out["localAddressSource"]=json!("frpc configured default");}
    if ["tcp","udp"].contains(&kind) {
        let remote=get(&["remotePort","remote_port"]).parse::<u16>().map_err(|_|Error::Invalid)?;
        if remote==0 {return Err(Error::Invalid);}out["remotePort"]=json!(remote);
    } else {
        let domains=get(&["customDomains","custom_domains","domains"]).split(',').map(str::trim).filter(|s|!s.is_empty()).collect::<Vec<_>>();
        if domains.is_empty() || domains.len()>64 || domains.iter().any(|s|!safe(s,253) || s.contains(['/',':','@',' '])) {return Err(Error::Invalid);}
        out["domains"]=json!(domains);
    }
    Ok(out)
}
fn frpc_json(data:&str)->Result<(Vec<Value>,Option<Error>),Error> {
    let mut root=raw_object(data,64)?;let proxies=root.remove("proxies").ok_or(Error::Invalid)?;
    let LimitedArray(raw)=serde_json::from_str::<LimitedArray<Box<RawValue>,64>>(proxies.get()).map_err(|_|Error::Invalid)?;
    let mut out=Vec::new();let mut bad=None;let mut seen=BTreeSet::new();
    for proxy in raw {
        let parsed=(||->Result<Value,Error>{
            let fields=raw_object(proxy.get(),64)?;let mut values=BTreeMap::new();
            for key in ["name","type","localIP","localAddress","localPort","remotePort","customDomains","domains"] {
                if let Some(raw)=fields.get(key) {
                    let value=match key {"localPort"|"remotePort"=>serde_json::from_str::<u16>(raw.get()).map_err(|_|Error::Invalid)?.to_string(),
                        "customDomains"|"domains"=>string_array(raw.get())?.join(","),
                        _=>serde_json::from_str::<String>(raw.get()).map_err(|_|Error::Invalid)?};
                    if !safe(&value,4096) {return Err(Error::Invalid);}values.insert(key.to_owned(),value);
                }
            }
            public_proxy(&values)
        })();match parsed {
            Ok(proxy) if seen.insert(proxy["name"].as_str().unwrap_or("").to_owned())=>out.push(proxy),
            Ok(_)=>bad=Some(Error::Invalid),Err(e)=>bad=Some(e),
        }
    }
    Ok((out,bad))
}
// Limited public-field projection of standard frpc TOML/legacy INI. No server
// auth/token/TLS/headers or raw document is retained in returned DTOs.
fn frpc_text(data:&str,toml:bool)->Result<(Vec<Value>,Option<Error>),Error> {
    let mut sections:Vec<BTreeMap<String,String>>=Vec::new();let mut active=None;let mut bad=None;
    let mut in_public=false;
    for line in data.lines() {
        let line=line.trim();if line.is_empty() || line.starts_with(['#',';']) {continue;}
        if line.starts_with('[') {
            if toml {
                in_public=line=="[[proxies]]";
                if in_public {if sections.len()>=64 {return Err(Error::Limit);}sections.push(BTreeMap::new());active=Some(sections.len()-1);}
            } else {
                if !line.ends_with(']') {bad=Some(Error::Invalid);continue;}
                let title=&line[1..line.len()-1];in_public=title!="common";
                if in_public {if !name(title) {bad=Some(Error::Invalid);in_public=false;continue;}
                    if sections.len()>=64 {return Err(Error::Limit);}let mut values=BTreeMap::new();values.insert("name".to_owned(),title.to_owned());
                    sections.push(values);active=Some(sections.len()-1);}
            }
            continue;
        }
        if !in_public {continue;}let Some(index)=active else {continue;};
        let Some((key,value))=line.split_once('=') else {bad=Some(Error::Invalid);continue;};let key=key.trim();
        if !["name","type","localIP","local_ip","localAddress","localPort","local_port","remotePort","remote_port","customDomains","custom_domains","domains"].contains(&key) {continue;}
        let value=if toml && ["customDomains","domains"].contains(&key) {
            match string_array(value.trim()) {Ok(v)=>v.join(","),_=>{bad=Some(Error::Invalid);continue;}}
        } else {
            match words(value.trim()) {Ok(w) if w.len()==1=>w[0].clone(),_=>{bad=Some(Error::Invalid);continue;}}
        };
        if !safe(&value,4096) {bad=Some(Error::Invalid);continue;}
        if sections[index].insert(key.to_owned(),value).is_some() {bad=Some(Error::Invalid);}
    }
    let mut out=Vec::new();let mut seen=BTreeSet::new();
    for values in sections {match public_proxy(&values) {
        Ok(v) if seen.insert(v["name"].as_str().unwrap_or("").to_owned())=>out.push(v),
        Ok(_)=>bad=Some(Error::Invalid),Err(e)=>bad=Some(e),
    }}
    Ok((out,bad))
}
impl Observations {
    fn frpc<B: Backend>(&mut self,r:&mut Request<'_, '_,B>)->Result<Value,ApiError> {
        let service_snapshot=self.services(r)?;
        let rows=service_snapshot["services"].as_array().expect("constructed services array");
        let native:Vec<_>=rows.iter().filter(|v|v["name"]=="frpc" || v["name"]=="frp").collect();
        let process_available=native.iter().any(|v|v["processState"]=="running" || v["processState"]=="not_running" || v["processState"]=="failed");
        let running=native.iter().any(|v|v["processState"]=="running");
        let mut proxies=Vec::new();let mut config_available=false;let mut config_source=None;
        match r.first(&["/etc/frpc.json","/etc/frp/frpc.json","/etc/frpc.toml","/etc/frp/frpc.toml","/etc/frpc.ini","/etc/frp/frpc.ini"],FILE_BYTES) {
            Ok((data,source))=>{
                let parsed=if source.ends_with(".json") {frpc_json(&data)}else{frpc_text(&data,source.ends_with(".toml"))};
                match parsed {Ok((rows,bad))=>{config_available=true;proxies=rows;r.available("frpc.config",&source,bad)?;config_source=Some(source);},
                    Err(e)=>r.gap("frpc.config","native frpc public configuration",e)?,}
            },Err(Error::Unavailable)=>r.gap("frpc.config","fixed native frpc configuration paths",Error::Unavailable)?,
            Err(e)=>r.gap("frpc.config","fixed native frpc configuration paths",e)?,
        }
        let supported=service_snapshot["stale"]==false;
        Ok(json!({"supported":supported,"running":running,"reason":if !supported {"Native procd observation is unavailable."}
            else if !process_available {"No currently verified frpc process is available."}else{""},"proxies":proxies,
            "processObservationAvailable":process_available,"configObservationAvailable":config_available,"configSource":config_source,
            "services":native,"sampledAt":timestamp(r.io.now_unix()),"errors":r.errors,"availability":r.availability,
            "remoteHealthVerified":false}))
    }
    fn modules<B: Backend>(&mut self,r:&mut Request<'_, '_,B>)->Result<Value,ApiError> {
        // Capability rows describe current native evidence, not an unconditional
        // ready flag. Root overlays admitted runtime/configuration owners.
        let system_available=match system(r) {
            Ok(_)=>{r.available("system","Linux procfs",None)?;true},
            Err(e) if e.code=="observation_cancelled" || e.code=="observation_timeout"=>return Err(e),
            Err(e)=>{let source_error=match e.code {"observation_too_large"=>Error::Limit,"observation_invalid"=>Error::Invalid,_=>Error::Unavailable};
                r.gap("system","Linux procfs",source_error)?;false},
        };
        let routes=routes(r)?;let interfaces=interfaces(r)?;let (_,_) = devices(r,&routes,&interfaces)?;
        wifi(r)?;resolvers(r)?;firewall(r,false)?;firewall(r,true)?;
        let services=self.services(r)?;
        let available=|module:&str|r.availability.get(module).is_some_and(|v|v["available"]==true);
        let complete=|module:&str|r.availability.get(module).is_some_and(|v|v["available"]==true && v["complete"]==true);
        let native=services["stale"]==false;
        let mut modules=Vec::new();
        for (id,title,description,observe) in [
            ("system","System","Current Linux host metrics and native service observations.",system_available),
            ("network","Network","Actual host interfaces, addresses and forwarding routes.",available("network.interfaces") || available("network.addresses")),
            ("devices","Devices","Current DHCP lease and complete ARP observations.",available("devices.leases") || available("devices.arp")),
            ("wifi","Wi-Fi","Configured Xiaomi/QSDK radios and SSIDs without credentials.",available("wifi")),
            ("dns","DNS","Generated upstream resolver observation.",available("dns")),
            ("firewall","Firewall","Actual native IPv4 and IPv6 policies and rule counts.",available("firewall.ipv4") || available("firewall.ipv6")),
            ("proxy","Proxy","Native proxy runtime integration.",native),
            ("frpc","frpc","Native reverse-tunnel process observation.",native),
        ] {
            let mut capabilities=vec![capability("observe","Native observation",observe,if observe {""}else{"Native observation sources are unavailable."})];
            if id=="network" {
                capabilities.push(capability("interfaces","Host interfaces",observe,"Native interface sources are unavailable."));
                capabilities.push(capability("routes","Route observation",complete("routes.ipv4") || complete("routes.ipv6"),"Native route sources are unavailable or incomplete."));
            }
            if matches!(id,"system"|"network"|"wifi"|"dns"|"firewall") {
                capabilities.push(capability("configure","Configuration management",false,"Configuration ownership is supplied by the integrated control plane."));
            }
            if matches!(id,"proxy"|"frpc") {
                capabilities.push(capability("plan","Configuration plan",false,"Planning ownership is supplied by the integrated control plane."));
                capabilities.push(capability("apply","Native configuration and start/stop",false,"Runtime ownership is supplied by the integrated control plane."));
                capabilities.push(capability("runtime","Runtime control",false,"Runtime ownership is supplied by the integrated control plane."));
            }
            modules.push(json!({"id":id,"title":title,"description":description,"state":if observe {"ready"}else{"unavailable"},"capabilities":capabilities}));
        }
        Ok(json!({"modules":modules,"sampledAt":timestamp(r.io.now_unix()),"errors":r.errors,"availability":r.availability}))
    }
}
fn capability(id:&str,title:&str,supported:bool,reason:&str)->Value {
    let mut result=json!({"id":id,"title":title,"supported":supported});
    if !supported {result["reason"]=json!(reason);}result
}
