use be6500_panel::memory::{MAX_MEMINFO_BYTES, parse_meminfo, read_memory};
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

#[test]
fn memory_requires_available_and_preserves_kib_units() {
    let m = parse_meminfo("MemTotal: 100 kB\nMemAvailable: 25 kB\n").unwrap();
    assert_eq!(m.total_bytes, 102400);
    assert_eq!(m.available_bytes, 25600);
    assert!(parse_meminfo("MemTotal: 100 kB\nMemFree: 25 kB\n").is_err());
    assert!(parse_meminfo("MemTotal: 1 kB\nMemAvailable: 2 kB\n").is_err());
    assert!(parse_meminfo("MemTotal: 1 kB\nMemTotal: 1 kB\nMemAvailable: 0 kB\n").is_err());
    assert!(parse_meminfo("MemTotal: 1 kB\nMemAvailable: 0 kB\nMemAvailable: 0 kB\n").is_err());
}

#[test]
fn memory_rejects_invalid_units_values_and_overflow() {
    for bad in [
        "MemTotal: 10 MB\nMemAvailable: 1 kB\n",
        "MemTotal: 10 kB\nMemAvailable: 1 KB\n",
        "MemTotal: 10\nMemAvailable: 1 kB\n",
        "MemTotal: +10 kB\nMemAvailable: 1 kB\n",
        "MemTotal: -10 kB\nMemAvailable: 1 kB\n",
        "MemTotal: 10 kB extra\nMemAvailable: 1 kB\n",
        "MemTotal: 18446744073709551615 kB\nMemAvailable: 1 kB\n",
        "MemTotal: 18446744073709551616 kB\nMemAvailable: 1 kB\n",
        "MemAvailable: 1 kB\n",
    ] {
        assert!(parse_meminfo(bad).is_err(), "accepted {bad:?}");
    }
    let m = parse_meminfo("MemTotal: 0 kB\nMemAvailable: 0 kB\n").unwrap();
    assert_eq!(m.total_bytes, 0);
}

#[test]
fn memory_ignores_other_fields_without_inventing_available() {
    let m =
        parse_meminfo("MemFree: 2 kB\nHugePages_Total: 0\nMemTotal: 10 kB\nMemAvailable:\t3 kB\n")
            .unwrap();
    assert_eq!(m.available_bytes, 3072);
}

#[test]
fn memory_read_is_bounded_and_requires_readable_utf8_source() {
    let fixture = std::env::temp_dir().join(format!(
        "be6500-memory-{}-{}",
        std::process::id(),
        NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&fixture).unwrap();
    let meminfo = fixture.join("meminfo");
    assert!(read_memory(&fixture).is_err());
    fs::write(&meminfo, b"MemTotal: 10 kB\nMemAvailable: 2 kB\n").unwrap();
    assert_eq!(read_memory(&fixture).unwrap().available_bytes, 2048);
    fs::write(&meminfo, vec![b' '; MAX_MEMINFO_BYTES + 1]).unwrap();
    assert!(read_memory(&fixture).is_err());
    assert!(parse_meminfo(&" ".repeat(MAX_MEMINFO_BYTES + 1)).is_err());
    fs::write(&meminfo, [0xff, 0xff]).unwrap();
    assert!(read_memory(&fixture).is_err());
    fs::remove_dir_all(&fixture).unwrap();
}
