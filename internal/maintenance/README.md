# Scoped configuration backup and import

`maintenance.Service` accepts only six native UCI document names: `network`,
`wireless`, `dhcp`, `firewall`, `system`, `dropbear`. `runtime.frpc` and
`runtime.sing-box` are optional, explicit accepted-runtime backup scopes.

The JSON envelope contains `model`, `build`, `createdAt`, native `generation`,
`scopes`, and `documents`. Each document has exact original `content` and a
lowercase SHA256 `digest`. Runtime documents also have their own accepted
`generation`. There is no separate schema/protocol version. The current project
format is the contract. Unknown vendor text and comments are preserved.

The complete encoded backup is limited to 2 MiB. Native text is limited to
128 KiB per document. Accepted runtime settings are limited to 512 KiB each,
matching the router runtime owner's configured limit. JSON escaping counts
against the aggregate limit; export smaller groups if required. Duplicate keys,
case-variant fields, extra fields, unknown scopes, missing or duplicate documents,
nulls, digest mismatch and oversized text are rejected before any staging.

## Root integration

Create one long-lived service, not a new store per request:

```go
maintenance.New(maintenance.Options{
    Native: controlManager,
    Metadata: func(ctx context.Context) (maintenance.Metadata, error) {
        // Get the actual router model and current panel/firmware build.
        return maintenance.Metadata{Model: model, Build: build}, nil
    },
    Runtime: func(ctx context.Context, service string) (maintenance.RuntimeDocument, error) {
        raw, generation, err := runtimeManager.Config(service)
        return maintenance.RuntimeDocument{Content: string(raw), Generation: generation}, err
    },
})
```

Authenticated, same-origin routes must call these exported helpers:

- `POST /api/maintenance/backup`: `HandleMaintenanceBackup`, body `{scopes}`.
  Returns private JSON as an attachment, `Cache-Control: no-store`.
- `POST /api/maintenance/import/preview`: `HandleMaintenancePreview`, original
  UTF-8 JSON file as the complete body, not a parsed/reserialized browser object.
  Returns native whole-document exact diffs, counts, diagnostics, risks, dependency
  checks and the current native generation. Runtime byte/digest comparison is
  preview only. Unavailable accepted runtime data is `uncompared`, not empty.
- `DELETE /api/maintenance/import/preview?id=...`: discard one memory-only preview.
- `POST /api/maintenance/import/stage`: `HandleMaintenanceStage`, body
  `{previewId,generation,modules,acknowledgeModelMismatch}`. Model mismatch requires
  explicit acknowledgment. Only changed selected native documents are staged.

The helpers rely on root auth/origin wiring. They expose no password, key file,
rescue, filesystem path, restore command, service start or Apply endpoint.
`dropbear` is the existing native configuration document only. `/data/ssh`,
password/shadow/key files and the independent rescue channel are never scopes.
Backup documents can still contain Wi-Fi credentials and runtime tokens. Warn
before manual external download. Do not save backup text to browser persistence,
logs or automatic router archives.

## Preview and staging boundaries

`control.PreviewDocuments` is pure. It checks syntax, native fields, execution-hook
changes and references against the selected native document set. It creates no
files, IDs or native commands. `Stage` then calls the existing private control
manager, which owns authoritative native UCI checks, platform safeguards and
shared storage admission. A selected dependency must be present in the bundle.
Native commands still verify each new draft against private temporary documents.

The returned draft IDs form one selected commit bundle. No live configuration is
written. The user must open the existing configuration queue, review risks and
explicitly Apply those drafts together. The control manager retains the current
risk acknowledgment and provisional confirmation/restore workflow.

Sequential Stage failures remove only IDs created by this call using a separate
bounded cleanup context. A cleanup failure returns `import_cleanup_failed`, the
original safe `causeCode` and top-level `retainedDraftIds`. A timed-out cleanup also
returns `cleanupPending: true`: these IDs are not confirmed removed, and an
already in-flight private deletion may still finish. Refresh the queue before
retrying. Never mask cleanup failure or silently retry uncertain Stage.

Preview storage is memory-only: at most four previews, 4 MiB total content and
10-minute expiry. Discard releases a preview immediately. `Clear()` can be called
on auth reset or shutdown. There is no flash archive or import history.

Runtime native validation and restore are not implemented by this native Stage
step. Runtime scopes are marked not stageable and validation deferred. A dedicated
service restore must use accepted runtime generation CAS and explicit user review;
it must not infer Start from an imported document. Existing service editors remain
the next recovery step, not a generic read-only product limit.

## Frontend

Mount `MaintenanceBackupPanel` from `web/src/components/maintenance`. Pass
`onOpenConfiguration` to open the actual existing draft queue. Root must dispatch
`be6500panel:logout` (or unmount authenticated UI) on logout; the surface also
clears its private state on the existing unauthorized event. Root integration must
verify auth, same-origin checks and the real queue-navigation callback.

Tests contain fake credentials only and exercise the native manager through its
project environment with an injected fixed UCI runner; no router access occurs.
