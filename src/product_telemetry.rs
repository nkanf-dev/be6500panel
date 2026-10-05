//! Native product telemetry. The server owns one instance and calls `tick` from
//! its cooperative request lane. There are no threads, subscriptions or shell
//! commands. WANRING files and device-names.json remain Go-format compatible.
use crate::http::Method;
use crate::product_io::{Backend, Program, timestamp};
use crate::readiness_tun::Budget;
use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Write};
use std::net::{IpAddr, SocketAddr, TcpStream};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SOURCE_BYTES: usize = 512 << 10;
const CORE_BYTES: usize = 1 << 20;
const ANNOTATION_BYTES: u64 = 3 << 20;
const RESERVE: u64 = 1 << 20;
const MAX_REVISION: u64 = (1 << 53) - 1;
const LAYOUTS: [(u64, usize); 3] = [(30, 5760), (300, 8640), (3600, 9600)];
const RANGES: [(&str, u64); 12] = [
    ("30m", 1800),
    ("1h", 3600),
    ("3h", 10800),
    ("6h", 21600),
    ("10h", 36000),
    ("12h", 43200),
    ("1d", 86400),
    ("3d", 259200),
    ("7d", 604800),
    ("30d", 2592000),
    ("180d", 15552000),
    ("1y", 31536000),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiError {
    pub status: u16,
    pub code: &'static str,
    pub message: &'static str,
}
impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.message)
    }
}
impl std::error::Error for ApiError {}
fn error(status: u16, code: &'static str, message: &'static str) -> ApiError {
    ApiError {
        status,
        code,
        message,
    }
}
fn invalid() -> ApiError {
    error(
        400,
        "invalid_input",
        "Provide valid bounded endpoint parameters.",
    )
}
fn storage() -> ApiError {
    error(
        500,
        "storage_failed",
        "Cannot persist or load device annotations.",
    )
}
fn check(b: &Budget<'_>) -> Result<(), ApiError> {
    b.check().map_err(|_| {
        error(
            408,
            "cancelled",
            "Telemetry operation was cancelled or exceeded its deadline.",
        )
    })
}
fn text(s: &str, chars: usize) -> bool {
    s.chars().count() <= chars && !s.chars().any(char::is_control)
}
fn bounded_text(s: &str, n: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(n).collect()
}
fn interface(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
}
fn canonical_mac(s: &str) -> Option<String> {
    let compact: String = if s.len() == 14
        && s.as_bytes().get(4) == Some(&b'.')
        && s.as_bytes().get(9) == Some(&b'.')
    {
        s.chars().filter(|c| *c != '.').collect()
    } else if s.len() == 17
        && (s.as_bytes().get(2) == Some(&b':') || s.as_bytes().get(2) == Some(&b'-'))
    {
        let sep = s.as_bytes()[2];
        if !(0..5).all(|i| s.as_bytes()[2 + i * 3] == sep) {
            return None;
        }
        s.chars().filter(|c| *c != char::from(sep)).collect()
    } else {
        return None;
    };
    if compact.len() != 12 || !compact.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(
        (0..6)
            .map(|i| compact[i * 2..i * 2 + 2].to_ascii_uppercase())
            .collect::<Vec<_>>()
            .join(":"),
    )
}
fn safe_ip(s: &str) -> String {
    s.parse::<IpAddr>()
        .map(|v| v.to_string())
        .unwrap_or_default()
}
fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in data {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ ((0u32.wrapping_sub(crc & 1)) & 0xedb88320);
        }
    }
    !crc
}
fn le64(raw: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(raw[at..at + 8].try_into().unwrap())
}
fn le32(raw: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(raw[at..at + 4].try_into().unwrap())
}
fn put64(raw: &mut [u8], at: usize, v: u64) {
    raw[at..at + 8].copy_from_slice(&v.to_le_bytes());
}
fn put32(raw: &mut [u8], at: usize, v: u32) {
    raw[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn regular(path: &Path, max: u64) -> Result<Option<u64>, ApiError> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() && m.len() <= max => Ok(Some(m.len())),
        Ok(_) => Err(storage()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(storage()),
    }
}
fn open_file(path: &Path, write: bool) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(write)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}
fn admit(dir: &Path, growth: u64) -> Result<(), ApiError> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let name = CString::new(dir.as_os_str().as_bytes()).map_err(|_| storage())?;
    let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(name.as_ptr(), stat.as_mut_ptr()) } != 0 {
        return Err(error(
            507,
            "storage_insufficient",
            "Persistent storage free space could not be measured.",
        ));
    }
    let stat = unsafe { stat.assume_init() };
    #[allow(clippy::unnecessary_cast)] // ARM32 statvfs word, Darwin u64.
    let block = (stat.f_frsize as u64).max(1);
    #[allow(clippy::unnecessary_cast)] // libc word width differs on ARM32/Darwin/Linux.
    let free = (stat.f_bavail as u64)
        .checked_mul(block)
        .ok_or_else(storage)?;
    admit_free(free, block, growth)
}
fn admit_free(free: u64, block: u64, growth: u64) -> Result<(), ApiError> {
    let block = block.max(1);
    let rounded = growth
        .checked_add(block - 1)
        .and_then(|v| v.checked_div(block))
        .and_then(|v| v.checked_mul(block))
        .ok_or_else(storage)?;
    let required = rounded
        .checked_add(RESERVE)
        .and_then(|v| v.checked_add(4096))
        .ok_or_else(storage)?;
    if free < required {
        Err(error(
            507,
            "storage_insufficient",
            "Persistent storage needs free space for safe configuration recovery.",
        ))
    } else {
        Ok(())
    }
}
fn temp_file(dir: &Path, prefix: &str) -> Result<(PathBuf, File), ApiError> {
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|_| storage())?;
    let name = nonce.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let path = dir.join(format!(".{prefix}-{name}"));
    let f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&path)
        .map_err(|_| storage())?;
    Ok((path, f))
}

