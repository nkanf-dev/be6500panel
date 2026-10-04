# Acquisition Absolute Deadline Implementation Plan

**Goal:** Keep one acquisition deadline/cancel flag through config reads, executable hashing, checker and irreversible manager boundaries. Never expire safety cleanup/retained-owner recovery authority.

**Architecture:** Fixed optional operation budget in existing Manager/Store, scoped by one internal acquire_verified_stage method, cleared on return. Existing unbounded public operations retain previous semantics; no generic scheduler/thread. Native process worker adds start_until/verify_until through existing supervisor. Root store checks per bounded read/write chunk and before manifest rename; postrename is still authoritative and finishes durability bookkeeping. Expired acquisition before old withdrawal leaves Run/state unchanged; expiry after withdrawal invokes bounded explicit old-proven recovery under independent original recovery budgets.

- [x] Worker source-only ProcessOwner additive absolute start/check APIs, hashchunk/root pre/postIO/spawn checks, checkerdeadline min outer and independent abort/reap.
- [x] Root RuntimeStore optional budget guard and read_config_until/hashchunks; precommitdeadline rejects, postcommit outcome remainsauthoritative. Test-only slowread/pre-rename expiryfixtures; no publicfaultknobs.
- [x] Root Manager scoped acquire_verified_stage fixed budget; guard before status/stage/check/withdraw/bind/start/readiness/metadata. Cleanupalwaysindependent; rollback abort restoresmetadata under independent fixed recoverybudget, not skipped by acquiredexpireddeadline.
- [x] HTTP90sabsolute operation captured before fetch; fetchdeadline min cap/childhookreserve/remaining; typed Stage handed with originaldeadline. Nestedartifact maponly visitor and zeroIO arrays regression.
- [x] Fullserial root tests/fmt/clippy +ARM; capture evidence resourcegate InsufficientSpace event remains refusal, thresholdsunchanged. Not hardreal-time filesystem syscall promise: expiry checked on return prevents later destructive steps.

## Result

Included in authenticatedacquisition384test serialized qualification and ARMv7crossbuild. Cutoff checks before irreversible steps; postrenameresults authoritative; safetycleanup and provenoldrollback remain independent.
