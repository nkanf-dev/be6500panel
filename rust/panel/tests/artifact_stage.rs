//! Synthetic streams and private temporary files only; no transport or activation.
use be6500_panel::artifact_stage::{
    MAX_DECODED_BYTES, MAX_ENCODED_BYTES, STREAM_BYTES, Stage, StageError,
};
use be6500_panel::readiness_tun::Budget;
use be6500_panel::runtime_store::Artifact;
use flate2::{Compression, read::GzEncoder, write::GzEncoder as WriteGzip};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, Cursor, Read, Write},
    os::unix::fs::{MetadataExt, PermissionsExt, symlink},
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::{Duration, Instant},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!(
                "b6p-artifact-stage-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir(&root).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        Self(root)
    }
    fn files(&self) -> Vec<PathBuf> {
        fs::read_dir(&self.0)
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn budget(cancel: &AtomicBool) -> Budget<'_> {
    Budget {
        deadline: Instant::now() + Duration::from_secs(60),
        cancel,
    }
}
fn artifact(bytes: &[u8], compression: &str) -> Artifact {
    Artifact {
        url: "https://synthetic.invalid/private-artifact".into(),
        sha256: format!("{:x}", Sha256::digest(bytes)),
        compression: compression.into(),
        version: "synthetic-v1".into(),
    }
}
fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut gzip = WriteGzip::new(Vec::new(), Compression::fast());
    gzip.write_all(bytes).unwrap();
    gzip.finish().unwrap()
}
fn digest(mut reader: impl Read) -> [u8; 32] {
    let mut hash = Sha256::new();
    let mut scratch = [0; STREAM_BYTES];
    loop {
        let n = reader.read(&mut scratch).unwrap();
        if n == 0 {
            break;
        }
        hash.update(&scratch[..n]);
    }
    hash.finalize().into()
}
struct Repeat {
    remaining: u64,
    byte: u8,
}
impl Read for Repeat {
    fn read(&mut self, into: &mut [u8]) -> io::Result<usize> {
        let n = into.len().min(self.remaining as usize);
        into[..n].fill(self.byte);
        self.remaining -= n as u64;
        Ok(n)
    }
}
fn repeated(length: u64) -> Repeat {
    Repeat {
        remaining: length,
        byte: 7,
    }
}
fn streaming_artifact(reader: impl Read, compression: &str) -> Artifact {
    Artifact {
        sha256: digest(reader).iter().map(|b| format!("{b:02x}")).collect(),
        ..artifact(b"", compression)
    }
}

#[test]
fn raw_stage_hashes_source_and_extracted_bytes_and_drop_removes_owned_file() {
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(false);
    let bytes = b"synthetic executable bytes";
    let request = artifact(bytes, "none");
    let stage =
        Stage::from_reader(&fixture.0, &request, Cursor::new(bytes), &budget(&cancel)).unwrap();
    let admitted = stage.admitted().unwrap();
    assert_eq!(admitted.root, fixture.0);
    assert_eq!(admitted.artifact, &request);
    assert_eq!(admitted.length, bytes.len() as u64);
    assert_eq!(
        admitted.extracted_sha256,
        <[u8; 32]>::from(Sha256::digest(bytes))
    );
    assert_eq!(fs::read(admitted.path).unwrap(), bytes);
    assert_eq!(fs::metadata(admitted.path).unwrap().mode() & 0o7777, 0o700);
    let name = admitted.path.file_name().unwrap().to_str().unwrap();
    assert!(name.starts_with(".artifact-"));
    assert_eq!(name.len(), 42);
    assert!(
        name[10..]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    );
    assert_eq!(format!("{stage:?}"), "ArtifactStage([private])");
    assert_eq!(format!("{admitted:?}"), "ArtifactAdmission([private])");
    drop(stage);
    assert!(fixture.files().is_empty());
}