#[derive(Clone, Copy, Default)]
struct Bucket {
    start: u64,
    rx: u64,
    tx: u64,
    coverage: u64,
    rx_peak: f64,
    tx_peak: f64,
    generation: u32,
    end_ms: u32,
}
impl Bucket {
    fn decode(raw: &[u8; 64], seconds: u64, capacity: usize, index: usize) -> Option<Self> {
        if le32(raw, 60) != 0x54465242 || le32(raw, 56) != crc32(&raw[..56]) {
            return None;
        }
        let b = Self {
            generation: le32(raw, 0),
            end_ms: le32(raw, 4),
            start: le64(raw, 8),
            rx: le64(raw, 16),
            tx: le64(raw, 24),
            coverage: le64(raw, 32),
            rx_peak: f64::from_bits(le64(raw, 40)),
            tx_peak: f64::from_bits(le64(raw, 48)),
        };
        (b.generation > 0
            && b.start <= 253402300799
            && b.start.is_multiple_of(seconds)
            && (b.start / seconds) % capacity as u64 == index as u64
            && b.end_ms > 0
            && u64::from(b.end_ms) <= seconds * 1000
            && b.coverage <= seconds * 1_000_000_000
            && b.rx_peak.is_finite()
            && b.tx_peak.is_finite()
            && b.rx_peak >= 0.0
            && b.tx_peak >= 0.0
            && (b.coverage > 0 || (b.rx == 0 && b.tx == 0)))
            .then_some(b)
    }
    fn encode(self) -> [u8; 64] {
        let mut raw = [0u8; 64];
        put32(&mut raw, 0, self.generation);
        put32(&mut raw, 4, self.end_ms);
        put64(&mut raw, 8, self.start);
        put64(&mut raw, 16, self.rx);
        put64(&mut raw, 24, self.tx);
        put64(&mut raw, 32, self.coverage);
        put64(&mut raw, 40, self.rx_peak.to_bits());
        put64(&mut raw, 48, self.tx_peak.to_bits());
        let sum = crc32(&raw[..56]);
        put32(&mut raw, 56, sum);
        put32(&mut raw, 60, 0x54465242);
        raw
    }
}
struct Ring {
    file: File,
    seconds: u64,
    capacity: usize,
    dirty: BTreeMap<usize, Bucket>,
    recovered: bool,
    complete: bool,
}
impl Ring {
    fn size(capacity: usize) -> u64 {
        64 + capacity as u64 * 128
    }
    fn create(dir: &Path, path: &Path, seconds: u64, capacity: usize) -> Result<(), ApiError> {
        let (tmp, mut f) = temp_file(dir, "wan-ring")?;
        let result = (|| {
            let mut h = [0u8; 64];
            h[..8].copy_from_slice(b"WANRING\0");
            put64(&mut h, 8, seconds);
            put64(&mut h, 16, capacity as u64);
            let crc = crc32(&h[..60]);
            put32(&mut h, 60, crc);
            f.write_all(&h).map_err(|_| storage())?;
            let zeros = [0u8; 32768];
            let mut left = capacity * 128;
            while left > 0 {
                let n = left.min(zeros.len());
                f.write_all(&zeros[..n]).map_err(|_| storage())?;
                left -= n;
            }
            f.sync_all().map_err(|_| storage())?;
            // Do not replace an existing historical file, including a raced creator.
            fs::hard_link(&tmp, path).map_err(|_| storage())?;
            File::open(dir)
                .and_then(|d| d.sync_all())
                .map_err(|_| storage())
        })();
        let _ = fs::remove_file(tmp);
        result
    }
    fn open(path: &Path, seconds: u64, capacity: usize) -> Result<Self, ApiError> {
        let file = open_file(path, true).map_err(|_| storage())?;
        let size = file.metadata().map_err(|_| storage())?.len();
        if size < 64 || size > Self::size(capacity) {
            return Err(storage());
        }
        let mut h = [0u8; 64];
        file.read_exact_at(&mut h, 0).map_err(|_| storage())?;
        if &h[..8] != b"WANRING\0"
            || le64(&h, 8) != seconds
            || le64(&h, 16) != capacity as u64
            || le32(&h, 60) != crc32(&h[..60])
        {
            return Err(storage());
        }
        let mut ring = Self {
            file,
            seconds,
            capacity,
            dirty: BTreeMap::new(),
            recovered: size < Self::size(capacity),
            complete: size == Self::size(capacity),
        };
        // Stream small slabs. Memory stays bounded and queries do not issue
        // tens of thousands of tiny reads on embedded storage.
        ring.recovered |= ring.scan(None, |_, _| Ok(()))?;
        if size < Self::size(capacity) {
            let parent = path.parent().ok_or_else(storage)?;
            // Headroom denial must not hide the valid preexisting prefix.
            // Leave the damaged tail untouched and expose recovered coverage.
            if admit(parent, Self::size(capacity) - size).is_err() {
                return Ok(ring);
            }
            let zeros = [0u8; 32768];
            let mut at = size;
            while at < Self::size(capacity) {
                let n = ((Self::size(capacity) - at) as usize).min(zeros.len());
                ring.file
                    .write_all_at(&zeros[..n], at)
                    .map_err(|_| storage())?;
                at += n as u64;
            }
            ring.file.sync_all().map_err(|_| storage())?;
            ring.complete = true;
        }
        Ok(ring)
    }
    fn disk_bucket(&self, index: usize) -> Result<(Option<Bucket>, bool), ApiError> {
        let mut raw = [0u8; 128];
        let mut read = 0;
        while read < raw.len() {
            match self
                .file
                .read_at(&mut raw[read..], 64 + index as u64 * 128 + read as u64)
            {
                Ok(0) => break,
                Ok(n) => read += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return Err(storage()),
            }
        }
        let mut selected = None;
        let mut damaged = read < raw.len();
        for part in raw.chunks_exact(64) {
            if let Some(b) =
                Bucket::decode(part.try_into().unwrap(), self.seconds, self.capacity, index)
            {
                if selected.is_none_or(|old: Bucket| b.generation > old.generation) {
                    selected = Some(b);
                }
            } else if part.iter().any(|b| *b != 0) {
                damaged = true;
            }
        }
        Ok((selected, damaged))
    }
    fn scan(
        &self,
        budget: Option<&Budget<'_>>,
        mut visit: impl FnMut(usize, Bucket) -> Result<(), ApiError>,
    ) -> Result<bool, ApiError> {
        let mut slab = [0u8; 16384];
        let mut damaged = false;
        let mut first = 0usize;
        while first < self.capacity {
            if let Some(b) = budget {
                check(b)?;
            }
            slab.fill(0);
            let count = (self.capacity - first).min(128);
            let wanted = count * 128;
            let mut read = 0;
            while read < wanted {
                match self.file.read_at(
                    &mut slab[read..wanted],
                    64 + first as u64 * 128 + read as u64,
                ) {
                    Ok(0) => break,
                    Ok(n) => read += n,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => return Err(storage()),
                }
            }
            damaged |= read < wanted;
            for local in 0..count {
                let index = first + local;
                let raw = &slab[local * 128..local * 128 + 128];
                let mut selected = None;
                for part in raw.chunks_exact(64) {
                    if let Some(b) =
                        Bucket::decode(part.try_into().unwrap(), self.seconds, self.capacity, index)
                    {
                        if selected.is_none_or(|old: Bucket| b.generation > old.generation) {
                            selected = Some(b);
                        }
                    } else if part.iter().any(|v| *v != 0) {
                        damaged = true;
                    }
                }
                if let Some(b) = self.dirty.get(&index).copied().or(selected) {
                    visit(index, b)?;
                }
            }
            first += count;
        }
        Ok(damaged)
    }
    fn bucket(&self, index: usize) -> Result<Option<Bucket>, ApiError> {
        if let Some(b) = self.dirty.get(&index) {
            Ok(Some(*b))
        } else {
            self.disk_bucket(index).map(|v| v.0)
        }
    }
    fn add(&mut self, from: u64, to: u64, rx: u64, tx: u64) -> Result<(), ApiError> {
        if !self.complete {
            return Err(error(
                507,
                "storage_insufficient",
                "Traffic history tail repair needs free storage headroom.",
            ));
        }
        let duration = to - from;
        let mut at = from;
        let mut assigned = (0u64, 0u64);
        while at < to {
            let start = at / self.seconds * self.seconds;
            let end = to.min(start + self.seconds);
            let index = ((start / self.seconds) % self.capacity as u64) as usize;
            let mut b = self.bucket(index)?.unwrap_or_default();
            if b.start != start {
                b = Bucket {
                    start,
                    generation: b.generation,
                    ..Bucket::default()
                };
            }
            let nr = (u128::from(rx) * u128::from(end - from) / u128::from(duration)) as u64;
            let nt = (u128::from(tx) * u128::from(end - from) / u128::from(duration)) as u64;
            let coverage = (end - at) * 1_000_000_000;
            if b.coverage + coverage <= self.seconds * 1_000_000_000 {
                b.rx = b.rx.checked_add(nr - assigned.0).ok_or_else(storage)?;
                b.tx = b.tx.checked_add(nt - assigned.1).ok_or_else(storage)?;
                b.coverage += coverage;
                b.end_ms = ((end - start) * 1000) as u32;
                b.rx_peak = b.rx_peak.max(rx as f64 / duration as f64);
                b.tx_peak = b.tx_peak.max(tx as f64 / duration as f64);
                if !self.dirty.contains_key(&index) && self.dirty.len() >= 128 {
                    self.flush()?;
                }
                self.dirty.insert(index, b);
            }
            assigned = (nr, nt);
            at = end;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<bool, ApiError> {
        if self.dirty.is_empty() {
            return Ok(false);
        }
        for (index, b) in &self.dirty {
            let generation = b
                .generation
                .checked_add(1)
                .filter(|v| *v != 0)
                .ok_or_else(storage)?;
            let raw = Bucket { generation, ..*b }.encode();
            self.file
                .write_all_at(
                    &raw,
                    64 + (*index as u64 * 2 + u64::from(generation % 2)) * 64,
                )
                .map_err(|_| storage())?;
        }
        self.file.sync_all().map_err(|_| storage())?;
        self.dirty.clear();
        Ok(true)
    }
}

#[derive(Clone)]
struct WanObservation {
    at: u64,
    source: String,
    rx: u64,
    tx: u64,
}
fn default_route(raw: &[u8], ipv6: bool) -> Result<Option<String>, ApiError> {
    if raw.len() > SOURCE_BYTES {
        return Err(invalid());
    }
    let s = std::str::from_utf8(raw).map_err(|_| invalid())?;
    let mut selected: Option<(u64, String)> = None;
    let mut rows = 0;
    for (i, line) in s.lines().enumerate() {
        let fields = line.split_ascii_whitespace().take(12).collect::<Vec<_>>();
        if !ipv6 && i == 0 {
            if fields.len() != 11
                || fields[0] != "Iface"
                || fields[1] != "Destination"
                || fields[7] != "Mask"
            {
                return Err(invalid());
            }
            continue;
        }
        if fields.is_empty() {
            continue;
        }
        rows += 1;
        if rows > 4096 {
            return Err(invalid());
        }
        let (name, destination, mask, metric, flags) = if ipv6 {
            if fields.len() != 10
                || fields[0].len() != 32
                || fields[2].len() != 32
                || fields[4].len() != 32
                || ![fields[0], fields[2], fields[4]]
                    .iter()
                    .all(|s| s.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                return Err(invalid());
            }
            (
                fields[9],
                fields[0],
                fields[1],
                u64::from_str_radix(fields[5], 16),
                u64::from_str_radix(fields[8], 16),
            )
        } else {
            if fields.len() != 11
                || ![fields[1], fields[2], fields[7]]
                    .iter()
                    .all(|s| s.len() == 8 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                return Err(invalid());
            }
            (
                fields[0],
                fields[1],
                fields[7],
                fields[6].parse::<u64>(),
                u64::from_str_radix(fields[3], 16),
            )
        };
        if !interface(name) {
            return Err(invalid());
        }
        if ipv6 {
            let bits = u8::from_str_radix(fields[1], 16).map_err(|_| invalid())?;
            let source_bits = u8::from_str_radix(fields[3], 16).map_err(|_| invalid())?;
            if bits > 128
                || source_bits > 128
                || fields[6..8]
                    .iter()
                    .any(|s| u64::from_str_radix(s, 16).is_err())
            {
                return Err(invalid());
            }
        } else {
            if [4, 5, 8, 9, 10]
                .iter()
                .any(|i| fields[*i].parse::<u64>().is_err())
            {
                return Err(invalid());
            }
            let word = u32::from_str_radix(fields[7], 16)
                .map_err(|_| invalid())?
                .swap_bytes();
            let inverse = !word;
            if inverse & inverse.wrapping_add(1) != 0 {
                return Err(invalid());
            }
        }
        let metric = metric.map_err(|_| invalid())?;
        let flags = flags.map_err(|_| invalid())?;
        if flags & 1 == 0
            || flags & 0x200 != 0
            || !destination.bytes().all(|b| b == b'0')
            || !mask.bytes().all(|b| b == b'0')
        {
            continue;
        }
        if selected
            .as_ref()
            .is_none_or(|old| metric < old.0 || (metric == old.0 && name < old.1.as_str()))
        {
            selected = Some((metric, name.to_owned()));
        }
    }
    Ok(selected.map(|v| v.1))
}
fn net_counters(raw: &[u8], selected: &str) -> Result<(u64, u64), ApiError> {
    if raw.len() > SOURCE_BYTES {
        return Err(invalid());
    }
    let s = std::str::from_utf8(raw).map_err(|_| invalid())?;
    let mut lines = s.lines();
    if !lines.next().is_some_and(|v| v.contains("Inter-"))
        || !lines.next().is_some_and(|v| v.contains("bytes"))
    {
        return Err(invalid());
    }
    let mut result = None;
    let mut names = BTreeSet::new();
    for (i, line) in lines.enumerate() {
        if i >= 4096 {
            return Err(invalid());
        }
        if line.trim().is_empty() {
            continue;
        }
        let (name, rest) = line.rsplit_once(':').ok_or_else(invalid)?;
        let name = name.trim();
        if !interface(name) || !names.insert(name) {
            return Err(invalid());
        }
        let values = rest
            .split_ascii_whitespace()
            .take(17)
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| invalid())?;
        if values.len() != 16 {
            return Err(invalid());
        }
        if name == selected {
            result = Some((values[0], values[8]));
        }
    }
    result.ok_or_else(invalid)
}
fn sysfs_counter(
    io: &mut impl Backend,
    name: &str,
    kind: &str,
    b: &Budget<'_>,
) -> Result<u64, ApiError> {
    let p = PathBuf::from(format!("/sys/class/net/{name}/statistics/{kind}_bytes"));
    let raw = io.read(&p, 64, b).map_err(|_| invalid())?;
    if raw.len() > 64 {
        return Err(invalid());
    }
    std::str::from_utf8(&raw)
        .map_err(|_| invalid())?
        .trim()
        .parse()
        .map_err(|_| invalid())
}
fn observe_wan(io: &mut impl Backend, b: &Budget<'_>) -> Result<WanObservation, ApiError> {
    check(b)?;
    let ipv4 = io.read(Path::new("/proc/net/route"), SOURCE_BYTES, b).ok();
    let selected = match ipv4 {
        Some(raw) => default_route(&raw, false)?,
        None => None,
    };
    let selected = if let Some(s) = selected {
        s
    } else {
        let raw = io
            .read(Path::new("/proc/net/ipv6_route"), SOURCE_BYTES, b)
            .map_err(|_| invalid())?;
        default_route(&raw, true)?.ok_or_else(invalid)?
    };
    check(b)?;
    let counters = match io.read(Path::new("/proc/net/dev"), SOURCE_BYTES, b) {
        Ok(raw) => net_counters(&raw, &selected)?,
        Err(_) => (
            sysfs_counter(io, &selected, "rx", b)?,
            sysfs_counter(io, &selected, "tx", b)?,
        ),
    };
    let at = io.now_unix();
    if at == 0 || at > 253402300799 {
        return Err(invalid());
    }
    Ok(WanObservation {
        at,
        source: selected,
        rx: counters.0,
        tx: counters.1,
    })
}

// Typed stream decoders bound cardinality before allocation. Unknown private
// fields are skipped, never cloned into a public JSON tree.
fn list_limit<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    d: D,
    limit: usize,
) -> Result<Vec<T>, D::Error> {
    struct V<T>(usize, std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> Visitor<'de> for V<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("bounded list")
        }
        fn visit_seq<S: SeqAccess<'de>>(self, mut s: S) -> Result<Self::Value, S::Error> {
            let mut out = Vec::new();
            while let Some(v) = s.next_element()? {
                if out.len() == self.0 {
                    return Err(de::Error::custom("list limit"));
                }
                out.push(v);
            }
            Ok(out)
        }
    }
    d.deserialize_seq(V(limit, std::marker::PhantomData))
}
fn addresses<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<WireCounter>, D::Error> {
    list_limit(d, 16)
}
fn tags<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    list_limit(d, 8)
}
fn chains<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    list_limit(d, 32)
}
#[derive(Clone, Deserialize)]
struct WireCounter {
    ip: String,
    #[serde(default)]
    hw: String,
    rx_bytes: Option<u64>,
    tx_bytes: Option<u64>,
}
#[derive(Deserialize)]
struct WireDevice {
    hw: String,
    #[serde(default)]
    hostname: String,
    #[serde(default)]
    ifname: String,
    assoc: Option<u8>,
    #[serde(default)]
    online_timer: Option<u64>,
    #[serde(default)]
    ageing_timer: Option<u64>,
    #[serde(default)]
    mld: Option<u8>,
    #[serde(default)]
    signal: String,
    #[serde(default)]
    noise: String,
    #[serde(default)]
    nego_rx_rate: String,
    #[serde(default)]
    nego_tx_rate: String,
    #[serde(default)]
    wifiprotocol: String,
    #[serde(default)]
    wireless_ageing: Option<u64>,
    #[serde(default, deserialize_with = "addresses")]
    ip_list: Vec<WireCounter>,
}
#[derive(Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct Counter {
    address: String,
    rx_bytes: u64,
    tx_bytes: u64,
}
#[derive(Clone, Serialize)]
struct Link {
    interface: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    protocol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mld: Option<bool>,
    #[serde(rename = "signalDBM", skip_serializing_if = "Option::is_none")]
    signal: Option<i16>,
    #[serde(rename = "noiseDBM", skip_serializing_if = "Option::is_none")]
    noise: Option<i16>,
    #[serde(rename = "negotiatedRX", skip_serializing_if = "String::is_empty")]
    rx: String,
    #[serde(rename = "negotiatedTX", skip_serializing_if = "String::is_empty")]
    tx: String,
    #[serde(rename = "ageingSeconds", skip_serializing_if = "Option::is_none")]
    ageing: Option<u64>,
}
#[derive(Clone)]
struct DeviceObservation {
    name: String,
    interface: String,
    associated: bool,
    online: Option<u64>,
    ageing: Option<u64>,
    counters: Vec<Counter>,
    links: Vec<Link>,
    counter_valid: bool,
}
struct DeviceRows {
    rows: BTreeMap<String, DeviceObservation>,
    partial: bool,
    truncated: bool,
}
impl<'de> Deserialize<'de> for DeviceRows {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = DeviceRows;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("MAC-keyed trafficd hw detail")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut m: M) -> Result<Self::Value, M::Error> {
                let mut out = DeviceRows {
                    rows: BTreeMap::new(),
                    partial: false,
                    truncated: false,
                };
                let mut keys = BTreeSet::new();
                let mut rejected = BTreeSet::new();
                while let Some(key) = m.next_key::<String>()? {
                    if keys.len() >= 512 || !keys.insert(key.clone()) {
                        return Err(de::Error::custom("trafficd row limit or duplicate"));
                    }
                    let raw = m.next_value::<Box<serde_json::value::RawValue>>()?;
                    let w = match serde_json::from_str::<WireDevice>(raw.get()) {
                        Ok(w) => w,
                        Err(_) => {
                            out.partial = true;
                            continue;
                        }
                    };
                    let Some(id) = canonical_mac(&w.hw) else {
                        out.partial = true;
                        continue;
                    };
                    if key.to_ascii_uppercase() != id
                        && key.to_ascii_uppercase()
                            != format!("{id}-{}", w.ifname.to_ascii_uppercase())
                        || !text(&w.hostname, 128)
                        || !text(&w.ifname, 32)
                        || !matches!(w.assoc, Some(0 | 1))
                        || !text(&w.wifiprotocol, 64)
                        || !text(&w.nego_rx_rate, 64)
                        || !text(&w.nego_tx_rate, 64)
                    {
                        rejected.insert(id);
                        out.partial = true;
                        continue;
                    }
                    let mut counters = BTreeMap::new();
                    let mut bad = false;
                    for c in w.ip_list {
                        let address = safe_ip(&c.ip);
                        let (Some(rx), Some(tx)) = (c.rx_bytes, c.tx_bytes) else {
                            bad = true;
                            continue;
                        };
                        if address.is_empty()
                            || !c.hw.is_empty() && canonical_mac(&c.hw).as_deref() != Some(&id)
                        {
                            bad = true;
                            continue;
                        }
                        let counter = Counter {
                            address: address.clone(),
                            rx_bytes: rx,
                            tx_bytes: tx,
                        };
                        if counters.get(&address).is_some_and(|old| old != &counter) {
                            bad = true;
                        }
                        counters.insert(address, counter);
                    }
                    let counters = counters.into_values().collect::<Vec<_>>();
                    if totals(&counters).is_none() {
                        bad = true;
                    }
                    if bad {
                        rejected.insert(id.clone());
                        out.partial = true;
                    }
                    let mut links = Vec::new();
                    if !w.wifiprotocol.is_empty()
                        || w.mld.is_some()
                        || !w.signal.is_empty()
                        || !w.noise.is_empty()
                    {
                        let dbm = |s: &str| -> Option<i16> {
                            s.parse::<i16>().ok().filter(|n| (-150..=0).contains(n))
                        };
                        let signal = dbm(&w.signal);
                        let noise = dbm(&w.noise);
                        if !w.signal.is_empty() && signal.is_none()
                            || !w.noise.is_empty() && noise.is_none()
                        {
                            out.partial = true;
                        }
                        links.push(Link {
                            interface: w.ifname.clone(),
                            protocol: w.wifiprotocol,
                            mld: w.mld.filter(|v| *v <= 1).map(|v| v == 1),
                            signal,
                            noise,
                            rx: w.nego_rx_rate,
                            tx: w.nego_tx_rate,
                            ageing: w.wireless_ageing,
                        });
                    }
                    let o = DeviceObservation {
                        name: w.hostname,
                        interface: w.ifname,
                        associated: w.assoc == Some(1),
                        online: w.online_timer,
                        ageing: w.ageing_timer,
                        counters,
                        links,
                        counter_valid: !bad,
                    };
                    if let Some(old) = out.rows.get_mut(&id) {
                        if old.counters != o.counters {
                            rejected.insert(id.clone());
                            out.partial = true;
                        }
                        old.associated |= o.associated;
                        if old.name.is_empty() {
                            old.name = o.name;
                        }
                        for link in o.links {
                            if !old.links.iter().any(|v| v.interface == link.interface) {
                                old.links.push(link);
                            }
                        }
                        if old.interface != o.interface && !o.interface.is_empty() {
                            let mut interfaces = old
                                .interface
                                .split(" + ")
                                .map(str::to_owned)
                                .collect::<BTreeSet<_>>();
                            interfaces.insert(o.interface);
                            old.interface = interfaces.into_iter().collect::<Vec<_>>().join(" + ");
                        }
                    } else if out.rows.len() < 128 {
                        out.rows.insert(id, o);
                    } else {
                        out.truncated = true;
                        // Match the mature admission order: associated first,
                        // then canonical MAC. Keep only 128 rows while reading.
                        let worst = out
                            .rows
                            .iter()
                            .max_by(|(aid, a), (bid, b)| {
                                (!a.associated).cmp(&(!b.associated)).then(aid.cmp(bid))
                            })
                            .map(|(id, row)| (id.clone(), row.associated));
                        if let Some((old, associated)) = worst
                            && (o.associated && !associated
                                || o.associated == associated && id < old)
                        {
                            out.rows.remove(&old);
                            out.rows.insert(id, o);
                        }
                    }
                }
                for (id, o) in &mut out.rows {
                    if rejected.contains(id) {
                        o.counters.clear();
                        o.counter_valid = false;
                    }
                    o.links.sort_by(|a, b| a.interface.cmp(&b.interface));
                }
                Ok(out)
            }
        }
        d.deserialize_map(V)
    }
}
fn totals(c: &[Counter]) -> Option<(u64, u64)> {
    if c.is_empty() {
        return Some((0, 0));
    }
    c.iter().try_fold((0u64, 0u64), |(rx, tx), c| {
        Some((rx.checked_add(c.rx_bytes)?, tx.checked_add(c.tx_bytes)?))
    })
}
#[derive(Clone, Copy, Default)]
struct DeviceBucket {
    start: u64,
    rx: u64,
    tx: u64,
    coverage: u64,
}
struct TrackedDevice {
    observation: DeviceObservation,
    seen: u64,
    previous: Option<(u64, Option<u64>, Vec<Counter>)>,
    rates: Option<(f64, f64)>,
    fine: Vec<DeviceBucket>,
    coarse: Vec<DeviceBucket>,
}
impl TrackedDevice {
    fn new(o: DeviceObservation, at: u64) -> Self {
        Self {
            observation: o,
            seen: at,
            previous: None,
            rates: None,
            fine: vec![DeviceBucket::default(); 289],
            coarse: vec![DeviceBucket::default(); 169],
        }
    }
    fn record(&mut self, o: DeviceObservation, at: u64) {
        let previous = self.previous.take();
        self.rates = None;
        self.seen = at;
        self.observation = o;
        let o = &self.observation;
        if !o.associated || o.counters.is_empty() || o.ageing.is_some_and(|v| v > 60) {
            return;
        }
        self.previous = Some((at, o.online, o.counters.clone()));
        let Some((old_at, old_online, old)) = previous else {
            return;
        };
        if at <= old_at
            || at - old_at > 45
            || o.online.zip(old_online).is_some_and(|(n, p)| n < p)
            || old.len() != o.counters.len()
        {
            return;
        }
        let mut delta = (0u64, 0u64);
        for (p, n) in old.iter().zip(&o.counters) {
            if p.address != n.address || n.rx_bytes < p.rx_bytes || n.tx_bytes < p.tx_bytes {
                return;
            }
            let (Some(rx), Some(tx)) = (
                delta.0.checked_add(n.rx_bytes - p.rx_bytes),
                delta.1.checked_add(n.tx_bytes - p.tx_bytes),
            ) else {
                return;
            };
            delta = (rx, tx);
        }
        let seconds = (at - old_at) as f64;
        if delta.0 as f64 / seconds > (1u64 << 40) as f64
            || delta.1 as f64 / seconds > (1u64 << 40) as f64
        {
            return;
        }
        self.rates = Some((delta.0 as f64 / seconds, delta.1 as f64 / seconds));
        add_device(&mut self.fine, 300, old_at, at, delta);
        add_device(&mut self.coarse, 3600, old_at, at, delta);
    }
}
fn add_device(ring: &mut [DeviceBucket], seconds: u64, from: u64, to: u64, delta: (u64, u64)) {
    let mut at = from;
    let mut assigned = (0u64, 0u64);
    while at < to {
        let start = at / seconds * seconds;
        let end = to.min(start + seconds);
        let idx = ((start / seconds) % ring.len() as u64) as usize;
        let b = &mut ring[idx];
        if b.start != start {
            *b = DeviceBucket {
                start,
                ..DeviceBucket::default()
            };
        }
        let rx = (u128::from(delta.0) * u128::from(end - from) / u128::from(to - from)) as u64;
        let tx = (u128::from(delta.1) * u128::from(end - from) / u128::from(to - from)) as u64;
        if b.coverage + end - at <= seconds
            && let (Some(nr), Some(nt)) = (
                b.rx.checked_add(rx - assigned.0),
                b.tx.checked_add(tx - assigned.1),
            )
        {
            b.rx = nr;
            b.tx = nt;
            b.coverage += end - at;
        }
        assigned = (rx, tx);
        at = end;
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Annotation {
    label: String,
    note: String,
    #[serde(deserialize_with = "tags")]
    tags: Vec<String>,
}
impl Annotation {
    fn valid(&self) -> bool {
        self.label.chars().count() <= 80
            && self.note.chars().count() <= 1000
            && self.tags.len() <= 8
            && self.tags.iter().all(|s| s.chars().count() <= 32)
    }
    fn empty(&self) -> bool {
        self.label.is_empty() && self.note.is_empty() && self.tags.is_empty()
    }
}
fn annotation_map<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<String, Annotation>, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = BTreeMap<String, Annotation>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("bounded canonical MAC annotations")
        }
        fn visit_map<M: MapAccess<'de>>(self, mut m: M) -> Result<Self::Value, M::Error> {
            let mut out = BTreeMap::new();
            while let Some(key) = m.next_key::<String>()? {
                if out.len() >= 256
                    || out.contains_key(&key)
                    || canonical_mac(&key).as_ref() != Some(&key)
                {
                    return Err(de::Error::custom("annotation keys"));
                }
                let a = m.next_value::<Annotation>()?;
                if !a.valid() {
                    return Err(de::Error::custom("annotation limit"));
                }
                out.insert(key, a);
            }
            Ok(out)
        }
    }
    d.deserialize_map(V)
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Annotations {
    revision: u64,
    #[serde(deserialize_with = "annotation_map")]
    devices: BTreeMap<String, Annotation>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AnnotationUpdate {
    mac: String,
    label: String,
    note: String,
    #[serde(deserialize_with = "tags")]
    tags: Vec<String>,
    expected_revision: u64,
}
struct UniqueObject;
impl<'de> Deserialize<'de> for UniqueObject {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = UniqueObject;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("one unique-key JSON object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut m: M) -> Result<Self::Value, M::Error> {
                let mut keys = BTreeSet::new();
                while let Some(key) = m.next_key::<String>()? {
                    if keys.len() >= 8 || !keys.insert(key) {
                        return Err(de::Error::custom("duplicate or excess fields"));
                    }
                    m.next_value::<de::IgnoredAny>()?;
                }
                Ok(UniqueObject)
            }
        }
        d.deserialize_map(V)
    }
}
fn load_annotations(dir: &Path) -> Result<Annotations, ApiError> {
    let path = dir.join("device-names.json");
    if regular(&path, ANNOTATION_BYTES)?.is_none() {
        return Ok(Annotations {
            revision: 0,
            devices: BTreeMap::new(),
        });
    }
    let f = open_file(&path, false).map_err(|_| storage())?;
    let mut d = serde_json::Deserializer::from_reader(BufReader::new(f.take(ANNOTATION_BYTES + 1)));
    let state = Annotations::deserialize(&mut d).map_err(|_| storage())?;
    d.end().map_err(|_| storage())?;
    if state.revision > MAX_REVISION {
        return Err(storage());
    }
    Ok(state)
}

