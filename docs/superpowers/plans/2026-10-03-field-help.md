# Firmware field help implementation plan

> **For agentic workers:** Execute the catalog and UI tasks in this dedicated worktree. Report source gaps rather than invent behavior.

**Goal:** Give every existing field a specific, source-backed explanation without changing input semantics.

**Architecture:** Separate structured metadata by firmware module. Join metadata with the existing field schema for compact labels and expandable detailed help. Each evidence entry records a source file and line, firmware context, and the exact fact it supports.

**Tech Stack:** TypeScript, React, Vitest, static Xiaomi RN02 firmware 1.0.43 shell/Lua sources.

---

- [x] Research all network fields into `web/src/components/configuration/field-help/network.ts`.
- [x] Research all wireless fields into `web/src/components/configuration/field-help/wireless.ts`.
- [x] Research all DHCP fields into `web/src/components/configuration/field-help/dhcp.ts`.
- [x] Research all firewall fields into `web/src/components/configuration/field-help/firewall.ts`.
- [x] Research all system and Dropbear fields into their module catalogs.
- [x] Add typed metadata and inventory APIs. Add tests that iterate the full catalog and check evidence paths/lines.
- [x] Add expandable field help in `NativeFields.tsx`; keep widgets, bounds, unknown values and credential masking unchanged.
- [x] Document baseline evidence, supported facts, search gaps and firmware context in `docs/field-help-evidence.md`.
- [x] Run focused tests, typecheck, then the full unit suite once. Review and commit the metadata/UI changes.

## Review decisions

- Keep all 395 canonical fields and all input semantics. No extra vendor fields are added in this batch.
- Use function-first descriptions and only source-confirmed examples. Do not recommend unsupported HE/EHT widths or claim one generic band/channel range.
- Keep original Dropbear gates separate from independently managed rescue SSH. Do not hard-code a rescue port from an unverified product context.
- Correct explicit `queryport=0` to the traced shared-socket mode. Do not equate it with an omitted option.
- Evidence hashes and line checks confirm integrity, not every semantic assertion. Independent reviewers sample actual parser records and consumer branches.

- Copy review applied only where facts remain precise: HT20 width example, 12h lease example, neutral root authentication behavior. Rejected blanket recommended HE/EHT widths, rescue-port assumptions and unsupported channel recommendations.

## Validation checkpoint

- Final focused tests: 515 passed (403 field-evidence tests, 4 help UI tests, 108 existing schema tests).
- Final project typecheck: passed.
- Mounted-source verification: 84 referenced source files/artifacts checked by SHA-256, bytes, line counts and cited offset bounds.
- The full unit suite is started once after all module catalogs and evidence sources are stable.

- Full unit suite: 48 files, 996 tests passed. This suite was run once after the final source updates.
- Scoped Prettier check and `git diff --check`: passed.
