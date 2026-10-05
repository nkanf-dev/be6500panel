use be6500_panel::static_files::{StaticFiles, TRANSFER_BUFFER_BYTES};
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "be6500-static-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn static_rejects_traversal_decoding_errors_directories_and_unknown_files() {
    let fixture = Fixture::new();
    fs::write(fixture.0.join("index.html"), b"index").unwrap();
    fs::create_dir(fixture.0.join("sub")).unwrap();
    fs::write(fixture.0.join("space name.css"), b"css").unwrap();
    fs::write(fixture.0.join("bad\u{85}"), b"control name").unwrap();
    let files = StaticFiles::new(&fixture.0).unwrap();
    for invalid in [
        "/../outside",
        "/sub/../index.html",
        "/./index.html",
        "/%2e%2e/outside",
        "/sub/%2E%2E/index.html",
        "/%2f../outside",
        "//index.html",
        "/sub//file",
        "/sub/",
        "/sub",
        "/unknown",
        "index.html",
        "/bad%",
        "/bad%zz",
        "/bad%00",
        "/bad%0a",
        "/bad%7f",
        "/bad%ff",
        "/bad%c2%85",
        "/bad%c2%9f",
        "/bad%5cname",
    ] {
        assert!(files.open(invalid).is_err(), "accepted {invalid:?}");
    }
    assert!(files.open(&format!("/{}", "a".repeat(2048))).is_err());
    assert_eq!(files.open("/").unwrap().length, 5);
    assert_eq!(
        files.open("/space%20name.css").unwrap().content_type,
        "text/css; charset=utf-8"
    );
    assert!(StaticFiles::new(&fixture.0.join("unknown")).is_err());
    assert!(StaticFiles::new(&fixture.0.join("index.html")).is_err());
}

#[cfg(unix)]
#[test]
fn static_rejects_symlink_files_and_components_even_within_root() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    let outside = Fixture::new();
    fs::write(outside.0.join("secret"), b"private").unwrap();
    fs::write(fixture.0.join("index.html"), b"index").unwrap();
    symlink(outside.0.join("secret"), fixture.0.join("escape")).unwrap();
    symlink(&outside.0, fixture.0.join("directory")).unwrap();
    symlink(
        fixture.0.join("index.html"),
        fixture.0.join("internal-link"),
    )
    .unwrap();
    let files = StaticFiles::new(&fixture.0).unwrap();
    for invalid in ["/escape", "/directory/secret", "/internal-link"] {
        assert!(files.open(invalid).is_err());
    }
}

struct ChunkCounter {
    total: usize,
    largest: usize,
}
impl Write for ChunkCounter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.total += bytes.len();
        self.largest = self.largest.max(bytes.len());
        assert!(bytes.iter().all(|b| *b == 42));
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn static_large_file_streams_in_fixed_chunks_and_original_length() {
    let fixture = Fixture::new();
    let length = TRANSFER_BUFFER_BYTES * 20 + 3;
    fs::write(fixture.0.join("big.bin"), vec![42_u8; length]).unwrap();
    let files = StaticFiles::new(&fixture.0).unwrap();
    let mut asset = files.open("/big.bin").unwrap();
    assert_eq!(asset.length, length as u64);
    // A growing asset must not make the body exceed the advertised length.
    fs::write(fixture.0.join("big.bin"), vec![42_u8; length + 19]).unwrap();
    let mut writer = ChunkCounter {
        total: 0,
        largest: 0,
    };
    asset.stream(&mut writer).unwrap();
    assert_eq!(writer.total, length);
    assert_eq!(writer.largest, TRANSFER_BUFFER_BYTES);
    assert_eq!(asset.content_type, "application/octet-stream");
    let mut shrinking = files.open("/big.bin").unwrap();
    fs::write(fixture.0.join("big.bin"), b"").unwrap();
    assert!(shrinking.stream(&mut writer).is_err());
}