#[derive(Deserialize)]
struct Accepted<'a> {
    #[serde(borrow)]
    experimental: Option<Experimental<'a>>,
    #[serde(default)]
    outbounds: IgnoredOutbounds,
}
#[derive(Deserialize)]
struct Experimental<'a> {
    #[serde(borrow)]
    clash_api: Option<ClashApi<'a>>,
}
// Neither accepted config nor the credential type implements Debug/Serialize.
#[derive(Deserialize)]
struct ClashApi<'a> {
    #[serde(borrow)]
    external_controller: std::borrow::Cow<'a, str>,
    #[serde(default, borrow)]
    secret: std::borrow::Cow<'a, str>,
}
#[derive(Default)]
struct IgnoredOutbounds {
    can_probe: bool,
}
impl<'de> Deserialize<'de> for IgnoredOutbounds {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Outbound {
            #[serde(default)]
            tag: String,
            #[serde(rename = "type", default)]
            kind: String,
        }
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = IgnoredOutbounds;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("accepted outbound identities")
            }
            fn visit_seq<S: SeqAccess<'de>>(self, mut s: S) -> Result<Self::Value, S::Error> {
                let mut out = IgnoredOutbounds::default();
                let mut count = 0;
                while let Some(v) = s.next_element::<Outbound>()? {
                    count += 1;
                    if count > 4096 {
                        return Err(de::Error::custom("outbound limit"));
                    }
                    out.can_probe |= v.tag == "proxy" && v.kind != "direct" && v.kind != "block";
                }
                Ok(out)
            }
        }
        d.deserialize_seq(V)
    }
}
fn accepted(raw: &[u8]) -> Result<(ClashApi<'_>, bool, [u8; 32]), ApiError> {
    if raw.is_empty() || raw.len() > 4 << 20 {
        return Err(invalid());
    }
    let c: Accepted<'_> = serde_json::from_slice(raw).map_err(|_| invalid())?;
    let api = c
        .experimental
        .and_then(|e| e.clash_api)
        .ok_or_else(invalid)?;
    let address = api
        .external_controller
        .parse::<SocketAddr>()
        .map_err(|_| invalid())?;
    if !address.ip().is_loopback()
        || address.port() == 0
        || api.secret.len() > 256
        || !api
            .secret
            .bytes()
            .all(|b| b >= 32 && b != 127 && b != b'\r' && b != b'\n')
    {
        return Err(invalid());
    }
    Ok((api, c.outbounds.can_probe, Sha256::digest(raw).into()))
}
fn core_get(api: &ClashApi<'_>, path: &str, b: &Budget<'_>) -> Result<Vec<u8>, ApiError> {
    check(b)?;
    let addr = api
        .external_controller
        .parse::<SocketAddr>()
        .map_err(|_| invalid())?;
    if !addr.ip().is_loopback() || addr.port() == 0 {
        return Err(invalid());
    }
    let left = b
        .deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_millis(150));
    if left.is_zero() {
        return Err(invalid());
    }
    let mut socket = TcpStream::connect_timeout(&addr, left).map_err(|_| {
        error(
            502,
            "telemetry_unavailable",
            "Local core telemetry could not be read.",
        )
    })?;
    let mut request = format!(
        "GET {path} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nAccept-Encoding: identity\r\nConnection: close\r\n",
        api.external_controller
    );
    if !api.secret.is_empty() {
        request.push_str("Authorization: Bearer ");
        request.push_str(&api.secret);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    let mut at = 0;
    while at < request.len() {
        check(b)?;
        let timeout = b
            .deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(100));
        socket
            .set_write_timeout(Some(timeout.max(Duration::from_millis(1))))
            .map_err(|_| invalid())?;
        match socket.write(&request.as_bytes()[at..]) {
            Ok(0) => return Err(invalid()),
            Ok(n) => at += n,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::Interrupted
                        | io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(_) => return Err(invalid()),
        }
    }
    drop(request);
    let mut bytes = Vec::with_capacity(8192);
    let mut chunk = [0u8; 8192];
    let mut header_end = None;
    let mut length = None;
    let mut chunked = false;
    loop {
        check(b)?;
        socket
            .set_read_timeout(Some(
                b.deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(100))
                    .max(Duration::from_millis(1)),
            ))
            .map_err(|_| invalid())?;
        match socket.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                if bytes.len() + n > CORE_BYTES + 16384 {
                    return Err(invalid());
                }
                bytes.extend_from_slice(&chunk[..n]);
                if header_end.is_none() {
                    let mut headers = [httparse::EMPTY_HEADER; 32];
                    let mut response = httparse::Response::new(&mut headers);
                    match response.parse(&bytes).map_err(|_| invalid())? {
                        httparse::Status::Partial => {
                            if bytes.len() > 16384 {
                                return Err(invalid());
                            }
                        }
                        httparse::Status::Complete(end) => {
                            if response.code != Some(200) || end > 16384 {
                                return Err(invalid());
                            }
                            for h in response.headers.iter() {
                                if h.name.eq_ignore_ascii_case("content-length") {
                                    if length.is_some() {
                                        return Err(invalid());
                                    }
                                    length = Some(
                                        std::str::from_utf8(h.value)
                                            .map_err(|_| invalid())?
                                            .trim()
                                            .parse::<usize>()
                                            .map_err(|_| invalid())?,
                                    );
                                    if length.is_some_and(|n| n > CORE_BYTES) {
                                        return Err(invalid());
                                    }
                                } else if h.name.eq_ignore_ascii_case("transfer-encoding") {
                                    if !h.value.eq_ignore_ascii_case(b"chunked") || chunked {
                                        return Err(invalid());
                                    }
                                    chunked = true;
                                } else if h.name.eq_ignore_ascii_case("content-encoding")
                                    && !h.value.eq_ignore_ascii_case(b"identity")
                                {
                                    return Err(invalid());
                                }
                            }
                            if chunked && length.is_some() {
                                return Err(invalid());
                            }
                            header_end = Some(end);
                        }
                    }
                }
                if let Some(end) = header_end {
                    if let Some(n) = length
                        && bytes.len() >= end + n
                    {
                        break;
                    }
                    if chunked && chunk_complete(&bytes[end..])? {
                        break;
                    }
                    if !chunked && bytes.len() - end > CORE_BYTES {
                        return Err(invalid());
                    }
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::Interrupted
                        | io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                ) =>
            {
                continue;
            }
            Err(_) => return Err(invalid()),
        }
    }
    let end = header_end.ok_or_else(invalid)?;
    let body = &bytes[end..];
    if chunked {
        decode_chunks(body)
    } else {
        if body.len() > CORE_BYTES || length.is_some_and(|n| n != body.len()) {
            return Err(invalid());
        }
        Ok(body.to_vec())
    }
}
fn chunk_complete(raw: &[u8]) -> Result<bool, ApiError> {
    let mut at = 0;
    let mut total = 0usize;
    loop {
        let Some(line) = raw[at..].windows(2).position(|s| s == b"\r\n") else {
            return Ok(false);
        };
        if line > 128 {
            return Err(invalid());
        }
        let size_text = std::str::from_utf8(&raw[at..at + line])
            .map_err(|_| invalid())?
            .split(';')
            .next()
            .ok_or_else(invalid)?;
        let n = usize::from_str_radix(size_text, 16).map_err(|_| invalid())?;
        at += line + 2;
        total = total.checked_add(n).ok_or_else(invalid)?;
        if total > CORE_BYTES {
            return Err(invalid());
        }
        if n == 0 {
            return Ok(raw.len() >= at + 2 && &raw[at..at + 2] == b"\r\n");
        }
        let Some(end) = at.checked_add(n).and_then(|v| v.checked_add(2)) else {
            return Err(invalid());
        };
        if raw.len() < end {
            return Ok(false);
        }
        if &raw[end - 2..end] != b"\r\n" {
            return Err(invalid());
        }
        at = end;
    }
}
fn decode_chunks(raw: &[u8]) -> Result<Vec<u8>, ApiError> {
    if !chunk_complete(raw)? {
        return Err(invalid());
    }
    let mut out = Vec::new();
    let mut at = 0;
    loop {
        let line = raw[at..]
            .windows(2)
            .position(|s| s == b"\r\n")
            .ok_or_else(invalid)?;
        let n = usize::from_str_radix(
            std::str::from_utf8(&raw[at..at + line])
                .map_err(|_| invalid())?
                .split(';')
                .next()
                .ok_or_else(invalid)?,
            16,
        )
        .map_err(|_| invalid())?;
        at += line + 2;
        if n == 0 {
            return Ok(out);
        }
        out.extend_from_slice(&raw[at..at + n]);
        at += n + 2;
    }
}
#[derive(Default, Deserialize)]
struct CoreMetadata {
    #[serde(default)]
    network: String,
    #[serde(default, rename = "sourceIP")]
    source_ip: String,
    #[serde(default, rename = "sourcePort")]
    source_port: String,
    #[serde(default, rename = "destinationIP")]
    destination_ip: String,
    #[serde(default, rename = "destinationPort")]
    destination_port: String,
    #[serde(default)]
    host: String,
}
#[derive(Deserialize)]
struct CoreConnection {
    id: String,
    start: String,
    upload: u64,
    download: u64,
    #[serde(default, deserialize_with = "chains")]
    chains: Vec<String>,
    #[serde(default)]
    rule: String,
    #[serde(default)]
    metadata: CoreMetadata,
}
struct CoreConnections {
    count: usize,
    rows: Vec<CoreConnection>,
}
impl<'de> Deserialize<'de> for CoreConnections {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = CoreConnections;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("finite core connections")
            }
            fn visit_seq<S: SeqAccess<'de>>(self, mut s: S) -> Result<Self::Value, S::Error> {
                let mut out = CoreConnections {
                    count: 0,
                    rows: Vec::new(),
                };
                while let Some(c) = s.next_element::<CoreConnection>()? {
                    out.count += 1;
                    if out.count > 4096 {
                        return Err(de::Error::custom("connection limit"));
                    }
                    // RFC3339 instants are sorted by parsed time below. Maintain
                    // just the newest 128 typed connections while streaming.
                    if out.rows.len() < 128 {
                        out.rows.push(c);
                    } else {
                        let oldest = out
                            .rows
                            .iter()
                            .enumerate()
                            .min_by_key(|(_, v)| parse_time(&v.start).unwrap_or(0))
                            .map(|(i, _)| i)
                            .unwrap();
                        if parse_time(&c.start) > parse_time(&out.rows[oldest].start) {
                            out.rows[oldest] = c;
                        }
                    }
                }
                Ok(out)
            }
        }
        d.deserialize_seq(V)
    }
}
#[derive(Deserialize)]
struct CoreResponse {
    #[serde(rename = "uploadTotal")]
    upload: u64,
    #[serde(rename = "downloadTotal")]
    download: u64,
    connections: CoreConnections,
}
#[derive(Default)]
struct CoreSelections {
    proxy: Option<String>,
}
impl<'de> Deserialize<'de> for CoreSelections {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Selection {
            #[serde(default)]
            now: Option<String>,
        }
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = CoreSelections;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("bounded live proxy selection map")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut m: M) -> Result<Self::Value, M::Error> {
                let mut out = CoreSelections::default();
                let mut keys = BTreeSet::new();
                while let Some(key) = m.next_key::<String>()? {
                    if keys.len() >= 4096 || !keys.insert(key.clone()) {
                        return Err(de::Error::custom("proxy identity limit"));
                    }
                    if key == "proxy" {
                        out.proxy = m.next_value::<Selection>()?.now;
                    } else {
                        m.next_value::<de::IgnoredAny>()?;
                    }
                }
                Ok(out)
            }
        }
        d.deserialize_map(V)
    }
}
#[derive(Deserialize)]
struct CoreProxies {
    proxies: CoreSelections,
}
// Gregorian RFC3339 parser. No wall time, locale or external date dependency.
fn parse_time(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let year = num(0..4)?;
    let month = num(5..7)?;
    let day = num(8..10)?;
    let hour = num(11..13)?;
    let minute = num(14..16)?;
    let second = num(17..19)?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1970..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || day < 1
        || day > days[(month - 1) as usize]
        || !(0..24).contains(&hour)
        || !(0..60).contains(&minute)
        || !(0..60).contains(&second)
    {
        return None;
    }
    let mut zone = 19;
    if b.get(zone) == Some(&b'.') {
        zone += 1;
        let start = zone;
        while b.get(zone).is_some_and(u8::is_ascii_digit) {
            zone += 1;
        }
        if zone == start {
            return None;
        }
    }
    let offset = if s.get(zone..) == Some("Z") {
        0
    } else {
        let z = s.get(zone..)?;
        let bz = z.as_bytes();
        if bz.len() != 6 || !matches!(bz[0], b'+' | b'-') || bz[3] != b':' {
            return None;
        }
        if ![bz[1], bz[2], bz[4], bz[5]].iter().all(u8::is_ascii_digit) {
            return None;
        }
        let h = z.get(1..3)?.parse::<i64>().ok()?;
        let m = z.get(4..6)?.parse::<i64>().ok()?;
        if h > 23 || m > 59 {
            return None;
        }
        (h * 3600 + m * 60) * if bz[0] == b'+' { 1 } else { -1 }
    };
    let y = year - i64::from(month <= 2);
    let era = y / 400;
    let yoe = y - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let seconds =
        (era * 146097 + doe - 719468) * 86400 + hour * 3600 + minute * 60 + second - offset;
    u64::try_from(seconds).ok().filter(|n| *n <= 253402300799)
}
fn safe_rule(raw: &str) -> String {
    if raw.is_empty() {
        return "核心未提供匹配规则".into();
    }
    if raw == "final" {
        return raw.into();
    }
    if raw.len() > 4096 {
        return "实际匹配规则（描述超过上限）".into();
    }
    let allowed = [
        "domain",
        "domain_suffix",
        "domain_keyword",
        "domain_regex",
        "ip_cidr",
        "source_ip_cidr",
        "ip_version",
        "inbound",
        "network",
        "port",
        "source_port",
        "rule_set",
    ];
    let mut found = false;
    for (at, _) in raw.match_indices('=') {
        let prefix = &raw[..at];
        let key = prefix
            .rsplit(|c: char| !(c.is_ascii_lowercase() || c == '_'))
            .next()
            .unwrap_or("");
        if key.is_empty() {
            continue;
        }
        found = true;
        if !allowed.contains(&key) {
            return "实际匹配规则（非公开条件）".into();
        }
    }
    if !found {
        "实际匹配规则（未提供公开条件）".into()
    } else {
        bounded_text(raw, 160)
    }
}
fn id(key: &[u8; 32], raw: &str) -> String {
    // Standard HMAC-SHA256, without another dependency. Truncate only the public
    // opaque identifier; the random process key remains private.
    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..32 {
        ipad[i] ^= key[i];
        opad[i] ^= key[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(raw.as_bytes());
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner.finalize());
    outer.finalize()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn capability(available: bool, reason: &str) -> Value {
    json!({"available":available,"reason":bounded_text(reason,256)})
}
fn unavailable_core() -> Value {
    let reason = "当前核心未运行或未配置 localhost Clash API";
    json!({"state":"unavailable","reason":reason,"source":"sing-box Clash API · localhost",
        "capabilities":{"connections":capability(false,reason),"traffic":capability(false,reason),"routing":capability(false,reason),
            "latency":capability(false,reason),"requestPhases":capability(false,"HTTPS 内容不透明；核心不提供 DNS/TCP/TLS/TTFB 请求阶段")},
        "totals":{"uploadBytes":0,"downloadBytes":0},"activeConnections":0,"truncated":false,"connections":[],"traffic":[],"probes":[]})
}

/// One instance owns telemetry baselines and compatible persistent history.
/// Private accepted config and credentials are borrowed only during a tick.
pub struct Telemetry {
    dir: PathBuf,
    rings: Vec<Ring>,
    history_error: &'static str,
    observation_error: &'static str,
    annotations: Option<Annotations>,
    annotation_error: Option<ApiError>,
    wan: Option<WanObservation>,
    wan_rate: Option<(f64, f64)>,
    wan_source: String,
    latest_wan: u64,
    last_flush: u64,
    devices: BTreeMap<String, TrackedDevice>,
    device_at: u64,
    device_error: &'static str,
    device_unavailable: bool,
    device_truncated: bool,
    core: Value,
    core_previous: Option<(u64, u64, u64, [u8; 32])>,
    core_traffic: VecDeque<Value>,
    probes: VecDeque<Value>,
    key: [u8; 32],
    core_epoch: Option<[u8; 32]>,
    owner_epoch: Option<(u64, u32)>,
    last_probe: u64,
    last_tick: Option<u64>,
    last_device_tick: Option<u64>,
}
impl Telemetry {
    pub fn open(data_dir: &Path) -> Result<Self, ApiError> {
        if data_dir.as_os_str().is_empty() {
            return Err(storage());
        }
        fs::create_dir_all(data_dir).map_err(|_| storage())?;
        let m = fs::symlink_metadata(data_dir).map_err(|_| storage())?;
        if !m.is_dir() || m.file_type().is_symlink() {
            return Err(storage());
        }
        let dir = data_dir.to_path_buf();
        let traffic = dir.join("traffic");
        let mut rings = Vec::new();
        let mut history_error = "";
        let histories: Result<(), ApiError> = (|| {
            fs::create_dir_all(&traffic).map_err(|_| storage())?;
            let m = fs::symlink_metadata(&traffic).map_err(|_| storage())?;
            if !m.is_dir() || m.file_type().is_symlink() {
                return Err(storage());
            }
            let mut growth = 0u64;
            let mut missing = Vec::new();
            for (seconds, capacity) in LAYOUTS {
                let p = traffic.join(format!("wan-{seconds}s.ring"));
                match regular(&p, Ring::size(capacity)) {
                    Ok(None) => {
                        growth += Ring::size(capacity) + 4096;
                        missing.push((p, seconds, capacity));
                    }
                    Ok(Some(size)) if size >= 64 => {
                        growth += Ring::size(capacity) - size;
                    }
                    _ => {
                        history_error = "Some persistent traffic tiers are unavailable; valid records were retained."
                    }
                }
            }
            // Existing tiers stay readable even if admission for a new tier is
            // denied. Never discard valid history because another tier failed.
            for (seconds, capacity) in LAYOUTS {
                let path = traffic.join(format!("wan-{seconds}s.ring"));
                if path.exists() {
                    match Ring::open(&path, seconds, capacity) {
                        Ok(r) => rings.push(r),
                        Err(_) => {
                            history_error = "Some persistent traffic tiers are unavailable; valid records were retained."
                        }
                    }
                }
            }
            if growth > 0 {
                admit(&traffic, growth)?;
            }
            for (p, s, c) in missing {
                Ring::create(&traffic, &p, s, c)?;
                match Ring::open(&p, s, c) {
                    Ok(r) => rings.push(r),
                    Err(_) => {
                        history_error = "Some persistent traffic tiers are unavailable; valid records were retained."
                    }
                }
            }
            Ok(())
        })();
        if histories.is_err() {
            history_error =
                "Persistent traffic history storage is unavailable; existing files were retained.";
        }
        let (annotations, annotation_error) = match load_annotations(&dir) {
            Ok(a) => (Some(a), None),
            Err(e) => (None, Some(e)),
        };
        let mut key = [0u8; 32];
        getrandom::fill(&mut key).map_err(|_| {
            error(
                503,
                "telemetry_unavailable",
                "Telemetry identity entropy is unavailable.",
            )
        })?;
        let mut latest = 0;
        for r in &rings {
            r.scan(None, |_, b| {
                latest = latest.max(b.start + u64::from(b.end_ms).div_ceil(1000));
                Ok(())
            })?;
        }
        Ok(Self {
            dir,
            rings,
            history_error,
            observation_error: "",
            annotations,
            annotation_error,
            wan: None,
            wan_rate: None,
            wan_source: String::new(),
            latest_wan: latest,
            last_flush: 0,
            devices: BTreeMap::new(),
            device_at: 0,
            device_error: "",
            device_unavailable: false,
            device_truncated: false,
            core: unavailable_core(),
            core_previous: None,
            core_traffic: VecDeque::new(),
            probes: VecDeque::new(),
            key,
            core_epoch: None,
            owner_epoch: None,
            last_probe: 0,
            last_tick: None,
            last_device_tick: None,
        })
    }
    /// Idempotent within each second. Failures are source-qualified DTO states;
    /// one missing source does not discard the other sources' valid samples.
    pub fn set_owner_epoch(&mut self, epoch: Option<(u64, u32)>) {
        if self.owner_epoch != epoch {
            self.owner_epoch = epoch;
            self.core_previous = None;
            self.core_epoch = None;
            self.probes.clear();
        }
    }
    pub fn close(&mut self) -> Result<(), ApiError> {
        let mut failed = None;
        for ring in &mut self.rings {
            if let Err(error) = ring.flush() {
                failed = Some(error);
            }
        }
        failed.map_or(Ok(()), Err)
    }
    pub fn tick(
        &mut self,
        native: Option<&[u8]>,
        io: &mut impl Backend,
        budget: &Budget<'_>,
    ) -> Result<(), ApiError> {
        check(budget)?;
        let now = io.now_unix();
        if now == 0 || now > 253402300799 {
            return Err(error(
                503,
                "observation_unavailable",
                "Native observation time is invalid.",
            ));
        }
        if self.last_tick == Some(now) {
            return Ok(());
        }
        self.last_tick = Some(now);
        if self
            .last_device_tick
            .is_none_or(|last| now < last || now - last >= 15)
        {
            self.last_device_tick = Some(now);
            let b = Budget {
                deadline: budget
                    .deadline
                    .min(Instant::now() + Duration::from_millis(250)),
                cancel: budget.cancel,
            };
            let args = [
                "call",
                "trafficd",
                "hw",
                r#"{"detail":true,"wlan":true,"mlo":true}"#,
            ]
            .map(str::to_owned);
            let rows = io
                .run(Program::Ubus, &args, None, SOURCE_BYTES, &b)
                .ok()
                .filter(|o| o.code == 0 && o.stdout.len() <= SOURCE_BYTES)
                .and_then(|o| serde_json::from_slice::<DeviceRows>(&o.stdout).ok());
            match rows {
                Some(rows) => self.record_devices(rows, now),
                None => {
                    self.device_error = "System trafficd device counters could not be read.";
                    self.device_unavailable = true;
                    for d in self.devices.values_mut() {
                        d.previous = None;
                        d.rates = None;
                    }
                }
            }
        }
        let b = Budget {
            deadline: budget
                .deadline
                .min(Instant::now() + Duration::from_millis(200)),
            cancel: budget.cancel,
        };
        match observe_wan(io, &b) {
            Ok(next) => self.record_wan(next),
            Err(_) => {
                self.wan = None;
                self.wan_rate = None;
                self.observation_error = "Default-route WAN counters are unavailable or invalid.";
            }
        }
        let b = Budget {
            deadline: budget
                .deadline
                .min(Instant::now() + Duration::from_millis(350)),
            cancel: budget.cancel,
        };
        match native.and_then(|raw| accepted(raw).ok()) {
            Some((api, can_probe, epoch)) => {
                match core_get(&api, "/connections", &b).and_then(|raw| {
                    serde_json::from_slice::<CoreResponse>(&raw).map_err(|_| invalid())
                }) {
                    Ok(raw) => {
                        self.record_core(raw, now, epoch, can_probe, &api.secret);
                        // Connections expose actual chains; selected outbound
                        // identity additionally comes from the finite local API,
                        // never guessed from accepted configuration tags.
                        let selection_budget = Budget {
                            deadline: budget
                                .deadline
                                .min(Instant::now() + Duration::from_millis(150)),
                            cancel: budget.cancel,
                        };
                        match core_get(&api, "/proxies", &selection_budget).and_then(|bytes| {
                            serde_json::from_slice::<CoreProxies>(&bytes).map_err(|_| invalid())
                        }) {
                            Ok(proxies) => {
                                let selected = proxies.proxies.proxy.filter(|v| {
                                    !v.is_empty()
                                        && text(v, 128)
                                        && (api.secret.is_empty()
                                            || !v.contains(api.secret.as_ref()))
                                });
                                if let Some(node) = selected {
                                    self.core["selectedOutbounds"] = json!([{"outbound":"proxy","nodeId":id(&self.key,&node),"nodeName":node}]);
                                    self.core["selectionState"] = json!("ready");
                                } else {
                                    self.core["selectionState"] =
                                        json!(if self.core.get("selectedOutbounds").is_some() {
                                            "stale"
                                        } else {
                                            "unavailable"
                                        });
                                }
                            }
                            Err(_) => {
                                self.core["selectionState"] =
                                    json!(if self.core.get("selectedOutbounds").is_some() {
                                        "stale"
                                    } else {
                                        "unavailable"
                                    });
                            }
                        }
                    }
                    Err(_) => self.core_failure("本机核心遥测读取失败；保留最后一次有效观测"),
                }
            }
            None => self.core_failure("当前核心未运行或未配置 localhost Clash API"),
        }
        if now >= self.last_flush.saturating_add(60) || now < self.last_flush {
            let mut changed = false;
            let mut failed = false;
            for r in &mut self.rings {
                if check(budget).is_err() {
                    failed = true;
                    break;
                }
                match r.flush() {
                    Ok(dirty) => changed |= dirty,
                    Err(_) => {
                        failed = true;
                        break;
                    }
                }
            }
            if failed {
                self.history_error = "Traffic history could not be synced to persistent storage.";
            } else if self.rings.len() == 3 && self.rings.iter().all(|r| r.complete) {
                self.history_error = "";
                if changed {
                    self.last_flush = now;
                }
            } else if changed {
                self.last_flush = now;
            }
        }
        check(budget)
    }
    fn record_wan(&mut self, next: WanObservation) {
        self.wan_source = next.source.clone();
        self.observation_error = "";
        self.wan_rate = None;
        let previous = self.wan.replace(next.clone());
        let Some(previous) = previous else {
            return;
        };
        if next.at == previous.at {
            self.wan = Some(previous);
            return;
        }
        if next.at < previous.at
            || next.at - previous.at > 10
            || next.source != previous.source
            || next.rx < previous.rx
            || next.tx < previous.tx
            || previous.at < self.latest_wan
        {
            return;
        }
        let rx = next.rx - previous.rx;
        let tx = next.tx - previous.tx;
        let seconds = (next.at - previous.at) as f64;
        if rx as f64 / seconds > (1u64 << 40) as f64 || tx as f64 / seconds > (1u64 << 40) as f64 {
            return;
        }
        self.wan_rate = Some((rx as f64 / seconds, tx as f64 / seconds));
        for r in &mut self.rings {
            if r.add(previous.at, next.at, rx, tx).is_err() {
                self.history_error = "Traffic history could not be updated in persistent storage.";
            }
        }
        self.latest_wan = next.at;
    }
    fn record_devices(&mut self, rows: DeviceRows, now: u64) {
        if now < self.device_at {
            for d in self.devices.values_mut() {
                d.previous = None;
                d.rates = None;
            }
            return;
        }
        if now == self.device_at {
            return;
        }
        self.device_at = now;
        self.device_error = if rows.partial {
            "Some trafficd rows are invalid; valid device observations are retained."
        } else {
            ""
        };
        self.device_unavailable = rows.partial && rows.rows.is_empty();
        self.device_truncated = rows.truncated;
        let incoming = rows.rows.keys().cloned().collect::<BTreeSet<_>>();
        for (id, o) in rows.rows {
            if !o.counter_valid {
                if let Some(d) = self.devices.get_mut(&id) {
                    d.previous = None;
                    d.rates = None;
                }
                continue;
            }
            if !self.devices.contains_key(&id) && self.devices.len() >= 128 {
                let oldest = self
                    .devices
                    .iter()
                    .filter(|(id, _)| !incoming.contains(*id))
                    .min_by_key(|(id, d)| (d.seen, (*id).clone()))
                    .map(|(id, _)| id.clone());
                if let Some(id) = oldest {
                    self.devices.remove(&id);
                } else {
                    self.device_truncated = true;
                    continue;
                }
                self.device_truncated = true;
            }
            if let Some(d) = self.devices.get_mut(&id) {
                d.record(o, now);
            } else {
                let mut d = TrackedDevice::new(o.clone(), now);
                d.record(o, now);
                self.devices.insert(id, d);
            }
        }
        self.devices.retain(|id, d| {
            if !incoming.contains(id) {
                d.previous = None;
                d.rates = None;
            }
            now.saturating_sub(d.seen) <= 604800
        });
    }
    fn core_failure(&mut self, reason: &str) {
        self.core_previous = None;
        self.core["state"] = json!(if self.core.get("sampledAt").is_some() {
            "stale"
        } else {
            "unavailable"
        });
        self.core["reason"] = json!(bounded_text(reason, 256));
        if self.core.get("selectionState").is_some() {
            self.core["selectionState"] = json!("stale");
        }
        for name in ["connections", "traffic", "routing", "latency"] {
            self.core["capabilities"][name] = capability(false, reason);
        }
    }
    fn record_core(
        &mut self,
        mut raw: CoreResponse,
        now: u64,
        epoch: [u8; 32],
        can_probe: bool,
        secret: &str,
    ) {
        if self.core_epoch.is_some_and(|e| e != epoch) {
            self.probes.clear();
        }
        let reset = self.core_previous.is_none_or(|(at, u, d, e)| {
            e != epoch || now <= at || now - at > 6 || raw.upload < u || raw.download < d
        });
        // Reset is a discontinuity, not a measured zero. The existing wire
        // marker makes its rate fields placeholders and charts leave a gap.
        let (ur, dr) = if reset {
            (0.0, 0.0)
        } else {
            let (at, u, d, _) = self.core_previous.unwrap();
            (
                (raw.upload - u) as f64 / (now - at) as f64,
                (raw.download - d) as f64 / (now - at) as f64,
            )
        };
        if ur > (1u64 << 40) as f64 || dr > (1u64 << 40) as f64 {
            self.core_failure("核心计数器变化无效；保留最后一次有效观测");
            return;
        }
        raw.connections
            .rows
            .sort_by_key(|c| std::cmp::Reverse(parse_time(&c.start).unwrap_or(0)));
        let mut connections = Vec::new();
        let mut routing = false;
        for c in raw.connections.rows {
            let Some(start) = parse_time(&c.start).filter(|v| *v <= now) else {
                continue;
            };
            if c.id.is_empty() {
                continue;
            }
            let outbound = if c.chains.iter().any(|s| s == "proxy") {
                "proxy"
            } else if c.chains.iter().any(|s| s == "direct") {
                "direct"
            } else {
                "unavailable"
            };
            routing |= !c.rule.is_empty() && outbound != "unavailable";
            let host = if c.metadata.host.len() <= 253
                && c.metadata
                    .host
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".-_:".contains(&b))
                && (secret.is_empty() || !c.metadata.host.contains(secret))
            {
                c.metadata.host
            } else {
                String::new()
            };
            let rule = if !secret.is_empty() && c.rule.contains(secret) {
                "实际匹配规则（非公开条件）".to_owned()
            } else {
                safe_rule(&c.rule)
            };
            let selected = c
                .chains
                .iter()
                .find(|s| *s != "proxy" && *s != "direct")
                .filter(|s| text(s, 128))
                .cloned();
            let mut dto = json!({"id":id(&self.key,&c.id),"startedAt":timestamp(start),"ageMs":(now-start)*1000,
                "network":if c.metadata.network=="tcp" || c.metadata.network=="udp" {c.metadata.network.as_str()} else {"unknown"},
                "sourceIP":safe_ip(&c.metadata.source_ip),"sourcePort":c.metadata.source_port.parse::<u16>().unwrap_or(0),
                "destinationIP":safe_ip(&c.metadata.destination_ip),"destinationPort":c.metadata.destination_port.parse::<u16>().unwrap_or(0),
                "host":host,"uploadBytes":c.upload,"downloadBytes":c.download,"outbound":outbound,
                "ruleId":id(&self.key,&c.rule),"rule":rule});
            // These identities come from actual connection chains, never a
            // fabricated selection or accepted server/password configuration.
            if let Some(node) = selected {
                dto["nodeId"] = json!(id(&self.key, &node));
            }
            connections.push(dto);
        }
        self.core_traffic.push_back(
            json!({"time":timestamp(now),"uploadRate":ur,"downloadRate":dr,"reset":reset}),
        );
        if self.core_traffic.len() > 900 {
            self.core_traffic.pop_front();
        }
        let previous_selection = if self.core_epoch == Some(epoch) {
            self.core.get("selectedOutbounds").cloned()
        } else {
            None
        };
        self.core = json!({"state":"ready","reason":"","source":"sing-box Clash API · localhost","sampledAt":timestamp(now),
            "capabilities":{"connections":capability(true,"仅活跃连接；短于采样间隔的连接可能未被观测，不提供结束时间"),
                "traffic":capability(true,"核心总流量（含直连）；内存保留最近 900 点，非 WAN 历史"),
                "routing":capability(routing || raw.connections.count==0,"仅活跃连接的实际匹配规则与出站；不是全量规则命中计数"),
                "latency":capability(can_probe,if can_probe {"手动测量当前 proxy 出站到固定 HTTPS 204 目标的请求延迟；不是连接 RTT"} else {"当前配置没有可供探测的选中 proxy 出站"}),
                "requestPhases":capability(false,"HTTPS 内容不透明；核心不提供 DNS/TCP/TLS/TTFB 请求阶段")},
            "totals":{"uploadBytes":raw.upload,"downloadBytes":raw.download},"activeConnections":raw.connections.count,
            "truncated":raw.connections.count>128,"connections":connections,"traffic":self.core_traffic,"probes":self.probes});
        if let Some(selected) = previous_selection {
            self.core["selectedOutbounds"] = selected;
            self.core["selectionState"] = json!("stale");
        }
        self.core_previous = Some((now, raw.upload, raw.download, epoch));
        self.core_epoch = Some(epoch);
    }
    #[allow(clippy::too_many_arguments)] // Fixed authenticated boundary preserves separate authority inputs.
    pub fn handle(
        &mut self,
        path: &str,
        method: Method,
        query: &str,
        body: &[u8],
        native: Option<&[u8]>,
        io: &mut impl Backend,
        budget: &Budget<'_>,
    ) -> Result<Value, ApiError> {
        check(budget)?;
        if !matches!(
            path,
            "/api/proxy/metrics"
                | "/api/proxy/probe"
                | "/api/traffic/history"
                | "/api/devices/activity"
                | "/api/devices/annotations"
        ) {
            return Err(error(404, "not_found", "Telemetry endpoint was not found."));
        }
        if path == "/api/proxy/probe" && method != Method::Post
            || method != Method::Get
                && !(method == Method::Post
                    && matches!(path, "/api/devices/annotations" | "/api/proxy/probe"))
        {
            return Err(error(
                405,
                "method_not_allowed",
                "Method is not allowed for this endpoint.",
            ));
        }
        if method == Method::Get && !body.is_empty() {
            return Err(invalid());
        }
        let q = query_fields(query)?;
        match path {
            "/api/proxy/metrics" => {
                if !q.is_empty() {
                    return Err(invalid());
                }
                let mut value = self.core.clone();
                if self
                    .core_previous
                    .is_some_and(|(at, _, _, _)| io.now_unix().saturating_sub(at) > 6)
                    && value["state"] == "ready"
                {
                    value["state"] = json!("stale");
                    value["reason"] = json!("核心样本已过期；下方为最后一次观测");
                }
                Ok(value)
            }
            "/api/proxy/probe" => {
                if !q.is_empty() {
                    return Err(invalid());
                }
                self.probe(body, native, io.now_unix(), budget)
            }
            "/api/traffic/history" => self.wan_history(q, io.now_unix(), budget),
            "/api/devices/activity" => self.device_history(q, io.now_unix(), budget),
            "/api/devices/annotations" => {
                if !q.is_empty() {
                    return Err(invalid());
                }
                if method == Method::Post {
                    self.save_annotation(body, budget)?;
                }
                let state = self
                    .annotations
                    .as_ref()
                    .ok_or_else(|| self.annotation_error.unwrap_or_else(storage))?;
                serde_json::to_value(state).map_err(|_| storage())
            }
            _ => unreachable!(),
        }
    }
    fn probe(
        &mut self,
        body: &[u8],
        native: Option<&[u8]>,
        now: u64,
        b: &Budget<'_>,
    ) -> Result<Value, ApiError> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Empty {}
        if body.len() > 1024 || serde_json::from_slice::<Empty>(body).is_err() {
            return Err(invalid());
        }
        if self.last_probe > 0 && now >= self.last_probe && now - self.last_probe < 10 {
            return Err(error(
                429,
                "probe_cooldown",
                "Wait before probing the selected outbound again.",
            ));
        }
        let raw = native.ok_or_else(|| {
            error(
                503,
                "telemetry_unavailable",
                "Local core telemetry is not ready.",
            )
        })?;
        let (api, can_probe, epoch) = accepted(raw).map_err(|_| {
            error(
                503,
                "telemetry_unavailable",
                "Local core telemetry is not ready.",
            )
        })?;
        if !can_probe {
            return Err(error(
                503,
                "telemetry_unavailable",
                "The selected proxy outbound cannot be probed.",
            ));
        }
        self.last_probe = now;
        #[derive(Deserialize)]
        struct Delay {
            delay: u16,
        }
        let response = core_get(
            &api,
            "/proxies/proxy/delay?timeout=5000&url=https%3A%2F%2Fwww.gstatic.com%2Fgenerate_204",
            b,
        )
        .and_then(|bytes| serde_json::from_slice::<Delay>(&bytes).map_err(|_| invalid()));
        if self.core_epoch.is_some_and(|e| e != epoch) {
            self.probes.clear();
        }
        self.core_epoch = Some(epoch);
        let delay = response.ok().map(|d| d.delay).filter(|d| *d > 0);
        self.probes.push_back(json!({"time":timestamp(now),"delayMs":delay.unwrap_or(0),"status":if delay.is_some() {"ok"} else {"failed"}}));
        if self.probes.len() > 32 {
            self.probes.pop_front();
        }
        self.core["probes"] = json!(self.probes);
        if delay.is_none() {
            Err(error(
                502,
                "probe_failed",
                "The selected outbound latency probe failed.",
            ))
        } else {
            Ok(self.core.clone())
        }
    }
    fn save_annotation(&mut self, body: &[u8], b: &Budget<'_>) -> Result<(), ApiError> {
        check(b)?;
        if body.len() > 65536 {
            return Err(error(
                413,
                "body_too_large",
                "Request body exceeds the endpoint limit.",
            ));
        }
        // Distinguish malformed/duplicate/unknown JSON from missing required
        // fields, as the mature authenticated annotations endpoint does.
        let mut decoder = serde_json::Deserializer::from_slice(body);
        UniqueObject::deserialize(&mut decoder).map_err(|_| {
            error(
                400,
                "invalid_json",
                "Provide one JSON object with supported fields.",
            )
        })?;
        decoder.end().map_err(|_| {
            error(
                400,
                "invalid_json",
                "Provide one JSON object with supported fields.",
            )
        })?;
        let fields: BTreeMap<String, Box<serde_json::value::RawValue>> =
            serde_json::from_slice(body).map_err(|_| invalid())?;
        if fields.keys().any(|k| {
            !matches!(
                k.as_str(),
                "mac" | "label" | "note" | "tags" | "expectedRevision"
            )
        }) {
            return Err(error(
                400,
                "invalid_json",
                "Provide one JSON object with supported fields.",
            ));
        }
        if ["mac", "label", "note", "tags", "expectedRevision"]
            .iter()
            .any(|k| fields.get(*k).is_none_or(|v| v.get() == "null"))
        {
            return Err(invalid());
        }
        let u: AnnotationUpdate = serde_json::from_slice(body).map_err(|_| {
            error(
                400,
                "invalid_json",
                "Provide correctly typed annotation fields.",
            )
        })?;
        let mac = canonical_mac(&u.mac).ok_or_else(invalid)?;
        let a = Annotation {
            label: u.label,
            note: u.note,
            tags: u.tags,
        };
        if !a.valid() || u.expected_revision > MAX_REVISION {
            return Err(invalid());
        }
        // Reload the small bounded document for CAS. A second owner cannot be
        // silently overwritten. Corrupt existing state is never reset.
        let current = load_annotations(&self.dir)?;
        if current.revision != u.expected_revision {
            self.annotations = Some(current);
            return Err(error(
                409,
                "revision_conflict",
                "Device annotations changed; refresh before saving.",
            ));
        }
        if current.revision == MAX_REVISION {
            return Err(error(
                409,
                "revision_exhausted",
                "Device annotation revision limit reached.",
            ));
        }
        let mut candidate = current;
        if a.empty() {
            candidate.devices.remove(&mac);
        } else {
            if !candidate.devices.contains_key(&mac) && candidate.devices.len() >= 256 {
                return Err(error(
                    400,
                    "annotation_limit",
                    "Device annotations are limited to 256 MAC addresses.",
                ));
            }
            candidate.devices.insert(mac, a);
        }
        candidate.revision += 1;
        let bytes = serde_json::to_vec(&candidate).map_err(|_| storage())?;
        if bytes.len() as u64 > ANNOTATION_BYTES {
            return Err(storage());
        }
        admit(&self.dir, bytes.len() as u64)?;
        let (tmp, mut f) = temp_file(&self.dir, "device-names")?;
        let mut committed = false;
        let result = (|| {
            for chunk in bytes.chunks(32768) {
                check(b)?;
                f.write_all(chunk).map_err(|_| storage())?;
            }
            check(b)?;
            f.sync_all().map_err(|_| storage())?;
            drop(f);
            check(b)?;
            let path = self.dir.join("device-names.json");
            regular(&path, ANNOTATION_BYTES)?;
            fs::rename(&tmp, path).map_err(|_| storage())?;
            committed = true;
            File::open(&self.dir)
                .and_then(|d| d.sync_all())
                .map_err(|_| storage())
        })();
        if committed {
            self.annotations = Some(candidate);
            self.annotation_error = None;
        }
        let _ = fs::remove_file(tmp);
        result
    }

    fn wan_history(
        &self,
        q: BTreeMap<String, String>,
        now: u64,
        b: &Budget<'_>,
    ) -> Result<Value, ApiError> {
        if q.keys()
            .any(|k| !matches!(k.as_str(), "range" | "maxPoints"))
        {
            return Err(invalid());
        }
        let name = q
            .get("range")
            .map(String::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("30m");
        let duration = RANGES
            .iter()
            .find(|r| r.0 == name)
            .map(|r| r.1)
            .ok_or_else(invalid)?;
        let max = parameter(&q, "maxPoints", 1500, 1, 2000)?;
        let seconds = if duration > 2592000 {
            3600
        } else if duration > 172800 {
            300
        } else {
            30
        };
        let first = now.saturating_sub(duration) / seconds * seconds;
        let last = now / seconds * seconds;
        let count = (last - first) / seconds + 1;
        let factor = count.div_ceil(max as u64);
        let resolution = seconds * factor;
        let mut h = json!({"enabled":!self.rings.is_empty(),"persistent":!self.rings.is_empty() && self.history_error.is_empty(),
            "retentionDays":400,"source":self.wan_source,"range":name,"resolutionSeconds":resolution,
            "supportedRanges":RANGES.iter().map(|r|r.0).collect::<Vec<_>>(),"samples":[],
            "summary":{"rxBytes":0,"txBytes":0,"coverageSeconds":0},"maxUnsyncedSeconds":60});
        if self.last_flush > 0 {
            h["lastFlushAt"] = json!(timestamp(self.last_flush));
        }
        let warning = if !self.history_error.is_empty() {
            self.history_error
        } else if !self.observation_error.is_empty() {
            self.observation_error
        } else if self.rings.iter().any(|r| r.recovered) {
            "Damaged history records were skipped; previous valid records were recovered."
        } else {
            ""
        };
        if !warning.is_empty() {
            h["error"] = json!(warning);
        }
        let Some(ring) = self.rings.iter().find(|r| r.seconds == seconds) else {
            h["error"] = json!(
                "The requested retention tier is unavailable; existing history files were retained."
            );
            return Ok(h);
        };
        let mut samples = vec![Bucket::default(); count.div_ceil(factor) as usize];
        for (i, s) in samples.iter_mut().enumerate() {
            s.start = first + i as u64 * resolution;
        }
        let mut oldest = None;
        for r in &self.rings {
            r.scan(Some(b), |_, v| {
                if v.coverage > 0
                    && v.start >= now.saturating_sub(r.seconds * r.capacity as u64)
                    && v.start <= now
                {
                    oldest = Some(oldest.map_or(v.start, |o: u64| o.min(v.start)));
                }
                Ok(())
            })?;
        }
        if let Some(oldest) = oldest {
            h["oldestAt"] = json!(timestamp(oldest));
        }
        let mut total = (0u64, 0u64, 0u64);
        ring.scan(Some(b), |_, v| {
            if v.start < first || v.start > last {
                return Ok(());
            }
            let s = &mut samples[((v.start - first) / resolution) as usize];
            s.rx = s.rx.checked_add(v.rx).ok_or_else(storage)?;
            s.tx = s.tx.checked_add(v.tx).ok_or_else(storage)?;
            s.coverage = s.coverage.checked_add(v.coverage).ok_or_else(storage)?;
            s.rx_peak = s.rx_peak.max(v.rx_peak);
            s.tx_peak = s.tx_peak.max(v.tx_peak);
            total.0 = total.0.checked_add(v.rx).ok_or_else(storage)?;
            total.1 = total.1.checked_add(v.tx).ok_or_else(storage)?;
            total.2 = total.2.checked_add(v.coverage).ok_or_else(storage)?;
            Ok(())
        })?;
        h["samples"]=json!(samples.into_iter().map(|s| {
            let coverage=s.coverage as f64/1e9;
            // Compatibility uses numeric placeholders with coverage=0. Never
            // claim these are observed zero rates; charts use the coverage gap.
            json!({"time":timestamp(s.start),"rx":if coverage>0.0 {s.rx as f64/coverage}else{0.0},
                "tx":if coverage>0.0 {s.tx as f64/coverage}else{0.0},"rxPeak":s.rx_peak,"txPeak":s.tx_peak,
                "rxBytes":s.rx,"txBytes":s.tx,"coverageSeconds":coverage})
        }).collect::<Vec<_>>());
        h["summary"] =
            json!({"rxBytes":total.0,"txBytes":total.1,"coverageSeconds":total.2 as f64/1e9});
        if let Some((rx, tx)) = self.wan_rate {
            h["current"] =
                json!({"time":timestamp(self.wan.as_ref().map_or(now,|v|v.at)),"rx":rx,"tx":tx});
        }
        Ok(h)
    }
    fn device_history(
        &self,
        q: BTreeMap<String, String>,
        now: u64,
        b: &Budget<'_>,
    ) -> Result<Value, ApiError> {
        if q.keys().any(|k| {
            !matches!(
                k.as_str(),
                "range" | "maxPoints" | "limit" | "search" | "offset"
            )
        }) {
            return Err(invalid());
        }
        let name = q
            .get("range")
            .map(String::as_str)
            .filter(|s| !s.is_empty())
            .unwrap_or("24h");
        let duration = match name {
            "30m" => 1800,
            "24h" => 86400,
            "7d" => 604800,
            _ => return Err(invalid()),
        };
        let max = parameter(&q, "maxPoints", 288, 1, 288)?;
        let limit = parameter(&q, "limit", 32, 1, 64)?;
        let offset = parameter(&q, "offset", 0, 0, 128)?;
        let search = q.get("search").map(String::as_str).unwrap_or("");
        if !text(search, 64) || search.len() > 256 {
            return Err(invalid());
        }
        if !search.trim().is_empty() && self.annotations.is_none() {
            return Err(error(
                503,
                "annotations_unavailable",
                "Device annotations could not be read for search.",
            ));
        }
        let search = search.trim().to_lowercase();
        let exact = canonical_mac(&search);
        let seconds = if name == "30m" { 300 } else { 3600 };
        let first = now.saturating_sub(duration) / seconds * seconds;
        let last = now / seconds * seconds;
        let count = (last - first) / seconds + 1;
        let factor = count.div_ceil(max as u64);
        let resolution = seconds * factor;
        let state = if self.device_unavailable {
            "unavailable"
        } else if self.device_at == 0 {
            "waiting"
        } else if now.saturating_sub(self.device_at) > 60 {
            "stale"
        } else {
            "ok"
        };
        let mut h = json!({"enabled":true,"persistent":false,"retentionDays":7,"source":"trafficd","direction":"vendor-rx-tx", "range":name,
            "resolutionSeconds":resolution,"state":state,"deviceCount":0,"matchedCount":0,"truncated":self.device_truncated,"devices":[],"groups":[]});
        if self.device_at > 0 {
            h["sampledAt"] = json!(timestamp(self.device_at));
        }
        if !self.device_error.is_empty() {
            h["error"] = json!(self.device_error);
        }
        let mut conflicts: BTreeMap<&str, usize> = BTreeMap::new();
        for d in self.devices.values() {
            if d.observation.associated
                && d.seen == self.device_at
                && now.saturating_sub(d.seen) <= 60
            {
                for c in &d.observation.counters {
                    *conflicts.entry(&c.address).or_default() += 1;
                }
            }
        }
        // First pass computes bounded metadata and group totals for every
        // matched identity. Allocate full sample arrays only for the page.
        let mut summaries = Vec::new();
        let mut groups: BTreeMap<String, GroupSummary> = BTreeMap::new();
        let mut oldest = None;
        let mut device_count = 0;
        for (id, d) in &self.devices {
            check(b)?;
            if now.saturating_sub(d.seen) > 604800 {
                continue;
            }
            device_count += 1;
            let o = &d.observation;
            let annotation = self.annotations.as_ref().and_then(|s| s.devices.get(id));
            let mut identity = format!(
                "{id} {} {} {}",
                o.name,
                o.interface,
                o.counters
                    .iter()
                    .map(|c| c.address.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            );
            if let Some(a) = annotation {
                identity.push_str(&format!(" {} {} {}", a.label, a.note, a.tags.join(" ")));
            }
            if exact.as_ref().is_some_and(|mac| mac != id)
                || exact.is_none()
                    && !search.is_empty()
                    && !identity.to_lowercase().contains(&search)
            {
                continue;
            }
            let ring = if name == "30m" { &d.fine } else { &d.coarse };
            let mut sum = (0u64, 0u64, 0u64);
            for bucket in ring {
                if bucket.coverage == 0 {
                    continue;
                }
                if bucket.start >= now.saturating_sub(604800) && bucket.start <= last {
                    oldest = Some(oldest.map_or(bucket.start, |v: u64| v.min(bucket.start)));
                }
                if bucket.start < first || bucket.start > last {
                    continue;
                }
                sum.0 = sum.0.checked_add(bucket.rx).ok_or_else(invalid)?;
                sum.1 = sum.1.checked_add(bucket.tx).ok_or_else(invalid)?;
                sum.2 += bucket.coverage;
            }
            let stale = now.saturating_sub(d.seen) > 60
                || d.seen < self.device_at
                || state == "stale"
                || state == "unavailable"
                || o.ageing.is_some_and(|v| v > 60);
            let rate = if stale { None } else { d.rates };
            let group = groups
                .entry(if o.interface.is_empty() {
                    "unreported".into()
                } else {
                    o.interface.clone()
                })
                .or_default();
            group.count += 1;
            group.rx = group.rx.checked_add(sum.0).ok_or_else(invalid)?;
            group.tx = group.tx.checked_add(sum.1).ok_or_else(invalid)?;
            group.coverage += sum.2;
            if let Some((rx, tx)) = rate {
                let r = group.rate.get_or_insert((0.0, 0.0));
                r.0 += rx;
                r.1 += tx;
            }
            summaries.push((id.clone(), sum, stale, rate));
        }
        summaries.sort_by(|a, b| {
            (u128::from(b.1.0) + u128::from(b.1.1))
                .cmp(&(u128::from(a.1.0) + u128::from(a.1.1)))
                .then(a.2.cmp(&b.2))
                .then(a.0.cmp(&b.0))
        });
        let matched = summaries.len();
        let mut devices = Vec::new();
        for (id, sum, stale, rate) in summaries.into_iter().skip(offset).take(limit) {
            check(b)?;
            let d = &self.devices[&id];
            let o = &d.observation;
            let ring = if name == "30m" { &d.fine } else { &d.coarse };
            let mut points = vec![DeviceBucket::default(); count.div_ceil(factor) as usize];
            for (i, p) in points.iter_mut().enumerate() {
                p.start = first + i as u64 * resolution;
            }
            for p in ring {
                if p.coverage == 0 || p.start < first || p.start > last {
                    continue;
                }
                let t = &mut points[((p.start - first) / resolution) as usize];
                t.rx += p.rx;
                t.tx += p.tx;
                t.coverage += p.coverage;
            }
            let addresses = o.counters.iter().map(|c| &c.address).collect::<Vec<_>>();
            let mut dto = json!({"id":id,"name":o.name,"addresses":addresses,"interface":o.interface,"associated":o.associated,
                "lastSeen":timestamp(d.seen),"stale":stale,"rxBytes":sum.0,"txBytes":sum.1,"coverageSeconds":sum.2,
                "counters":o.counters,"links":o.links,"addressConflicts":o.counters.iter().filter(|c|conflicts.get(c.address.as_str()).is_some_and(|v|*v>1)).map(|c|c.address.clone()).collect::<Vec<_>>(),
                "samples":points.iter().map(|p|json!({"time":timestamp(p.start),"rxBytes":if p.coverage>0 {Some(p.rx)}else{None},
                    "txBytes":if p.coverage>0 {Some(p.tx)}else{None},"coverageSeconds":p.coverage})).collect::<Vec<_>>()});
            if !o.counters.is_empty()
                && let Some((rx, tx)) = totals(&o.counters)
            {
                dto["rawRXBytes"] = json!(rx);
                dto["rawTXBytes"] = json!(tx);
            }
            if let Some((rx, tx)) = rate {
                dto["rxBytesPerSecond"] = json!(rx);
                dto["txBytesPerSecond"] = json!(tx);
            }
            if let Some(v) = o.online {
                dto["onlineSeconds"] = json!(v);
            }
            if let Some(v) = o.ageing {
                dto["ageingSeconds"] = json!(v);
            }
            devices.push(dto);
        }
        h["deviceCount"] = json!(device_count);
        h["matchedCount"] = json!(matched);
        h["truncated"] = json!(self.device_truncated || matched > offset + devices.len());
        h["offset"] = json!(offset);
        if offset + devices.len() < matched {
            h["nextOffset"] = json!(offset + devices.len());
        }
        h["devices"] = json!(devices);
        h["groups"]=json!(groups.into_iter().map(|(name,g)| {
            let mut value=json!({"name":name,"deviceCount":g.count,"rxBytes":g.rx,"txBytes":g.tx,"coverageSeconds":g.coverage});
            if let Some((rx,tx))=g.rate {value["rxBytesPerSecond"]=json!(rx);value["txBytesPerSecond"]=json!(tx);}value
        }).collect::<Vec<_>>());
        if let Some(at) = oldest {
            h["oldestAt"] = json!(timestamp(at));
        }
        Ok(h)
    }
}
#[derive(Default)]
struct GroupSummary {
    count: usize,
    rx: u64,
    tx: u64,
    coverage: u64,
    rate: Option<(f64, f64)>,
}
fn query_fields(query: &str) -> Result<BTreeMap<String, String>, ApiError> {
    if query.len() > 2048 {
        return Err(invalid());
    }
    let mut out = BTreeMap::new();
    if query.is_empty() {
        return Ok(out);
    }
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let key = decode_query(key)?;
        let value = decode_query(value)?;
        if out.len() >= 8 || key.is_empty() || out.insert(key, value).is_some() {
            return Err(invalid());
        }
    }
    Ok(out)
}
fn decode_query(s: &str) -> Result<String, ApiError> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if i + 2 >= b.len() {
                return Err(invalid());
            }
            let hex = |c: u8| -> Option<u8> {
                match c {
                    b'0'..=b'9' => Some(c - b'0'),
                    b'a'..=b'f' => Some(c - b'a' + 10),
                    b'A'..=b'F' => Some(c - b'A' + 10),
                    _ => None,
                }
            };
            out.push(hex(b[i + 1]).ok_or_else(invalid)? * 16 + hex(b[i + 2]).ok_or_else(invalid)?);
            i += 3;
        } else {
            out.push(if b[i] == b'+' { b' ' } else { b[i] });
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| invalid())
}
fn parameter(
    q: &BTreeMap<String, String>,
    key: &str,
    default: usize,
    min: usize,
    max: usize,
) -> Result<usize, ApiError> {
    match q.get(key) {
        None => Ok(default),
        Some(s) => s
            .parse::<usize>()
            .ok()
            .filter(|v| *v >= min && *v <= max)
            .ok_or_else(invalid),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admission_reserves_one_mib_plus_metadata_and_rounds_growth() {
        assert_eq!(
            admit_free(RESERVE + 4096 + 4095, 4096, 1)
                .unwrap_err()
                .status,
            507
        );
        assert!(admit_free(RESERVE + 4096 + 4096, 4096, 1).is_ok());
        assert!(admit_free(u64::MAX, 4096, u64::MAX).is_err());
    }
    #[test]
    fn real_go_record_crc_layout_round_trips_and_rejects_orphan_slots() {
        let b = Bucket {
            start: 300,
            rx: 120,
            tx: 40,
            coverage: 2_000_000_000,
            rx_peak: 60.0,
            tx_peak: 20.0,
            generation: 9,
            end_ms: 2000,
        };
        let raw = b.encode();
        let read = Bucket::decode(&raw, 30, 5760, 10).unwrap();
        assert_eq!((read.rx, read.tx, read.coverage), (120, 40, 2_000_000_000));
        assert!(Bucket::decode(&raw, 30, 5760, 11).is_none());
        let mut corrupt = raw;
        corrupt[16] ^= 1;
        assert!(Bucket::decode(&corrupt, 30, 5760, 10).is_none());
        assert_eq!(crc32(b"123456789"), 0xcbf43926);
    }
    #[test]
    fn literal_listener_only_and_borrowed_private_credentials() {
        let good=br#"{"experimental":{"clash_api":{"external_controller":"127.0.0.1:9090","secret":"private"}},"outbounds":[{"tag":"proxy","type":"selector"}]}"#;
        let (api, probe, _) = accepted(good).unwrap();
        assert!(probe);
        assert!(matches!(api.secret, std::borrow::Cow::Borrowed("private")));
        for address in [
            "localhost:9090",
            "0.0.0.0:9090",
            "192.0.2.1:9090",
            "127.0.0.1:0",
        ] {
            let raw = json!({"experimental":{"clash_api":{"external_controller":address,"secret":"hidden"}}});
            assert!(accepted(&serde_json::to_vec(&raw).unwrap()).is_err());
        }
        assert_eq!(
            safe_rule("auth_user=private => route(proxy)"),
            "实际匹配规则（非公开条件）"
        );
    }
    #[test]
    fn finite_chunked_http_and_real_rfc3339_offsets() {
        assert_eq!(
            decode_chunks(b"4\r\n{\"x\"\r\n3\r\n:1}\r\n0\r\n\r\n").unwrap(),
            br#"{"x":1}"#
        );
        assert!(!chunk_complete(b"4\r\nabc").unwrap());
        assert!(decode_chunks(b"ffffffffffffffff\r\n").is_err());
        assert_eq!(
            parse_time("2026-10-05T08:00:00+08:00"),
            parse_time("2026-10-05T00:00:00Z")
        );
        assert!(parse_time("2026-02-30T00:00:00Z").is_none());
    }
}