#[test]
fn gzip_all_members_hash_the_whole_encoded_stream_not_decoded_bytes() {
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(false);
    let encoded = gzip(b"first")
        .into_iter()
        .chain(gzip(b"second"))
        .collect::<Vec<_>>();
    let request = artifact(&encoded, "gzip");
    let stage = Stage::from_reader(&fixture.0, &request, &encoded[..], &budget(&cancel)).unwrap();
    assert_eq!(
        fs::read(stage.admitted().unwrap().path).unwrap(),
        b"firstsecond"
    );
    assert_eq!(
        stage.admitted().unwrap().extracted_sha256,
        <[u8; 32]>::from(Sha256::digest(b"firstsecond"))
    );
    drop(stage);
    let wrong = artifact(b"firstsecond", "gzip");
    assert_eq!(
        Stage::from_reader(&fixture.0, &wrong, &encoded[..], &budget(&cancel)).err(),
        Some(StageError::Digest)
    );
    assert!(fixture.files().is_empty());
}

#[test]
fn corrupt_crc_size_truncated_members_and_trailing_bytes_refuse_and_cleanup() {
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(false);
    let good = gzip(b"small synthetic payload");
    let mut crc = good.clone();
    let end = crc.len();
    crc[end - 8] ^= 1;
    let mut size = good.clone();
    size[end - 4] ^= 1;
    let mut second = good.clone();
    second.extend_from_slice(&good[..good.len() - 3]);
    let mut garbage = good.clone();
    garbage.extend_from_slice(b"trailing-private-garbage");
    let mut zero = good.clone();
    zero.push(0);
    for bad in [
        crc,
        size,
        good[..end - 1].to_vec(),
        good[..5].to_vec(),
        second,
        garbage,
        zero,
    ] {
        let request = artifact(&bad, "gzip");
        assert_eq!(
            Stage::from_reader(&fixture.0, &request, &bad[..], &budget(&cancel)).err(),
            Some(StageError::Gzip)
        );
        assert!(fixture.files().is_empty());
    }
}

#[test]
fn exact_encoded_limit_and_one_extra_are_streamed_without_whole_body_allocation() {
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(false);
    let request = streaming_artifact(repeated(MAX_ENCODED_BYTES), "none");
    let stage = Stage::from_reader(
        &fixture.0,
        &request,
        repeated(MAX_ENCODED_BYTES),
        &budget(&cancel),
    )
    .unwrap();
    assert_eq!(stage.admitted().unwrap().length, MAX_ENCODED_BYTES);
    assert_eq!(
        digest(File::open(stage.admitted().unwrap().path).unwrap()),
        stage.admitted().unwrap().extracted_sha256
    );
    drop(stage);
    let request = streaming_artifact(repeated(MAX_ENCODED_BYTES + 1), "none");
    assert_eq!(
        Stage::from_reader(
            &fixture.0,
            &request,
            repeated(MAX_ENCODED_BYTES + 1),
            &budget(&cancel)
        )
        .err(),
        Some(StageError::EncodedLimit)
    );
    assert!(fixture.files().is_empty());
}

#[test]
fn exact_decoded_limit_and_one_extra_use_streaming_gzip_generators() {
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(false);
    for count in [MAX_DECODED_BYTES, MAX_DECODED_BYTES + 1] {
        let request =
            streaming_artifact(GzEncoder::new(repeated(count), Compression::fast()), "gzip");
        let result = Stage::from_reader(
            &fixture.0,
            &request,
            GzEncoder::new(repeated(count), Compression::fast()),
            &budget(&cancel),
        );
        if count == MAX_DECODED_BYTES {
            let stage = result.unwrap();
            assert_eq!(stage.admitted().unwrap().length, count);
            assert_eq!(
                stage.admitted().unwrap().extracted_sha256,
                digest(repeated(count))
            );
        } else {
            assert_eq!(result.err(), Some(StageError::DecodedLimit));
        }
        assert!(fixture.files().is_empty());
    }
}

