# Shared flash admission plan

Goal: protect native and managed configuration recovery while keeping the fixed 400-day WAN history available on a small shared flash volume.

1. Add internal/storage with filesystem measurement, one shared admission gate, 1 MiB free reserve and an optional physically allocated 256 KiB emergency reserve reclaimed only for rollback. Track concurrent outstanding allocations. Inject measurement and emergency-file I/O for portable tests.
2. Preflight aggregate new traffic ring/tail growth, hold admission through opening all rings, and remove only newly created rings on failure. Preserve complete existing rings at low free space. Return explicit resource errors.
3. Admit runtime candidate and metadata writes before file creation/rename. Provide transaction and rollback helpers to the runtime manager owner. Bound temporary growth and preserve ready last-good snapshots.
4. Test measured low space, 6 MiB admission, rollback at zero free, concurrent requests, canceled work, ENOSPC cleanup, measurement errors, and reopening preexisting rings at low free.
5. Run package tests and race tests, commit only owned files, and send exact wiring APIs to the root and integration owners.
