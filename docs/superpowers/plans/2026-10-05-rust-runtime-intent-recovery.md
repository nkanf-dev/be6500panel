# Fixed runtime intent and bounded recovery

## Scope

Persist only the fixed sing-box/frpc desired flags in the existing desired-services.json format. Reuse the actual RuntimeHttp/Manager; no extra daemon, thread, generic scheduler or GET-triggered action. Production main activation remains out of scope.

## Implementation and tests

- [ ] Private bounded 4KiB no-follow map; two known Boolean keys only, duplicate/unknown/null/malformed values refused. Missing means both off. File0600/directory0700; measured1MiB headroom, same-directory atomic fsync/rename/dir-fsync. Postrename uncertainty returns the committed in-memory intent truthfully.
- [ ] Explicit stop latches off before persistence and always attempts manager withdrawal/stop even when saving off fails. Recovery remains disabled for that in-process latch. Start/restart save on intent only after operation success; Save/Preview/GET never starts anything.
- [ ] Restoring saved intent is a separate explicit owner call. Constructor/GET zero child actions. Shutdown keeps saved intent, and failed withdrawal retains owned process for retry.
- [ ] Caller-invoked poll uses existing manager exit observation/cleanup and bounded retries (fixed services, max8 attempts,2s exponential delay capped60s). No busy loop and no automatic concurrent check. If cleanup fails, do not restart; preserve journal/child identity. Stop cancels retry immediately. No real device tests.
- [ ] Fake-core tests: existing format import, missing off, duplicate/bad/private/symlink bounds; saved intent survives owner close/reopen, explicit restore only; exited child withdrawn before retry; eight-attempt bound; off persistence failure still cleanup and blocks recovery; no GET/constructor start. Root serial native tests/fmt/clippy and shared-tree ARM build.
