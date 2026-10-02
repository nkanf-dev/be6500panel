# Native proxy engine implementation plan

Goal: compile bounded private VLESS subscriptions into native sing-box 1.14.2 configurations and produce owned single-client firewall intent. No network or command execution occurs in this package.

Files and steps:
1. `types.go`, `subscription.go`, `subscription_test.go`: bounded YAML tree validation, private VLESS/REALITY validation, ordered known rules, explicit unsupported diagnostics, safe public node projection.
2. `native.go`, `native_test.go`: current 1.14 DNS transports/actions, mixed TCP/UDP SOCKS input, transparent TCP/UDP input, DNS input, management/bootstrap bypass, domain-first intent, controlled local SRS, IPv6/failure behavior, deterministic private JSON hash.
3. `firewall.go`, `firewall_test.go` (independent worker): validated exact argv only, single-client scope, free mark bit, separate IPv4/IPv6 chains and owned cleanup; no ECM change.
4. `README.md`: stable root API contract and activation prerequisites.
5. Run `go test ./internal/proxy`, `go test -race ./internal/proxy`, `go vet ./internal/proxy`, then all repository tests. Commit only owned files and yaml.v3 module dependency.

Native source basis: upstream sing-box v1.14.2 `option/dns.go`, `option/rule_action.go`, `option/inbound.go`, `option/simple.go`, `option/vless.go`, `option/tls.go`, `option/route.go`, `option/rule_set.go`, `protocol/vless/outbound.go`, and `route/route.go`. Root must run an ARM native config check and actual traffic verification before activation.
