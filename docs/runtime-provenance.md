# Runtime artifact provenance

## sing-box 1.14.2 minimal ARMv7

Unmodified upstream source: https://github.com/SagerNet/sing-box/tree/v1.14.2
License: GPL-3.0-or-later; upstream additional terms are preserved in SING-BOX-LICENSE.txt.
Compiler: Go1.26.2. Build from the v1.14.2 source directory:

```text
CGO_ENABLED=0 GOOS=linux GOARCH=arm GOARM=7 go build -trimpath   -tags with_utls,badlinkname,tfogo_checklinkname0   -ldflags "-X runtime.godebugDefault=multipathtcp=0,tlssha1=1 -checklinkname=0 -X github.com/sagernet/sing-box/constant.Version=1.14.2 -s -w -buildid="   -o sing-box ./cmd/sing-box
gzip -9 -n -c sing-box > sing-box-1.14.2-linux-armv7-minimal.gz
```

No registry/source modifications. CompressedSHA256 c6b303c1663757b516f6b370c7b8486031782b6ab8e81015bf300485897ceae2, ELF SHA256 bae123c029d40ff4c01fbb363fc970ccb604cebdea47f350c08bab31b4a060c3.

## FRPC 0.71.0 linux_arm

Unmodified official binary from https://github.com/fatedier/frp/releases/tag/v0.71.0
Source commit4a23aa181c1d7e28eecaa8216024ed753b9d27c8, Apache-2.0, FRPC-LICENSE.txt.
Official archivechecksum f40a984f83e8d34a9241b0be4a9d5fbcfe513a4a5c022b84a02637ff6d36833b.
Only frpc, not frps. RawSHA256 d92fcc4e62232bae6d58030d2a73f3d2caf49e19742800c9a861a621c2ad1d1a; gzipSHA256 d0a1cb1d1b05c168ada92600f4bc4a7328aac410592e07e4e6c0afc63aedcc89.

## Rule data

CN domain/IP SRS data remains fetched from the maintained source with upstream licensing/attribution. Not included in this runtime asset set. Panel source uses MIT; these external executables retain their own licenses.
