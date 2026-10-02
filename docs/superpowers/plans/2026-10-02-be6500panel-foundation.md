# be6500panel Modular Control Plane Implementation Plan

> **For agentic workers:** Execute the tasks below in isolated Git worktrees, with root integration and verification between tasks. Read the design/API contract first.

**Goal:** Publish a runnable modular router management foundation with professional UI, integrated visualizations and a low-overhead read-only Go backend.

**Architecture:** One static Go process serves JSON/SSE and built browser files. Built-in domain modules register through explicit contracts. React/Effect UI uses separated theme and visualization components; no device deployment in this delivery.

**Tech Stack:** Go standard library, React/TypeScript, Vite, Effect, Motion, Tailwind CSS, Radix/shadcn-style primitives, Apache ECharts, Bun tooling.

---

### Task 1: Backend module and observation foundation
**Owner:** backend worker, isolated backend worktree.
**Files:** go.mod; cmd/be6500panel/main.go; internal/core/*; internal/modules/*; internal/httpapi/*; backend tests.
- [ ] Define explicit module/capability registry and exact API structs from design. Centralize error envelope and unknown API handling.
- [ ] Write failing tests for GET health/modules/system/network, input validation, unsupported apply, authentication and bounded SSE disconnect.
- [ ] Implement host observation via Go os/net/proc reads, with explicitly selected deterministic --demo mode, no shell commands/writes.
- [ ] Implement one shared sampler/bounded broadcast, safe shutdown and low-overhead API. Protect non-loopback binds and mutating-origin checks.
- [ ] Run `go test ./...`, `go test -race ./...`, `go vet ./...`; build `CGO_ENABLED=0 GOOS=linux GOARCH=arm GOARM=7 go build -trimpath -ldflags='-s -w' -o dist/be6500panel-armv7 ./cmd/be6500panel`.
- [ ] Commit only backend-owned files and report exact tests/limitations.

### Task 2: Browser shell and domain pages
**Owner:** frontend worker, isolated frontend worktree.
**Files:** web/package.json; web/bun.lock; web/vite.config.ts; web/tsconfig*; web/index.html; web/src/{app,lib,modules,components/ui}/*; web/src/main.tsx; tests.
- [ ] Pin build tooling compatible with available Node/Bun; configure backend /api dev proxy and stable TS strict build.
- [ ] Implement typed Effect API boundary, module route/navigation/command registration, login state and live read-only status. Keep unknown fields/errors visible.
- [ ] Implement professional dense responsive console, Cmd+K palette, accessible shared primitives and keyboard focus.
- [ ] Implement dashboard/system/network observations, clear unsupported device/Wi-Fi/DNS/firewall pages, proxy plan form/view without false deployment.
- [ ] Integrate `src/theme` and `components/visualizations` public exports (contract below); do not author their files.
- [ ] Run `bun install`, `bun run typecheck`, `bun run test`, `bun run build`; commit frontend-owned files and report.

### Task 3: Theme and professional visualizations
**Owner:** visualization worker, isolated theme worktree.
**Files:** web/src/theme/*; web/src/components/visualizations/*; associated component tests.
- [ ] Export `ThemeProvider`, `useTheme` (`mode`, `resolvedTheme`, `setMode`) and `theme.css` tokens. Mode is `light|dark|system`.
- [ ] Export `TrafficTrend`, `ActivityHeatmap`, `RequestWaterfall`, `LatencyDistribution`, `RuleHitChart`; props `{demo?:boolean}` initially, showing labeled deterministic samples only when demo=true and explicit unavailable otherwise.
- [ ] Implement ECharts lazy tree-shaken wrapper and coherent token palette, ResizeObserver cleanup, reduced motion, tooltip, bounded data, text/table alternatives and usable filters/zoom.
- [ ] Theme/chart rendering tests validate demo labeling/empty-state semantics and theme persistence. Worker uses own local manifest for validation only; no committing web/package.json.
- [ ] Commit theme/visualization-owned files only and report dependency requirements/API.

### Task 4: Root integration, runnable packaging and public publication
**Files:** README.md; LICENSE; .gitignore; .github/workflows/ci.yml; Makefile; scripts/*; docs/*; minimal integration fixes.
- [ ] Merge independently verified backend/theme/frontend branches into main, resolve API seams and align dependencies.
- [ ] Build and test both applications, cross-build ARMv7, run loopback demo API and UI smoke; use browser screenshot if tooling available.
- [ ] Document exact development/run/build commands, auth/bind behavior, truthful capability table, theme/module extension points and deployment boundaries.
- [ ] Verify staged files are new public source only (no device snapshots/credentials/binaries/node_modules), then create public `nkanf-dev/be6500panel` with `gh repo create` and push small coherent commits.
- [ ] Report repository URL, tests, delivered features and unimplemented real-device mutation/telemetry capabilities. Do not deploy router.

## Cross-worker contract

Read docs/superpowers/specs/2026-10-02-be6500panel-design.md. Backend owns API contract and sends seam changes before breaking UI. Frontend imports theme API and chart exports above. Dependencies required by visuals: echarts and React only; main UI owns combined package.json. Everyone keeps private original analysis directory out of new repo, no user subscription/host secrets. No worker connects device. Worktree local commits are allowed; only root pushes public repo.
