# Shared persistent storage admission

Use one `storage.Budget` for traffic, runtime, and native control stores. This is a
measured free-space gate, not a feature quota or an assumption that `/data` has
unlimited capacity. It reserves **1 MiB** of ordinary free space for panel and
native rollback operation. An optional **256 KiB physically written file** adds
emergency scratch space. Only rollback may remove that file.

```go
budget, err := storage.New(storage.Options{
    EmergencyPath: filepath.Join(dataRoot, ".rollback-reserve"),
})
// Report failure, but do not pretend a failed reserve allocation succeeded.
err = budget.Replenish(ctx)
trafficOpts.StorageAdmission = budget.Admit
runtimeOpts.StorageAdmission = budget.Admit
controlOpts.StorageAdmission = budget.Admit
```

The callback is:

```go
type Admission func(context.Context, string, int64, bool) (func(), error)
```

- Path selects the destination filesystem. A missing file uses its nearest
  existing parent. Separate filesystems receive separate reservations.
- Bytes are full new/temporary allocations, not final-size differences. The
  gate rounds to 4 KiB and adds 4 KiB per request. An aggregate request must also
  allow filesystem overhead for each temporary file.
- `false` preserves ordinary recovery headroom. `true` is only for restoring
  existing accepted state and its journal/metadata, never new history or drafts.
- Hold the returned release function through all writes, activation, rollback,
  and temporary-file cleanup. It is safe to call more than once.
- Reserve transaction peak scratch space before its first persistent mutation.
  Do not reserve the same files again inside an already admitted transaction.
- Outstanding bytes stay charged even after writes appear in measured free
  space. This intentionally conservative rule prevents concurrent overspending.
- `errors.Is(err, storage.ErrInsufficientSpace)` identifies an explicit resource
  denial. `ErrMeasurement` means free capacity is unknown. Neither contains a
  private path or document. Context cancellation remains a context error.

`Replenish` is explicit. Call at startup or after confirmed space cleanup, not
while rolling back. Failure leaves the recovery reserve unavailable; report it.
Reserve deletion never implies that history should be erased or retention
shortened. The year-history requirement remains the same. Make room by moving
checked release backups off-device or removing obsolete release packages.

## Router sizing and compatibility

The three complete WAN rings use **3,072,192 bytes** (2.93 MiB). With measured
6 MiB free, they can be admitted together while leaving the ordinary reserve
and the emergency file. This is not a promise that every maximum-sized config
transaction also fits. Runtime candidate/rollback files, current and last-good
configs, native private drafts, JSON escaping, and atomic rewrites all count.
Root wiring can set new runtime payloads to 512 KiB. Existing accepted configs
remain readable/restorable up to the previous 4 MiB ceiling. Native legacy
reader limits must not be reduced just to hide storage pressure.

A new history allocation or a truncated ring tail is admitted as one aggregate
request. A complete preexisting history opens and overwrites its fixed slots
without new admission even below the free reserve. Allocation failure removes
only newly created ring files and returns an explicit resource error. No tier,
retention period, or archive is silently disabled.

External processes can consume space after admission. Real ENOSPC must still
abort the new write and preserve accepted state. Portable tests inject measured
capacity and emergency-file writes; traffic/runtime tests inject write errors
without filling the host disk.
