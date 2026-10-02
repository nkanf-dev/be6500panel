# Managed external runtimes

This package is standard-library only. It targets Linux ARMv7 (`GOOS=linux GOARCH=arm GOARM=7`, `CGO_ENABLED=0`). Import it as `managedruntime "be6500panel/internal/runtime"` when the Go `runtime` package is also needed.

## API

```go
New(Options) (*Manager, error)
(*Manager).Status(service string) (Status, error)
(*Manager).Config(service string) (raw []byte, generation uint64, err error)
(*Manager).Acquire(ctx context.Context, service string, artifact Artifact) (Status, error)
(*Manager).Configure(ctx context.Context, service string, raw []byte, expectedGeneration uint64) (Status, error)
(*Manager).Restore(ctx context.Context, service string, expectedGeneration uint64) (Status, error)
(*Manager).Start(ctx context.Context, service string) (Status, error)
(*Manager).Stop(ctx context.Context, service string) (Status, error)
(*Manager).Close() error
```

Only `sing-box` (`SingBox`) and `frpc` (`FRPC`) are accepted. All mutations, including exit supervision, share one operation lane. An overlapping user mutation returns `ErrBusy`; it does not queue. `Status` and `Config` may be read during a download or verifier. A stale generation returns `ErrGeneration`. `Status` remains readable after Close; other operations reject closed managers.

`Config` is the private accepted config body, copied from a bounded 0600 file. The root API must authenticate and authorize access. Never forward this body into public views or logs. There is no generic redaction or content scanning. `Status`, manager errors and the supplied logger never include config bodies or core stdout/stderr. Core output is retained only in a private bounded 4 KiB RAM suffix; it is not exported, since even core verification errors can echo credentials.

## Setup and storage

`Options{DataDir, RunDir, Logger *slog.Logger}` is sufficient. Choose separate, private, dedicated directories. Do not put RunDir below DataDir or alias it through symlinks. The intended RunDir is RAM/tmpfs. The manager checks Linux filesystem free capacity before downloading. `MinFreeRunBytes` adds a reserve above the maximum extraction size. The root chooses this reserve based on measured router RSS; this package cannot guarantee process RSS limits or total router memory availability.

Defaults: compressed download 16 MiB; extraction 40 MiB; each config 1 MiB; tail 4 KiB; download timeout 2 minutes; verification timeout 10 seconds; TERM grace 2 seconds; exponential backoff 1–30 seconds with downward jitter; 5 automatic retries; stable running period 1 minute resets the crash budget. Configurable maxima are bounded in `New`.

The data store holds only small metadata and at most the current and previous accepted private config snapshots after a durable commit. Its directories are 0700, files 0600. A data-directory lock prevents multiple controllers. Config acceptance writes/syncs a private candidate, checks it, renames it to an immutable generation snapshot, syncs that directory, then atomically writes/syncs the manifest. A failed check leaves the accepted body, generation and old core untouched. Candidate bytes are checked again after verification to reject accidental verifier writes. Start and artifact-replacement verification check a private copy rather than the accepted file. `Restore` verifies the previous accepted body and accepts it as a new generation; generations never move backwards.

`ErrDurability` is different from a rejected commit. The rename already succeeded, but post-rename directory fsync failed. The returned generation and `Config` body are authoritative. Previous-manifest snapshots are retained, and the prior running core is not replaced. Inspect storage and explicitly Stop/Start to apply the accepted body. Do not repeat the original expected generation blindly. A later durable commit can prune old snapshots.

`New` restores private config/generation, but never blindly trusts a binary found in RunDir and never automatically starts cores. An accepted config without a loaded artifact has state `rebuilding` and recovery action `acquire_verified_artifact`. Root boot code must Acquire from a fixed source/checksum, then Start. RunDir executables are removed on Close, after all processes reap. Artifact source metadata is private, never returned by Status.

## Artifacts

