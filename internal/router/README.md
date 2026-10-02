# RN02 read-only adapter

```go
adapter := router.New("") // live /; keep one adapter for all clients
snapshot, err := adapter.Snapshot(ctx)
```

`New(root string) *Adapter` and `Snapshot(context.Context) (Snapshot, error)` use only the Go standard library. The exported JSON types live in `types.go`. A caller owns each returned copy. All list fields are non-null arrays.

## Observation and refresh semantics

- Call `Snapshot` from the application's central sampler every two seconds. The adapter starts no goroutine. Calls inside two seconds return the same cached observation without reads or commands.
- Slow sources refresh after five seconds. At a two-second caller cadence, this usually means every six seconds. Concurrent clients share the mutex and cache.
- Traffic comes from `/proc/net/dev`, measured in bytes and bytes per second. The first sample is zero-rate. Rates use actual elapsed sample time. A counter reset gives zero for that direction. A disappearing interface loses its previous sample. No speed or throughput is invented when a source fails.
- `sampledAt` is the UTC time of the traffic read. Slow fields can be up to the refresh interval older. There is no claim that independently read files and firewall commands form an atomic kernel snapshot.
- Source failures go into `errors` with a module and stable code. Codes are `unavailable`, `permission_denied`, `read_failed`, `too_large`, `timeout`, `canceled`, and `invalid`. Valid rows survive malformed rows. Only caller context cancellation/deadline returns a top-level error.
- `devices[].online` means a complete ARP entry exists. ARP cache evidence is not an active reachability probe. Devices include current IPv4 DHCP leases and complete ARP-only entries. `expiresAt` is a UTC timestamp, or null for infinite leases/ARP-only entries. `dns.leaseCount` counts valid, unexpired leases, not all ARP devices.
- WiFi is **configured** safe UCI data, not a runtime-radio probe. Auto channel returns `channel: 0`. `hwmode: 11beg` maps to `2.4GHz`; `11bea` maps to `5GHz`. Nonzero RN02 `bw` is shown as configured; zero/absent `bw` falls back to `htmode` such as `HT40`. An absent/unknown band remains empty. `disabled` combines radio and interface settings. `name` uses `ifname`, otherwise the named/anonymous UCI section.
- Firewall policies come from the filter table. `rules` counts `-A` rules across all saved tables. Missing policies are empty, never assumed `ACCEPT`. Raw rules, comments, and counters are not returned.
- Routes include IPv4 and IPv6 up/non-reject routes. IPv4 proc words use RN02/Linux little-endian layout; IPv6 words use network byte order. Destination strings use CIDR; direct gateways are `0.0.0.0` or `::`. The schema does not expose IPv6 source-specific route selectors.

## Sources and bounds

| Module | Source |
| --- | --- |
| Platform | `/etc/config/version` (`HARDWARE`, `ROM`), optional `/etc/miwifi_version` or `/etc/xiaoqiang_version` assignments, `/tmp/sysinfo/model`, `/etc/openwrt_release`, `/proc/sys/kernel/osrelease` |
| Devices | First existing `/tmp/dhcp.leases`, `/tmp/dnsmasq.leases`, `/var/lib/misc/dnsmasq.leases`; `/proc/net/arp` |
| WiFi | `/etc/config/wireless`; only safe fields are retained |
| DNS | First existing `/tmp/resolv.conf.d/resolv.conf.auto`, `/tmp/resolv.conf.auto`, `/etc/resolv.conf` |
| Firewall | Fixed argv `iptables-save` and `ip6tables-save`, no shell or arguments |
| Traffic | `/proc/net/dev` |
| Routes | `/proc/net/route`, `/proc/net/ipv6_route` |

Files are limited to 256 KiB, proc sources to 512 KiB, and each firewall stdout to 512 KiB. Each command has a 750 ms context timeout and 100 ms pipe-drain bound. Stderr is discarded. No UCI export, write command, network probe, DNS lookup, or router connection is performed. Read error messages never include raw configuration or command output. The RN02 model and firmware come from actual version data, not constants. Architecture uses OpenWrt's userspace target; live fallback uses the running Go architecture.

For any root other than `/` or `""`, all files resolve under that fixture root and **no host command runs**. Fixture firewall files are `/var/run/be6500panel/iptables-save` and `/var/run/be6500panel/ip6tables-save` under the root. These paths are fixture-only. Out-of-root symlinks are rejected; normal in-root symlinks work. Synthetic test fixtures are generated inside test temp directories. No device capture is stored in this package.

## Validation

```text
go test ./internal/router
go test -race ./internal/router
go vet ./internal/router
GOOS=linux GOARCH=arm GOARM=7 go test -c -o /tmp/router-adapter-arm.test ./internal/router
```
