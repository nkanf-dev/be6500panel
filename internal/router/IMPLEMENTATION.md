# RN02 Read-only Router Adapter Implementation Plan

> **For agentic workers:** Execute this bounded package plan with synthetic fixtures and review checkpoints.

**Goal:** Return real RN02 read-only observations with partial module errors, cached samples, and no wireless credentials.

**Architecture:** The adapter reads bounded rooted files, uses fixed timed firewall commands only for the live root, and caches slow observations for five seconds. Pure parsers use synthetic inputs. Traffic samples refresh every two seconds under one mutex; counter resets give zero rates.

**Tech Stack:** Go 1.23 standard library, Linux proc files, UCI config files, iptables-save/ip6tables-save.

## Files and checkpoints

- [x] `types.go`: define the exact JSON contract from the production spec. Send exported field types to the parent before integration starts.
- [x] `parse_proc.go`, `parse_proc_test.go`: implement IPv4 little-endian hex routes, IPv6 network-order routes, and network counters. Validate headers, row lengths, masks, hex numbers, and overflow. Tests must cover malformed rows, byte order, and empty valid sources.
- [x] `parse_uci.go`, `parse_uci_test.go`: tokenize UCI without shell execution, select only safe fields, parse version and wireless radio/iface relationships. Test quoted values and secret non-disclosure.
- [x] `parse_network.go`, `parse_network_test.go`: parse synthetic leases, ARP, resolvers, and firewall policies/rule counts. Invalid rows produce partial errors and never raw data in errors.
- [x] `sources.go`: bounded rooted reads and fixed argument timed firewall commands. Fixture roots must never execute host commands. Tests cover missing, oversize, and denied/root-escaping sources plus command bounds.
- [x] `adapter.go`, `adapter_test.go`: integrate per-module observations, source fallbacks, 2s/5s caches, deep-copy results, and delta/reset behavior. Context cancellation is the only ordinary top-level error. A single module failure must preserve others.
- [x] `README.md`: document public API, fixture paths, time semantics, configured-versus-runtime WiFi, firewall rule counts, online evidence, and source/refresh limits.

## Validation and publication

Run `go test ./internal/router`, `go test -race ./internal/router`, and `go vet ./internal/router` from this worktree. Cross-compile the package tests with `GOOS=linux GOARCH=arm GOARM=7 go test -c -o /tmp/router-adapter-arm.test ./internal/router`. Confirm `git diff --name-only` contains only `internal/router/**`. Commit the verified package and send the hash, API, and source limitations to the parent. Do not connect to the router or copy live fixture data.
