# Native router configuration control

`control` is the private draft and transaction owner for `network`, `wireless`,
`dhcp`, `firewall`, `system`, and `dropbear`. It uses only the Go standard library.
It does not own HTTP, authentication, panel listen-address changes, or UI.

## API

```go
manager, err := control.New(control.Options{
    Root: "", // actual /; tests supply a temporary root
    DataDir: "/data/be6500panel/control", // dedicated private directory
    Logger: logger,
    // Optional injected seams; production defaults execute fixed argv.
    Runner: func(ctx context.Context, executable string, args ...string) ([]byte, error) { ... },
    Reload: func(ctx context.Context, module string) error { ... },
    Verify: func(ctx context.Context, changedModules []string) error { ... },
    ConfirmationTimeout: 120 * time.Second,
})
defer manager.Close()
```

- `Documents(ctx) (DocumentSet, error)` reads private native documents and the
  current `uint64` source generation. Reading an external edit advances CAS.
- `Stage(ctx, StageRequest{Module, Content, Generation}) (Draft, error)` persists
  a private candidate with full-document unified diff, risk codes and validation
  results. Invalid candidates remain inspectable drafts, but cannot be committed.
- `Drafts(ctx) ([]Draft, error)` lists staged metadata, diffs and diagnostics.
- `DeleteDraft(ctx, id) error` deletes exactly one private draft.
- `Commit(ctx, CommitRequest{DraftIDs, Generation, AcknowledgeRisks}) (Operation, error)`
  validates again, journals prior documents, atomically replaces each changed
  document and reloads fixed services. Select exactly one draft per module.
- `Confirm(ctx, id) (Operation, error)` accepts a provisional transaction. The
  HTTP owner must call it **only after receiving an authenticated real request
  through the new/current management address**. A local call alone is not proof
  of browser reachability. An optional Verify hook runs again at confirmation.
- `Rollback(ctx, id) (Operation, error)` restores the retained prior snapshot.
  An old committed operation cannot overwrite newer generations. It can retry
  failed recovery even while regular mutations are disabled.
- `Status() Status` reports enabled state, source generation, pending deadline
  and a recovery error code.
- `Close() error` cancels subprocess groups and stops the deadline worker.
  It leaves a pending journal for safe restart recovery, not implicit acceptance.

Request/result types have the JSON fields in the production API contract.
`*control.Error` has stable `Code` and credential-free `Message` fields. On apply
failure, Commit returns the rollback Operation **and** the original typed error;
if recovery fails, its error is `rollback_failed` and mutations stay disabled.
The HTTP owner should return both result and error so recovery state is visible.

## Commit and recovery

Drafting never writes `/etc/config`, changes a UCI savedir, or reloads a service.
Native validation runs `/sbin/uci -s -c PRIVATE_DIR -P PRIVATE_DIR show MODULE`.
Its output is discarded; diagnostics never include raw config or command output.
Only native `config`, `option`, and `list` statements are accepted. Quoted UCI
strings, escaped quotes, lists and anonymous sections are supported. Validation
checks IP/CIDR/netmask, MAC, ports, booleans, hostnames, primary Wi-Fi fields and
cross-document interface/radio references. Expert unknown native fields remain
available. Firewall include sections are factory-owned: transactions must
preserve their complete parsed names, fields, values and duplicate counts.
Formatting and section/option reordering are allowed, but adding, changing or
removing includes is rejected. New unregistered network protocols are rejected;
unchanged vendor protocols remain supported.

A process lock and one manager mutation gate serialize transactions. SHA-256 of
bounded live documents detects edits outside this manager. The generation is
only a CAS counter, not a schema/protocol version. One private journal stores the
last operation and prior documents. All changed files are replaced by atomic
rename, with file and directory fsync. POSIX cannot atomically rename multiple
files; the durable journal restores **all** of them after partial failure or
interruption. Reload/Verify failures restore prior documents and reload every
changed module. Built-in apply verification checks installed text after reload.
Rollback also checks every prior document's exact text, existence and original
permissions after reloading. A rewrite, recreation, missing document or changed
mode leaves recovery failed and the journal in `rolling_back`; it is never
accepted as `rolled_back`. The supervisor retries until exact recovery succeeds.
On restart, any applying, pending or rolling-back journal is restored before New
returns a ready manager, even if its confirmation deadline has not elapsed.

Network changes, LAN address/mask, WAN replacement, management service changes,
broad firewall policies, primary Wi-Fi credential/radio changes and disabling all
Wi-Fi require explicit risk acknowledgment. Such commits remain provisional for
120 seconds by default. The server-provided deadline is authoritative. Timeout
restores prior documents and fixed services. Failed rollback disables mutations
and reports `rollback_failed`. The deadline supervisor remains alive and retries
recovery after 1 second, then doubles the delay up to a 30-second maximum. Each
attempt is limited to 90 seconds. Wake notifications do not bypass retry pacing.
Manual Rollback can retry immediately; successful recovery clears the error and
keeps later provisional commits protected. Close cancels retry work and retains
the journal for restart recovery. No new commit may run while pending.
Known guest interface changes need not require primary-Wi-Fi confirmation.

Firewall risks cover zone input/forward/output policies and network/device
membership. Active local INPUT DROP/REJECT rules require acknowledgment even
when both source and destination IP filters are present. Changes to active
local INPUT ACCEPT exceptions also require acknowledgment because deleting or
narrowing an exception can remove management access. The native document does
not identify the browser's route or panel bind address, so this classification
is conservative and does not assume WAN input is safe. These fields remain
editable with provisional rollback; they are not prohibited. Disabled rules,
constrained forwarding rules with explicit destination zones, and unrelated
expert fields remain available without management-risk acknowledgment.

Fixed reload argv:

| Module | Executable and args |
| --- | --- |
| network | `/etc/init.d/network reload` |
| wireless | `/sbin/wifi reload` |
| dhcp | `/etc/init.d/dnsmasq reload` |
| firewall | `/etc/init.d/firewall reload` |
| system | `/etc/init.d/system reload` |
| dropbear | `/etc/init.d/dropbear reload` |

Before system writes, `/sbin/uci -q get xiaoqiang.common.INITTED` must return `YES`
to avoid the factory script's uninitialized password-reset side effect. Injected
Reload hooks own equivalent platform guards. A Verify hook can check concrete
service state; browser reachability remains the authenticated Confirm boundary.
The raw UCI namespace has no panel-specific port/auth settings. HTTP/main own
those settings and must use their own safe transaction integration.

## Limits and privacy

Documents are limited to 128 KiB each, at most 12 drafts and 512 KiB aggregate
candidate text. Only one rollback snapshot/journal is retained. Files use 0600,
data/candidate directories use 0700; original live file modes are restored on
rollback. The disk store is bounded to 3 MiB per state/journal file. Config,
secrets and diffs belong only in authenticated API responses and private storage.
Logs contain only operation ID, state code and generation. No fixtures are copied
from a live router. Do not log Runner output or HTTP bodies.

Tests use temporary synthetic root documents and injected exec/reload hooks.
They never connect to a router. Run `go test ./internal/control`,
`go test -race ./internal/control`, and `go vet ./internal/control`.