#[test]
fn metadata_bounds_refuse_before_read_or_creation_and_accept_uppercase_hash() {
    struct Never;
    impl Read for Never {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            panic!("invalid metadata read source")
        }
    }
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(false);
    let valid = artifact(b"bytes", "none");
    for kind in [
        "url",
        "emptyurl",
        "version",
        "compression",
        "hash",
        "hashlen",
        "control",
    ] {
        let mut invalid = valid.clone();
        match kind {
            "url" => invalid.url = "a".repeat(4097),
            "emptyurl" => invalid.url.clear(),
            "version" => invalid.version = "a".repeat(129),
            "compression" => invalid.compression = "zip".into(),
            "hash" => invalid.sha256 = "z".repeat(64),
            "hashlen" => invalid.sha256 = "a".repeat(63),
            "control" => invalid.url.push('\n'),
            _ => unreachable!(),
        }
        assert_eq!(
            Stage::from_reader(&fixture.0, &invalid, Never, &budget(&cancel)).err(),
            Some(StageError::Metadata)
        );
        assert!(fixture.files().is_empty());
    }
    let mut uppercase = valid;
    uppercase.sha256.make_ascii_uppercase();
    uppercase.url = "a".repeat(4096);
    uppercase.version = "v".repeat(128);
    assert!(Stage::from_reader(&fixture.0, &uppercase, &b"bytes"[..], &budget(&cancel)).is_ok());
}

#[test]
fn private_root_symlink_and_public_directory_refuse_without_repair() {
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(false);
    let request = artifact(b"bytes", "none");
    fs::set_permissions(&fixture.0, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        Stage::from_reader(&fixture.0, &request, &b"bytes"[..], &budget(&cancel)).err(),
        Some(StageError::Directory)
    );
    assert_eq!(fs::metadata(&fixture.0).unwrap().mode() & 0o7777, 0o755);
    fs::set_permissions(&fixture.0, fs::Permissions::from_mode(0o700)).unwrap();
    let outer = Fixture::new();
    let link = outer.0.join("symlink-root");
    symlink(&fixture.0, &link).unwrap();
    assert_eq!(
        Stage::from_reader(&link, &request, &b"bytes"[..], &budget(&cancel)).err(),
        Some(StageError::Directory)
    );
    assert!(fixture.files().is_empty());
}

#[test]
fn checked_admission_and_drop_never_remove_replacement_inode_or_symlink() {
    let cancel = AtomicBool::new(false);
    for replacement in ["file", "symlink"] {
        let fixture = Fixture::new();
        let stage = Stage::from_reader(
            &fixture.0,
            &artifact(b"bytes", "none"),
            &b"bytes"[..],
            &budget(&cancel),
        )
        .unwrap();
        let path = stage.admitted().unwrap().path.to_owned();
        let old = fixture.0.join("moved-original");
        fs::rename(&path, &old).unwrap();
        if replacement == "file" {
            fs::write(&path, b"keep replacement").unwrap();
        } else {
            symlink(&old, &path).unwrap();
        }
        assert_eq!(stage.admitted().err(), Some(StageError::Identity));
        drop(stage);
        assert!(fs::symlink_metadata(&path).is_ok());
        assert_eq!(fs::read(&old).unwrap(), b"bytes");
    }
}

#[test]
fn changed_contents_or_root_binding_refuse_and_retained_cleanup_is_explicit() {
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(false);
    let stage = Stage::from_reader(
        &fixture.0,
        &artifact(b"bytes", "none"),
        &b"bytes"[..],
        &budget(&cancel),
    )
    .unwrap();
    let path = stage.admitted().unwrap().path.to_owned();
    fs::write(&path, b"changed-longer").unwrap();
    assert_eq!(stage.admitted().err(), Some(StageError::Identity));
    drop(stage);
    assert!(!path.exists());
    let stage = Stage::from_reader(
        &fixture.0,
        &artifact(b"bytes", "none"),
        &b"bytes"[..],
        &budget(&cancel),
    )
    .unwrap();
    let path = stage.admitted().unwrap().path.to_owned();
    let retained = stage.into_retained().unwrap();
    drop(retained);
    assert!(path.exists());
    fs::remove_file(path).unwrap();
    let stage = Stage::from_reader(
        &fixture.0,
        &artifact(b"bytes", "none"),
        &b"bytes"[..],
        &budget(&cancel),
    )
    .unwrap();
    let mut retained = stage.into_retained().unwrap();
    retained.cleanup().unwrap();
    retained.cleanup().unwrap();
    assert_eq!(retained.admitted().err(), Some(StageError::Identity));
    assert!(fixture.files().is_empty());
}

