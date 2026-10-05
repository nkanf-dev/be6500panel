#!/usr/bin/env python3
"""Package public Release assets from an already built ARM manager and web tree."""
import argparse
import gzip
import hashlib
import os
from pathlib import Path
import tarfile

ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = (
    "install.sh", "install.ps1", "enable-ssh.sh", "enable-ssh.ps1",
    "install-panel.sh", "install-panel.ps1", "router-install.sh",
    "router-rescue-setup.sh", "rescue-bootstrap.sh", "rescue-ssh.init",
)

def write_archive(destination, entries):
    epoch = int(os.environ.get("SOURCE_DATE_EPOCH", "0"))
    with destination.open("wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=epoch, compresslevel=9) as zipped:
            with tarfile.open(fileobj=zipped, mode="w", format=tarfile.USTAR_FORMAT) as archive:
                for source, name in entries:
                    if not source.is_file() or source.is_symlink():
                        raise ValueError(f"Not a regular release file: {source}")
                    info = archive.gettarinfo(str(source), arcname=name)
                    info.uid = info.gid = 0
                    info.uname = info.gname = "root"
                    info.mtime = epoch
                    info.mode = 0o755 if name == "be6500-panel" or name.endswith((".sh", ".init")) else 0o644
                    with source.open("rb") as content:
                        archive.addfile(info, content)

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--web", type=Path, default=ROOT / "web/dist")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if not (args.web / "index.html").is_file():
        parser.error("Build web/dist before packaging")
    core = [(args.binary, "be6500-panel"), (ROOT / "scripts/bootstrap.sh", "bootstrap.sh"),
            (ROOT / "scripts/router-native.lua", "router-native.lua")]
    core.extend((p, "web/" + p.relative_to(args.web).as_posix()) for p in sorted(args.web.rglob("*")) if p.is_file())
    tools = [(ROOT / "scripts" / name, name) for name in SCRIPTS]
    tools.extend([(ROOT / "third_party/xiaomi-ssh/LICENSE", "third_party/xiaomi-ssh/LICENSE"),
                  (ROOT / "docs/install.md", "INSTALL.md")])
    # Validate all inputs first. Never package configuration, keys or native cores.
    for path, _ in core + tools:
        if not path.is_file() or path.is_symlink():
            parser.error(f"Missing public release input: {path}")
    args.out.mkdir(parents=True, exist_ok=True)
    assets = [("be6500panel-armv7.tar.gz", core), ("be6500panel-installers.tar.gz", tools)]
    hashes = []
    for name, entries in assets:
        destination = args.out / name
        write_archive(destination, entries)
        hashes.append(f"{hashlib.sha256(destination.read_bytes()).hexdigest()}  {name}")
        print(f"{name}: {destination.stat().st_size} bytes")
    (args.out / "SHA256SUMS").write_text("\n".join(hashes) + "\n", encoding="ascii")

if __name__ == "__main__":
    main()
