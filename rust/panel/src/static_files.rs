//! One trusted asset root; files are streamed, never embedded or tree-copied.
use crate::http::decode_target;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub const TRANSFER_BUFFER_BYTES: usize = 8 * 1024;

pub struct StaticFiles {
    root: PathBuf,
}

pub struct Asset {
    file: File,
    pub length: u64,
    pub content_type: &'static str,
}

fn not_found() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "asset unavailable")
}

impl StaticFiles {
    pub fn new(root: &Path) -> io::Result<Self> {
        let root = root.canonicalize()?;
        if !root.is_dir() {
            return Err(not_found());
        }
        Ok(Self { root })
    }

    /// The configured tree must be trusted and not concurrently modified.
    /// Reject all symlink components, including links that stay inside the root.
    pub fn open(&self, path: &str) -> io::Result<Asset> {
        let decoded = decode_target(path).map_err(|_| not_found())?;
        let relative = if decoded == "/" {
            "index.html"
        } else {
            decoded.strip_prefix('/').ok_or_else(not_found)?
        };
        let mut file_path = self.root.clone();
        for component in relative.split('/') {
            if component.is_empty() || matches!(component, "." | "..") {
                return Err(not_found());
            }
            file_path.push(component);
            let metadata = fs::symlink_metadata(&file_path)?;
            if metadata.file_type().is_symlink() {
                return Err(not_found());
            }
        }
        // Defense in depth for an accidentally changed root; not an atomic
        // sandbox against another process mutating a trusted asset tree.
        let canonical = file_path.canonicalize()?;
        if !canonical.starts_with(&self.root) {
            return Err(not_found());
        }
        if !fs::metadata(&canonical)?.is_file() {
            return Err(not_found());
        }
        let file = File::open(&canonical)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(not_found());
        }
        let content_type = match canonical.extension().and_then(|ext| ext.to_str()) {
            Some("html") => "text/html; charset=utf-8",
            Some("js") => "text/javascript; charset=utf-8",
            Some("css") => "text/css; charset=utf-8",
            Some("json") => "application/json",
            Some("svg") => "image/svg+xml",
            Some("png") => "image/png",
            Some("ico") => "image/x-icon",
            Some("woff2") => "font/woff2",
            _ => "application/octet-stream",
        };
        Ok(Asset {
            file,
            length: metadata.len(),
            content_type,
        })
    }
}

impl Asset {
    pub fn stream(&mut self, writer: &mut impl Write) -> io::Result<()> {
        let mut buffer = [0_u8; TRANSFER_BUFFER_BYTES];
        let mut remaining = self.length;
        while remaining > 0 {
            let wanted = remaining.min(buffer.len() as u64) as usize;
            let count = self.file.read(&mut buffer[..wanted])?;
            if count == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "asset changed",
                ));
            }
            writer.write_all(&buffer[..count])?;
            remaining -= count as u64;
        }
        Ok(())
    }
}
