//! Real procfs memory diagnostics. Linux meminfo's `kB` means 1024 bytes.
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

pub const MAX_MEMINFO_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemorySnapshot {
    pub total_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug)]
pub enum MemoryError {
    Io(io::Error),
    TooLarge,
    InvalidUtf8,
    MissingField,
    DuplicateField,
    InvalidField,
    Overflow,
    AvailableExceedsTotal,
}

impl fmt::Display for MemoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("memory source unavailable or invalid")
    }
}

impl std::error::Error for MemoryError {}

pub fn parse_meminfo(input: &str) -> Result<MemorySnapshot, MemoryError> {
    if input.len() > MAX_MEMINFO_BYTES {
        return Err(MemoryError::TooLarge);
    }
    let mut total = None;
    let mut available = None;
    for line in input.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let slot = match name {
            "MemTotal" => &mut total,
            "MemAvailable" => &mut available,
            _ => continue,
        };
        if slot.is_some() {
            return Err(MemoryError::DuplicateField);
        }
        let mut fields = value.split_ascii_whitespace();
        let number = fields.next().ok_or(MemoryError::InvalidField)?;
        if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
            return Err(MemoryError::InvalidField);
        }
        if fields.next() != Some("kB") || fields.next().is_some() {
            return Err(MemoryError::InvalidField);
        }
        let kib = number.parse::<u64>().map_err(|_| MemoryError::Overflow)?;
        *slot = Some(kib.checked_mul(1024).ok_or(MemoryError::Overflow)?);
    }
    let total_bytes = total.ok_or(MemoryError::MissingField)?;
    let available_bytes = available.ok_or(MemoryError::MissingField)?;
    if available_bytes > total_bytes {
        return Err(MemoryError::AvailableExceedsTotal);
    }
    Ok(MemorySnapshot {
        total_bytes,
        available_bytes,
    })
}

/// Reads only `meminfo` under the supplied proc root, with a 64 KiB source cap.
pub fn read_memory(proc_root: &Path) -> Result<MemorySnapshot, MemoryError> {
    let file = File::open(proc_root.join("meminfo")).map_err(MemoryError::Io)?;
    let mut bytes = Vec::with_capacity(4096);
    file.take((MAX_MEMINFO_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(MemoryError::Io)?;
    if bytes.len() > MAX_MEMINFO_BYTES {
        return Err(MemoryError::TooLarge);
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| MemoryError::InvalidUtf8)?;
    parse_meminfo(text)
}
