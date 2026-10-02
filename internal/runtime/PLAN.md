# Runtime manager implementation plan

Goal: own only internal/runtime, use the standard library, and build for linux/arm/v7.

1. Define the exported API, service states, limits and a non-blocking mutation gate.
2. Add bounded HTTPS / explicitly enabled loopback HTTP / trusted local artifact acquisition with SHA-256 and atomic activation. Test limits and failed activation.
3. Add private immutable config snapshots plus atomic accepted/last-good state. Test generation conflicts, restart restore and failed checks.
4. Add fixed-argv process-group execution, bounded validation, serial lifecycle changes, bounded retry with jitter, cleanup hook and live RSS. Test cancellation, stale PID protection, children cleanup and old-config survival.
5. Document the integration contract. Run native test, race, vet and linux/arm/v7 compilation. Commit only owned files and report SHA.