`Artifact{URL, SHA256, Compression, Version}` accepts only `none` or `gzip`. SHA256 is exactly 64 hexadecimal characters and hashes the fetched bytes **before** decompression. Downloads and gzip extraction stream to a unique private RunDir file; neither full core nor full compressed data is read into RAM. Both byte limits, gzip trailers and checksum are checked before activation. There is no tar extraction or arbitrary destination path.

HTTPS is the normal source. For controlled local experiments only, set `AllowLoopbackHTTP: true`; HTTP then requires a numeric loopback host, not a hostname. Every redirect must satisfy the source policy. `LocalSourceRoot` enables `file://` sources whose canonical regular-file path stays under that trusted root. The root must be operator-controlled, not an attacker-writable directory. URL credentials/fragments and URLs above 4096 bytes are rejected. `HTTPClient` can supply a trusted TLS/transport policy; the client is cloned rather than modified.

If a config already exists, a new artifact must verify that config before it replaces the active binary. Failed downloads/verifiers preserve the previous binary and running core. Runtime acquisition executes only the supplied checksum-verified artifact when checking an existing config; callers must supply trusted binaries/checksums. Tests execute only scripts created by the test suite, never remote binaries.

## Fixed subprocess contract

| Service | Verify | Run |
|---|---|---|
| sing-box | `check -c <private candidate.json>` | `run -c <accepted config.json>` |
| frpc | `verify -c <private candidate.toml>` | `-c <accepted config.toml>` |

FRPC also accepts valid JSON; a valid JSON body receives `.json`, otherwise `.toml`. It does not enable legacy INI/YAML. There are no shell command strings or user-controlled argv. The subprocess environment is minimal, not the panel environment. Working directory/TMPDIR/HOME are RunDir. Relative core resources therefore resolve under RunDir; root compiler/deployment should use explicit private resource paths when needed.

Each verifier/core has its own process group. Checks honor context and timeout. Stop/Close send TERM then KILL after finite grace, reap the leader, and kill same-group descendants before releasing the unreaped leader identity. No PID from API/persistent metadata is used for signaling. Linux waitid(WNOWAIT) preserves the PID/group until cleanup. A failed identity-retention syscall kills/reaps the managed group rather than hanging forever. Detached children which deliberately leave the group are outside this contract.

Linux parent-death SIGKILL also terminates the direct managed leader if the panel crashes/is killed. Its creator OS thread is locked for the child's lifetime, so retirement of a transient API-handler thread cannot kill a healthy core. Linux does not inherit that signal into forked descendants. A hard panel death therefore needs root init/cgroup supervision for arbitrary forking cores; normal Stop/Close and managed leader exit clean the full owned group. The manager does not claim recovery of foreign PIDs. RSS is actual Linux `/proc/<pid>/statm`; non-Linux development status reports `rssAvailable: false`, not invented data.

## Cleanup hook

Supply `Options.CleanupHook func(context.Context, string) error` to remove root-owned network resources on Stop, Close and unexpected core exit. No firewall/routing mutation occurs in this package. The hook runs on the serialized mutation lane, receives a bounded context, must honor that context, must remove only its owned resources, and must not call a manager mutation (that would return ErrBusy). Hook failure prevents replacement/restart and returns `cleanup_failed`; explicit Start retries the cleanup first. Required cleanup is tracked separately from later operation diagnostics, so editing/reacquiring cannot erase it. The first Start after New also invokes this hook to clear caller-owned boot leftovers. Close invokes it for both fixed services to support boot/partial-apply resource rollback.

## Verification

```sh
go test ./internal/runtime
go test -race ./internal/runtime
go vet ./internal/runtime
CGO_ENABLED=0 GOOS=linux GOARCH=arm GOARM=7 go test -c -o /tmp/be6500panel-runtime-armv7.test ./internal/runtime
```

Fixture tests cover checks preserving old core/config; generation/restore/reopen; private file modes and format-aware FRPC verifier paths; bounded artifact streams/cancellation/path/redirect policy; commit I/O failure and post-commit fsync warning; crash backoff budget, cancellation and late-supervisor races; TERM/KILL, group descendants, unrelated PID safety, cleanup hook failure/retry, identity-retention failure, and bounded core-output tails.
