# Foundation verification

Date: 2026-10-02. Source-only delivery; no router deployment in this milestone.

## Backend

- `go test ./...`: pass.
- `go test -race ./...`: pass.
- `go vet ./...`: pass.
- Static `CGO_ENABLED=0 GOOS=linux GOARCH=arm GOARM=7` build: 6,291,618 bytes, ELF32 ARM EABI5.
- Native loopback runtime: system/network/modules/logs APIs; password sessions; SSE disconnect/expiry/observation failure; invalid/oversized JSON; plan validation; unsupported apply.
- Production static service: HTML and JS/CSS assets200, APIs200, unknown API JSON404 and client-route fallback200.

## Browser

- Strict TypeScript: pass.
- Vitest: 9 files, 47 tests pass, including stream-observation failure and session-expiry recovery.
- Production Vite build: pass. Pages and ECharts runtime are lazy chunks.
- Real Chromium integration: 4 tests pass. Overview observations and canvas; Cmd+K; dark/light theme; network table; real proxy/FRPC planning POST; mobile navigation with no page overflow; log filtering by code.
- Chart-specific Chromium smoke: five canvases, filters, reduced motion, table alternatives, mobile width and no page errors.

## Outputs

Screenshots are local test artifacts under `web/test-results`; they are not committed. Browser rendering is verified by assertions. No subjective screenshot review is claimed for the current text-only model.

`system` observes Linux proc metrics; network observes host interfaces, including in demo mode. Charts show explicitly labeled fixed samples in demo and empty collector state otherwise. Proxy and FRPC plan validation is live API functionality; process execution and router mutation remain next-stage work.

## Continuous integration

GitHub Actions run37013592261 passed on Ubuntu, including Go tests/race/vet, browser tests, production assets and ARMv7 cross-build. The final recovery patch also passed the complete local suite.