#[test]
fn deadline_cancel_source_error_and_interruption_keep_errors_fixed_and_cleanup_owned() {
    struct Cancel<'a>(&'a AtomicBool);
    impl Read for Cancel<'_> {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            self.0.store(true, Ordering::Relaxed);
            out[0] = 1;
            Ok(1)
        }
    }
    struct Fail;
    impl Read for Fail {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("private-source-url"))
        }
    }
    struct Interrupted(u8);
    impl Read for Interrupted {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            self.0 += 1;
            match self.0 {
                1 => Err(io::ErrorKind::Interrupted.into()),
                2 => {
                    out[0] = 7;
                    Ok(1)
                }
                _ => Ok(0),
            }
        }
    }
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(false);
    let request = artifact(b"", "none");
    let expired = Budget {
        deadline: Instant::now(),
        cancel: &cancel,
    };
    assert_eq!(
        Stage::from_reader(&fixture.0, &request, Cursor::new(b""), &expired).err(),
        Some(StageError::Deadline)
    );
    assert_eq!(
        Stage::from_reader(&fixture.0, &request, Cancel(&cancel), &budget(&cancel)).err(),
        Some(StageError::Cancelled)
    );
    cancel.store(false, Ordering::Relaxed);
    assert_eq!(
        Stage::from_reader(&fixture.0, &request, Fail, &budget(&cancel)).err(),
        Some(StageError::Source)
    );
    assert!(
        Stage::from_reader(
            &fixture.0,
            &artifact(&[7], "none"),
            Interrupted(0),
            &budget(&cancel)
        )
        .is_ok()
    );
    assert!(fixture.files().is_empty());
    for error in [
        StageError::Metadata,
        StageError::Directory,
        StageError::Source,
        StageError::Gzip,
        StageError::Digest,
        StageError::Identity,
        StageError::Storage,
        StageError::Deadline,
        StageError::Cancelled,
    ] {
        assert!(!format!("{error:?} {error}").contains("private-source-url"));
    }
}

#[test]
fn empty_raw_or_gzip_never_admits_an_executable_and_checked_respects_budget() {
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(false);
    for (bytes, compression) in [(Vec::new(), "none"), (gzip(b""), "gzip")] {
        assert_eq!(
            Stage::from_reader(
                &fixture.0,
                &artifact(&bytes, compression),
                &bytes[..],
                &budget(&cancel)
            )
            .err(),
            Some(StageError::Empty)
        );
        assert!(fixture.files().is_empty());
    }
    let stage = Stage::from_reader(
        &fixture.0,
        &artifact(b"bytes", "none"),
        &b"bytes"[..],
        &budget(&cancel),
    )
    .unwrap();
    let expired = Budget {
        deadline: Instant::now(),
        cancel: &cancel,
    };
    assert_eq!(stage.checked(&expired).err(), Some(StageError::Deadline));
    let mut retained = stage.into_retained().unwrap();
    cancel.store(true, Ordering::Relaxed);
    assert_eq!(
        retained.checked(&budget(&cancel)).err(),
        Some(StageError::Cancelled)
    );
    cancel.store(false, Ordering::Relaxed);
    assert!(retained.checked(&budget(&cancel)).is_ok());
    retained.cleanup().unwrap();
}

#[test]
fn stage_root_replacement_and_hardlink_refuse_admission_without_deleting_foreign_file() {
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(false);
    let root = fixture.0.join("root");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let stage = Stage::from_reader(
        &root,
        &artifact(b"bytes", "none"),
        &b"bytes"[..],
        &budget(&cancel),
    )
    .unwrap();
    let name = stage
        .admitted()
        .unwrap()
        .path
        .file_name()
        .unwrap()
        .to_owned();
    let moved = fixture.0.join("moved-root");
    fs::rename(&root, &moved).unwrap();
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(root.join(&name), b"foreign").unwrap();
    assert_eq!(stage.admitted().err(), Some(StageError::Identity));
    drop(stage);
    assert_eq!(fs::read(root.join(&name)).unwrap(), b"foreign");
    assert!(!moved.join(name).exists());
    let stage = Stage::from_reader(
        &root,
        &artifact(b"bytes", "none"),
        &b"bytes"[..],
        &budget(&cancel),
    )
    .unwrap();
    let path = stage.admitted().unwrap().path.to_owned();
    let alias = root.join("hardlink");
    fs::hard_link(&path, &alias).unwrap();
    assert_eq!(stage.admitted().err(), Some(StageError::Identity));
    drop(stage);
    assert_eq!(fs::read(alias).unwrap(), b"bytes");
    assert!(!path.exists());
}

struct PaddedGzip {
    padding: u64,
    tail: Cursor<Vec<u8>>,
    header: Vec<u8>,
    header_offset: usize,
    extra: usize,
    trailer: usize,
    value: usize,
}
impl PaddedGzip {
    fn new(length: u64) -> Self {
        let mut tail = gzip(b"binary");
        // Divide padding into empty members; last member FEXTRA length absorbs
        // the exact remainder, with no encoded-sized fixture allocation.
        let mut padding = length - tail.len() as u64;
        if padding % 65550 < 22 {
            let extra = 22usize;
            let mut enlarged = Vec::new();
            enlarged.extend_from_slice(&tail[..10]);
            enlarged[3] = 4;
            enlarged.extend_from_slice(&(extra as u16).to_le_bytes());
            enlarged.resize(enlarged.len() + extra, 0);
            enlarged.extend_from_slice(&tail[10..]);
            tail = enlarged;
            padding = length - tail.len() as u64;
        }
        Self {
            padding,
            tail: Cursor::new(tail),
            header: Vec::new(),
            header_offset: 0,
            extra: 0,
            trailer: 0,
            value: 0,
        }
    }
}
impl Read for PaddedGzip {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        if self.header_offset < self.header.len() {
            let n = out.len().min(self.header.len() - self.header_offset);
            out[..n].copy_from_slice(&self.header[self.header_offset..self.header_offset + n]);
            self.header_offset += n;
            return Ok(n);
        }
        if self.extra != 0 {
            let n = out.len().min(self.extra);
            out[..n].fill(0);
            self.extra -= n;
            return Ok(n);
        }
        if self.trailer != 0 {
            // Empty raw DEFLATE payload 03 00 followed by zero CRC and ISIZE.
            let tail = [3, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            let n = out.len().min(self.trailer);
            out[..n].copy_from_slice(&tail[self.value..self.value + n]);
            self.trailer -= n;
            self.value += n;
            return Ok(n);
        }
        if self.padding != 0 {
            let size = self.padding.min(65550) as usize;
            let extra = size - 22;
            self.header = vec![
                31,
                139,
                8,
                4,
                0,
                0,
                0,
                0,
                0,
                255,
                extra as u8,
                (extra >> 8) as u8,
            ];
            self.header_offset = 0;
            self.extra = extra;
            self.trailer = 10;
            self.value = 0;
            self.padding -= size as u64;
            return self.read(out);
        }
        self.tail.read(out)
    }
}

#[test]
fn exact_gzip_encoded_limit_and_one_extra_validate_all_empty_members() {
    let fixture = Fixture::new();
    let cancel = AtomicBool::new(false);
    for size in [MAX_ENCODED_BYTES, MAX_ENCODED_BYTES + 1] {
        let request = streaming_artifact(PaddedGzip::new(size), "gzip");
        let result = Stage::from_reader(
            &fixture.0,
            &request,
            PaddedGzip::new(size),
            &budget(&cancel),
        );
        if size == MAX_ENCODED_BYTES {
            let stage = result.unwrap();
            assert_eq!(fs::read(stage.admitted().unwrap().path).unwrap(), b"binary");
        } else {
            assert_eq!(result.err(), Some(StageError::EncodedLimit));
        }
        assert!(fixture.files().is_empty());
    }
}
