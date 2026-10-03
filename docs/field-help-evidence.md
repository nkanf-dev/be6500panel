# Configuration field help: firmware evidence

This catalog describes every field in the panel's canonical inventory. All fields remain editable. Generated, legacy, hardware-dependent, and credential flags describe behavior; they do not add read-only controls.

## Evidence boundaries

- Baseline: Xiaomi RN02 firmware 1.0.43, read-only evidence root `/Volumes/RN02_STATIC/rootfs` (`BE6500_FIRMWARE_ROOT`).
- Decompiled vendor Lua: `/Users/nkanf/docs/miwifibe6500/static/lua-analysis/decompiled` (under `BE6500_RESEARCH_ROOT`). These are research artifacts derived from baseline Lua bytecode, not current live settings.
- Public 1.0.64 service/protocol scripts: `/Users/nkanf/docs/miwifibe6500/live-inspection/field-help-live-1.0.64`. No private `/etc/config` values are copied here.
- Public script comparison: Dropbear, system, DHCPv4 and PPP scripts have the same SHA in 1.0.43 and the inspected 1.0.64 scripts. dnsmasq and DHCPv6 scripts differ. No static protocol script exists in either snapshot; static interface parsing belongs to netifd.
- A source location proves the stated fact only. A parsed option name in an ELF table does not by itself prove runtime behavior. Search results list exactly which consumers were checked and which finer behavior is not yet proved.
- Defaults below are explicit source fallbacks, not sampled settings. Units and ranges describe the named consumer or protocol, not blanket device support. Existing form bounds are unchanged.
- Stock Dropbear init gates are not the panel's independently managed rescue SSH process. Help does not turn these fields into read-only controls.

## Reproduce source integrity checks

From `web/`:

```sh
BE6500_FIRMWARE_ROOT="/Volumes/RN02_STATIC/rootfs" BE6500_RESEARCH_ROOT="/Users/nkanf/docs/miwifibe6500" node scripts/build-field-evidence-manifest.mjs
BE6500_FIRMWARE_ROOT="/Volumes/RN02_STATIC/rootfs" BE6500_RESEARCH_ROOT="/Users/nkanf/docs/miwifibe6500" npm test -- src/components/configuration/field-help.test.ts src/components/configuration/NativeFields-help.test.tsx
```

The manifest stores source SHA-256 hashes, byte sizes and line counts, not private source settings. Tests cover inventory equality, path/line bounds, extracted-binary offset bounds, firmware labels, credential help, editable native values and source hashes. These checks verify source integrity, not every semantic claim; reviewers also inspect the actual consumer branches. The optional mounted-source check is skipped when the evidence trees are not available.

## Module coverage

| Module | Catalog fields | Fields with explicit finer-evidence gaps |
| --- | ---: | ---: |
| network | 112 | 15 |
| wireless | 47 | 8 |
| dhcp | 93 | 17 |
| firewall | 103 | 6 |
| system | 27 | 20 |
| dropbear | 13 | 1 |

Total: 395 fields; 84 referenced source files.

## network

### interface

#### network/interface/proto

选择逻辑接口的地址获取或拨号协议。DHCP、DHCPv6、PPP 和隧道分别交给对应处理器；协议插件是否安装会影响可用项。

- Flags: version-dependent
- Condition: 与所选协议的地址、DNS、认证字段配合使用。
- Apply impact: 更换协议会改变地址、路由与认证流程，可能中断此接口的连接。
- Evidence: `docs/field-help-network-research.md:20` (Xiaomi RN02 1.0.43) — netifd 接口参数表声明 proto 的原生输入类型。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172632.
- Evidence: `lib/netifd/netifd-proto.sh:489–495` (Xiaomi RN02 1.0.43) — add_protocol 只匹配请求的协议，再分派 setup、teardown 或 renew。
- Evidence: `lib/netifd/proto/ppp.sh:405–409` (Xiaomi RN02 1.0.43) — PPPoE/PPTP 的注册依赖对应 pppd 插件文件。
- Evidence: `docs/field-help-network-research.md:167` (Xiaomi RN02 1.0.43) — 接口创建解析真实参数表并消费 proto 槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 27008.
- Evidence: `live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh:405–409` (Xiaomi RN02 1.0.64) — 当前公开脚本与 1.0.43 逐字节相同；PPPoE/PPTP 的注册依赖对应 pppd 插件文件。

#### network/interface/device

设置逻辑接口绑定的底层设备。netifd 接口参数表明确接收 device 字符串；串口 PPP 也使用同名参数，但含义是 PPP 设备路径。串口 PPP 的同名 device 则由协议解释为 PPP 设备路径。

- Compact help: 绑定底层网卡或桥，优先于 ifname；串口 PPP 则表示 PPP 设备路径。
- Flags: hardware-dependent
- Condition: 普通接口引用已有设备；device 缺失时核心回退读取 ifname；串口 PPP 则由协议解释此项。
- Apply impact: 修改底层绑定会改变逻辑接口的承载设备；串口 PPP 与以太网设备名称不能混用。
- Evidence: `docs/field-help-network-research.md:18` (Xiaomi RN02 1.0.43) — netifd 接口参数表将 device 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172616.
- Evidence: `lib/netifd/proto/ppp.sh:197–209` (Xiaomi RN02 1.0.43) — 串口 PPP 读取 device 并将其传给通用 PPP setup。
- Evidence: `docs/field-help-network-research.md:167` (Xiaomi RN02 1.0.43) — 接口创建解析真实参数表并消费 device 槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 27008.
- Evidence: `live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh:197–209` (Xiaomi RN02 1.0.64) — 当前公开脚本与 1.0.43 逐字节相同；串口 PPP 读取 device 并将其传给通用 PPP setup。
- Evidence: `docs/field-help-network-research.md:176` (Xiaomi RN02 1.0.43) — 核心接口主设备绑定优先 device，缺省再读 ifname。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 24404.

#### network/interface/ifname

绑定已有网卡或网卡列表。厂商端口映射会重建 LAN 的 ifname；网桥接口在状态查询中可被替换为 br-接口名或实际三层设备。

- Flags: legacy, generated, hardware-dependent
- Condition: type=bridge 时可有多个成员；厂商端口分配操作可能重写此项。
- Apply impact: 修改成员或网卡会改变 LAN/WAN 的物理承载，可能让管理连接转到另一端口。
- Evidence: `docs/field-help-network-research.md:19` (Xiaomi RN02 1.0.43) — netifd 接口参数表声明 ifname 的原生输入类型。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172624.
- Evidence: `lib/network/config.sh:42–49` (Xiaomi RN02 1.0.43) — fixup_interface 读取 ifname，桥接时使用 br-章节名，有 l3_device 时再覆盖。
- Evidence: `sbin/port_map:148–168` (Xiaomi RN02 1.0.43) — 端口映射收集 LAN 端口的 ifname，并写回 network.lan.ifname 后 reload。
- Evidence: `docs/field-help-network-research.md:167` (Xiaomi RN02 1.0.43) — 接口创建解析真实参数表并消费 ifname 槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 27008.

#### network/interface/type

旧式 interface 章节中，bridge 表示将成员接口组成网桥。访客网络创建脚本仍使用这一结构。

- Flags: legacy
- Condition: 与 ifname 成员及该逻辑接口的 IP 配置配合。
- Apply impact: 桥接会把成员接入同一二层网络；改变 LAN 桥结构可能中断本机管理与终端互通。
- Evidence: `lib/network/config.sh:42–44` (Xiaomi RN02 1.0.43) — type 为 bridge 时，脚本采用 br-章节名作为接口名。
- Evidence: `usr/sbin/guestwifi.sh:87–94` (Xiaomi RN02 1.0.43) — 访客网络以 interface 章节创建，并设置 type=bridge、proto=static。

#### network/interface/ipaddr

static 模式下设置 IPv4 本机地址；dhcp 模式下同名参数是向 DHCP 服务器请求的地址，不是强制使用的固定地址。

- Format / range: IPv4 地址；静态网段还需正确的掩码。
- Condition: 解释取决于 proto；LAN 地址还需与 DHCP 地址池和防火墙网络一致。
- Apply impact: 静态地址改变后，管理入口和本地网段会改变；DHCP 请求地址仍由服务器决定。
- Evidence: `docs/field-help-network-research.md:65` (Xiaomi RN02 1.0.43) — netifd static 地址参数表接收 ipaddr。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173208.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/admin_network/proto_static.lua:21–27` (Xiaomi RN02 1.0.43) — 静态协议表单将 ipaddr 标记为 IPv4 address，并要求 ip4addr。
- Evidence: `lib/netifd/proto/dhcp.sh:102` (Xiaomi RN02 1.0.43) — DHCP 处理器把非空 ipaddr 传给 udhcpc 的 -r 地址请求选项。
- Evidence: `live-inspection/field-help-live-1.0.64/lib/netifd/proto/dhcp.sh:102` (Xiaomi RN02 1.0.64) — 当前公开脚本与 1.0.43 逐字节相同；DHCP 处理器把非空 ipaddr 传给 udhcpc 的 -r 地址请求选项。

#### network/interface/netmask

定义静态 IPv4 地址的网段掩码。Mesh 子节点的 DHCP 回调也会把租约掩码写回 LAN 配置。

- Flags: generated
- Condition: 与 ipaddr 一起设置；Mesh RE 模式可能由租约回写。
- Apply impact: 掩码决定哪些地址视为本地直连；与 DHCP 地址池不一致时，终端可能无法访问网关。
- Evidence: `docs/field-help-network-research.md:67` (Xiaomi RN02 1.0.43) — netifd static 地址参数表接收 netmask。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173224.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/admin_network/proto_static.lua:32–38` (Xiaomi RN02 1.0.43) — 静态协议表单定义 netmask，标签为 IPv4 netmask。
- Evidence: `lib/netifd/dhcp.script:214–223` (Xiaomi RN02 1.0.43) — Mesh RE 租约回调把 netmask 写入 network.lan.netmask。

#### network/interface/gateway

设置静态 IPv4 接口的下一跳网关。Mesh RE 的 DHCP 回调会用租约的 router 更新 LAN gateway。

- Flags: generated
- Condition: 用于静态地址；网关需能由承载接口到达。
- Apply impact: 影响接口离开本地子网的路径；错误网关可能使上网或远端管理失败。
- Evidence: `docs/field-help-network-research.md:70` (Xiaomi RN02 1.0.43) — netifd static 地址参数表接收 gateway。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173248.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/admin_network/proto_static.lua:55–61` (Xiaomi RN02 1.0.43) — 静态协议表单将 gateway 标记为 IPv4 gateway 并要求 ip4addr。
- Evidence: `lib/netifd/dhcp.script:220–223` (Xiaomi RN02 1.0.43) — Mesh RE 回调将租约 router 写为 network.lan.gateway。

#### network/interface/broadcast

static 模式下表示 IPv4 广播地址。dhcp 处理器却将同名字段作为布尔值，用于发送带广播标志的 DHCP 请求。

- Compact help: static 用广播地址；dhcp 同名字段是 0/1 广播标志，格式不能混用。
- Condition: 必须先确认 proto；static 用地址，dhcp 用 0/1 原生值。
- Apply impact: 静态广播地址影响本子网广播；DHCP 广播标志影响租约协商，不能把两种格式混用。
- Evidence: `docs/field-help-network-research.md:68` (Xiaomi RN02 1.0.43) — netifd static 地址参数表接收 broadcast。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173232.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/admin_network/proto_static.lua:66–72` (Xiaomi RN02 1.0.43) — 静态表单的 broadcast 是 IPv4 broadcast，datatype 为 ip4addr。
- Evidence: `lib/netifd/proto/dhcp.sh:17` (Xiaomi RN02 1.0.43) — DHCP 将 broadcast 声明为 bool。
- Evidence: `lib/netifd/proto/dhcp.sh:57` (Xiaomi RN02 1.0.43) — broadcast=1 时给 udhcpc 添加 -B。
- Evidence: `live-inspection/field-help-live-1.0.64/lib/netifd/proto/dhcp.sh:17` (Xiaomi RN02 1.0.64) — 当前公开脚本与 1.0.43 逐字节相同；DHCP 将 broadcast 声明为 bool。

#### network/interface/ip6addr

设置静态 IPv6 本机地址。PPP 自动 IPv6 的静态分支会读取 wan6 的地址并传给动态接口。

- Format / range: IPv6 地址及原生前缀格式。
- Condition: 静态 IPv6；PPP 自动创建路径还依赖 ipv6.<wan6>.mode=static。
- Apply impact: 改变接口的 IPv6 可达地址；地址与前缀不匹配会影响下游和上游通信。
- Evidence: `docs/field-help-network-research.md:66` (Xiaomi RN02 1.0.43) — netifd static 地址参数表接收 ip6addr。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173216.
- Evidence: `lib/netifd/ppp6-up:33–37` (Xiaomi RN02 1.0.43) — 静态 IPv6 分支要求 ip6addr、ip6gw、ip6prefix 都非空。
- Evidence: `lib/netifd/ppp6-up:45–49` (Xiaomi RN02 1.0.43) — 静态动态接口更新提交 ip6addr 数组。

#### network/interface/ip6gw

设置静态 IPv6 的下一跳网关；PPP 自动 IPv6 的静态分支将此值传入 wan6 动态配置。

- Format / range: IPv6 网关地址。
- Condition: 与 ip6addr、ip6prefix 及静态 IPv6 模式配合。
- Apply impact: 网关决定静态 IPv6 的上游出口；错误值会使远端 IPv6 目标不可达。
- Evidence: `docs/field-help-network-research.md:71` (Xiaomi RN02 1.0.43) — netifd static 地址参数表接收 ip6gw。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173256.
- Evidence: `lib/netifd/ppp6-up:33–37` (Xiaomi RN02 1.0.43) — 静态分支读取 network.<wan6>.ip6gw，并要求其非空。
- Evidence: `lib/netifd/ppp6-up:48` (Xiaomi RN02 1.0.43) — 动态 static 接口配置提交 ip6gw。

#### network/interface/ip6prefix

配置可供下游使用的 IPv6 前缀，不等同于本机 ip6addr。DHCPv6 处理器会将手动前缀导出为 USERPREFIX；PPP 静态分支也会转交该前缀。

- Format / range: IPv6 前缀及前缀长度，可保留原生列表。
- Condition: 下游分配还需 ip6assign；上游需能够路由此前缀。
- Apply impact: 手动前缀会影响下游地址来源；错误前缀可能让 LAN IPv6 看似有地址却无法回程。
- Evidence: `docs/field-help-network-research.md:72` (Xiaomi RN02 1.0.43) — netifd static 地址参数表接收 ip6prefix。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173264.
- Evidence: `lib/netifd/proto/dhcpv6.sh:22` (Xiaomi RN02 1.0.43) — DHCPv6 将 ip6prefix 声明为地址列表。
- Evidence: `lib/netifd/proto/dhcpv6.sh:116` (Xiaomi RN02 1.0.43) — 读取 ip6prefix 列表并累加至 ip6prefixes。
- Evidence: `lib/netifd/proto/dhcpv6.sh:174` (Xiaomi RN02 1.0.43) — 非空 ip6prefixes 导出为 USERPREFIX。
- Evidence: `lib/netifd/ppp6-up:49` (Xiaomi RN02 1.0.43) — PPP 的静态 IPv6 动态配置转交 ip6prefix。

#### network/interface/ip6assign

选择分配给下游的 IPv6 前缀长度。厂商 LAN IPv6 设置会比较旧值，变更后提交 network 并重新加载 IPv6 网络。

- Unit: 位
- Format / range: IPv6 前缀长度为 0–128；上游前缀必须容纳所需子网。
- Condition: 需要可用的上游委派前缀或本地前缀；与 LAN IPv6 服务模式相关。
- Apply impact: 长度改变会重划下游子网和 IPv6 地址；终端可能需要重新获取地址。
- Evidence: `docs/field-help-network-research.md:31` (Xiaomi RN02 1.0.43) — netifd 接口参数表声明 ip6assign 的原生输入类型。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172720.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua:8274–8281` (Xiaomi RN02 1.0.43) — 厂商函数从 network.lan 读取 ip6assign 并转为数字。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua:8431–8449` (Xiaomi RN02 1.0.43) — ip6assign 改变后写入 LAN、提交 network，并执行 ipv6.sh reload_network all。
- Evidence: `docs/field-help-network-research.md:167` (Xiaomi RN02 1.0.43) — 接口创建解析真实参数表并消费 ip6assign 槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 27008.

#### network/interface/ip6hint

IPv6 子网提示，以十六进制子网编号选择可分配前缀内的子网。netifd 接口表接收字符串，不应改成十进制数值。

- Compact help: 十六进制子网编号提示，需能放入 ip6assign 对应的分配空间。
- Flags: version-dependent
- Condition: 面板语义要求配合 ip6assign 与可用前缀。
- Apply impact: 改变希望分配的下游子网编号；超出分配空间的位会被掩码去掉，不保证上游一定提供所需前缀。
- Evidence: `docs/field-help-network-research.md:32` (Xiaomi RN02 1.0.43) — netifd 接口参数表将 ip6hint 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172728.
- Evidence: `etc/init.d/network:34` (Xiaomi RN02 1.0.43) — 网络服务启动 netifd；前缀分配实现不在该启动脚本中。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua:8277–8281` (Xiaomi RN02 1.0.43) — 已检查的 LAN IPv6 消费函数读取 network.lan.ip6assign。
- Evidence: `docs/field-help-network-research.md:168` (Xiaomi RN02 1.0.43) — 核心将 ip6hint 按 base=16 转换，并按分配长度掩码保存。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 27432.

#### network/interface/ip6class

设置参与 IPv6 前缀分配的类别列表；netifd 接口表接收数组。厂商切换 WAN IPv6 模式会清空 LAN 的此项。

- Flags: generated, version-dependent
- Condition: 需要可用前缀类别；可保留上游接口名称或原生列表。
- Apply impact: 若核心执行类别筛选，会影响 LAN 取得哪个上游前缀；厂商模式切换可能重置此选择。
- Trace result: 已确认 interface.ip6class 的数组解析、核心逐项字符串保存及厂商清空路径；类别不匹配和前缀冲突时的最终分配顺序未在本轮定位。
- Evidence: `docs/field-help-network-research.md:35` (Xiaomi RN02 1.0.43) — netifd 接口参数表将 ip6class 声明为 array。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172752.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua:7150–7154` (Xiaomi RN02 1.0.43) — WAN IPv6 模式配置函数调用 set 清空 ip6class。
- Evidence: `docs/field-help-network-research.md:167` (Xiaomi RN02 1.0.43) — 接口创建解析真实参数表并消费 ip6class 槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 27008.

#### network/interface/dns

设置手动 DNS 服务器列表。PPP 创建动态 IPv6 接口时，peerdns=0 才会读此列表；静态协议表单将其定义为自定义 DNS。

- Condition: DHCP/拨号场景与 peerdns 配合；地址需能经路由到达。
- Apply impact: 影响路由器及其 DNS 转发的上游解析来源；错误地址会造成域名解析失败。
- Evidence: `docs/field-help-network-research.md:26` (Xiaomi RN02 1.0.43) — netifd 接口参数表声明 dns 的原生输入类型。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172680.
- Evidence: `lib/netifd/ppp6-up:80–86` (Xiaomi RN02 1.0.43) — peerdns=0 时读取 network.<接口>.dns 并加入动态 IPv6 DNS 更新。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/admin_network/proto_static.lua:77–84` (Xiaomi RN02 1.0.43) — 静态协议表单使用 DynamicList 定义自定义 dns，datatype 为 ipaddr。
- Evidence: `docs/field-help-network-research.md:167` (Xiaomi RN02 1.0.43) — 接口创建解析真实参数表并消费 dns 槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 27008.

#### network/interface/dns_search

设置 DNS 搜索域列表，供短主机名解析时展开。netifd 接口表直接接收 dns_search 数组；DHCP 租约中的 domain 也作为搜索域提交。

- Compact help: DNS 搜索域列表，用于展开短主机名；与 DNS 服务器地址分开设置。
- Flags: version-dependent
- Condition: 需与 DNS 服务器及其可解析的域配合。
- Apply impact: 改变短名称的域名展开，可能影响本地名称解析；不会更改 DNS 服务器地址。
- Evidence: `docs/field-help-network-research.md:27` (Xiaomi RN02 1.0.43) — netifd 接口参数表将 dns_search 声明为 array。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172688.
- Evidence: `lib/netifd/dhcp.script:81–83` (Xiaomi RN02 1.0.43) — DHCP 回调将租约 domain 逐项加入 DNS 搜索域。
- Evidence: `lib/netifd/netifd-proto.sh:349` (Xiaomi RN02 1.0.43) — 协议更新把 PROTO_DNS_SEARCH 输出为 dns_search 数组。
- Evidence: `docs/field-help-network-research.md:169` (Xiaomi RN02 1.0.43) — 接口创建直接消费手动 dns_search 数组。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 27160.

#### network/interface/peerdns

允许使用上游协商得到的 DNS。核心接口创建在缺少此项时采用 1；PPP 自动 IPv6 路径同样回退为 1，设为 0 时读取手动 dns。

- Source fallback: 1（核心接口创建读取该项时的缺省；协议/厂商模式仍可能另行控制）。
- Condition: 主要用于 DHCP/拨号；关闭后需可用的手动 dns。
- Apply impact: 决定动态 IPv6 接口使用上游还是手动 DNS；不影响 DHCP 服务器给终端分配地址。
- Evidence: `docs/field-help-network-research.md:25` (Xiaomi RN02 1.0.43) — netifd 接口参数表声明 peerdns 的原生输入类型。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172672.
- Evidence: `lib/netifd/ppp6-up:77–86` (Xiaomi RN02 1.0.43) — 创建动态 IPv6 接口时 peerdns 为空回退为 1，并在值为 0 时加载手动 dns。
- Evidence: `lib/netifd/netifd-proto.sh:25` (Xiaomi RN02 1.0.43) — 协议公共配置声明 peerdns 为布尔项。
- Evidence: `docs/field-help-network-research.md:167` (Xiaomi RN02 1.0.43) — 接口创建解析真实参数表并消费 peerdns 槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 27008.
- Evidence: `docs/field-help-network-research.md:175` (Xiaomi RN02 1.0.43) — 布尔项缺失时返回调用者提供的回退1，非空规范化为0/1。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 24084.

#### network/interface/defaultroute

决定协议接口是否允许安装默认出口。公共协议配置声明此布尔项，随固件附带的 DHCP 表单说明关闭后不配置默认路由。

- Source fallback: 1（核心接口创建读取该项时的缺省；协议/厂商模式仍可能另行控制）。
- Condition: 与 proto、上游网关、metric 及其他出口配合。
- Apply impact: 关闭可能让此 WAN 不再承担普通上网出口；已有静态或其他接口的路由仍可独立存在。
- Evidence: `docs/field-help-network-research.md:24` (Xiaomi RN02 1.0.43) — netifd 接口参数表声明 defaultroute 的原生输入类型。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172664.
- Evidence: `lib/netifd/netifd-proto.sh:23–26` (Xiaomi RN02 1.0.43) — 公共协议配置声明 defaultroute、peerdns、metric。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/admin_network/proto_dhcp.lua:90–97` (Xiaomi RN02 1.0.43) — DHCP 表单将 defaultroute 标注为 Use default gateway，说明关闭则不配置默认路由。
- Evidence: `docs/field-help-network-research.md:167` (Xiaomi RN02 1.0.43) — 接口创建解析真实参数表并消费 defaultroute 槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 27008.
- Evidence: `docs/field-help-network-research.md:175` (Xiaomi RN02 1.0.43) — 布尔项缺失时返回调用者提供的回退1，非空规范化为0/1。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 24084.

#### network/interface/delegate

允许将 IPv6 前缀交给下游分配。DHCP 的 6rd 和 DHCPv6 的 MAP/DS-Lite 子接口会在此值为 0 时传递禁用委派标志。

- Source fallback: 1（核心接口创建读取该项时的缺省；协议/厂商模式仍可能另行控制）。
- Condition: 需要上游委派或手动前缀；下游用 ip6assign 分配。
- Apply impact: 关闭会影响这些动态子接口的下游前缀可用性，但不等同于停用 IPv6 地址或 DNS。
- Evidence: `docs/field-help-network-research.md:36` (Xiaomi RN02 1.0.43) — netifd 接口参数表声明 delegate 的原生输入类型。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172760.
- Evidence: `lib/netifd/proto/dhcp.sh:66` (Xiaomi RN02 1.0.43) — delegate=0 时导出 IFACE6RD_DELEGATE=0。
- Evidence: `lib/netifd/proto/dhcpv6.sh:178–179` (Xiaomi RN02 1.0.43) — delegate=0 时导出 DS-Lite 与 MAP 的禁用委派标志。
- Evidence: `lib/netifd/dhcpv6.script:250` (Xiaomi RN02 1.0.43) — MAP 动态接口把 IFACE_MAP_DELEGATE 传为 delegate。
- Evidence: `docs/field-help-network-research.md:167` (Xiaomi RN02 1.0.43) — 接口创建解析真实参数表并消费 delegate 槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 27008.
- Evidence: `live-inspection/field-help-live-1.0.64/lib/netifd/proto/dhcp.sh:66` (Xiaomi RN02 1.0.64) — 当前公开脚本与 1.0.43 逐字节相同；delegate=0 时导出 IFACE6RD_DELEGATE=0。
- Evidence: `docs/field-help-network-research.md:175` (Xiaomi RN02 1.0.43) — 布尔项缺失时返回调用者提供的回退1，非空规范化为0/1。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 24084.

#### network/interface/ipv6

此字段的含义取决于协议。RN02 的通用 PPP 处理器仅在值为 auto 且 force_disable_ipv6 不为 1 时打开 IPv6 并启用自动 wan6；L2TP 则以值 1 启用 IPv6。

- Compact help: 协议含义不同：通用 PPP 使用 auto；L2TP 使用 1，不能视为等价。
- Flags: version-dependent
- Condition: 受 proto、network.<接口>.force_disable_ipv6 和 ipv6.<wan6>.mode 影响。
- Apply impact: 改变 PPP 的 IPv6 协商和动态子接口生成；不要认为 1 与 auto 在所有拨号协议中等价。
- Evidence: `lib/netifd/proto/ppp.sh:101–107` (Xiaomi RN02 1.0.43) — 通用 PPP 只在 ipv6=auto 且未 force_disable_ipv6 时设置 IPv6 和 AUTOIPV6。
- Evidence: `lib/netifd/proto/l2tp.sh:67` (Xiaomi RN02 1.0.43) — L2TP 仅保留值为 1 的 ipv6。
- Evidence: `lib/netifd/ppp6-up:19–31` (Xiaomi RN02 1.0.43) — AUTOIPV6=1 时进入动态 wan6 创建逻辑。
- Evidence: `live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh:101–107` (Xiaomi RN02 1.0.64) — 当前公开脚本与 1.0.43 逐字节相同；通用 PPP 只在 ipv6=auto 且未 force_disable_ipv6 时设置 IPv6 和 AUTOIPV6。

#### network/interface/auto

控制逻辑接口的自动启动；netifd 接口表接收布尔值。另有厂商 autovpn：PPTP 的 vpn.auto=1 且未连接时，会发起重连。

- Source fallback: 1（核心接口创建读取该项时的缺省；协议/厂商模式仍可能另行控制）。
- Condition: PPTP 自动重连另需 user_option=1，且非 AP/Mesh 子节点模式。
- Apply impact: 关闭可阻止接口自动启动；PPTP VPN 的自动重连还受厂商模式和用户开关控制。
- Evidence: `docs/field-help-network-research.md:21` (Xiaomi RN02 1.0.43) — netifd 接口参数表将 auto 声明为 bool/int8。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172640.
- Evidence: `etc/init.d/autovpn:41–50` (Xiaomi RN02 1.0.43) — vpn.auto=1、proto=pptp、user_option=1 且状态未连接时执行 ifdown 与 vpn.lua up。
- Evidence: `docs/field-help-network-research.md:167` (Xiaomi RN02 1.0.43) — 接口创建解析真实参数表并消费 auto 槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 27008.
- Evidence: `docs/field-help-network-research.md:175` (Xiaomi RN02 1.0.43) — 布尔项缺失时返回调用者提供的回退1，非空规范化为0/1。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 24084.

#### network/interface/force_link

控制接口是否忽略底层载波状态。核心读取布尔项，未设置时采用协议标志；厂商 LAN 链路聚合重建会写入 1。

- Flags: generated, hardware-dependent
- Condition: 与底层链路或聚合设备相关。
- Apply impact: 若核心接受此项，可在物理链路未就绪时仍处理接口；链路聚合重建可能覆盖手动设置。
- Evidence: `docs/field-help-network-research.md:38` (Xiaomi RN02 1.0.43) — netifd 接口参数表将 force_link 声明为 bool/int8。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172776.
- Evidence: `sbin/port_map:153–158` (Xiaomi RN02 1.0.43) — LAN 端口 service=lag 时向对应 network 章节写 force_link=1。
- Evidence: `docs/field-help-network-research.md:167` (Xiaomi RN02 1.0.43) — 接口创建解析真实参数表并消费 force_link 槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 27008.

#### network/interface/disabled

标记逻辑接口停用。厂商网络诊断遇到 WAN disabled=1 会跳过该接口；IPv6 模式迁移也会生成此标记以避免重复 WAN6。

- Flags: generated
- Condition: 厂商 IPv6 模式可能管理 WAN6 的此标记。
- Apply impact: 停用 WAN 可能移除该出口；停用 LAN 可能失去当前管理连接，诊断也不再检测停用的 WAN。
- Evidence: `usr/sbin/nettb2:436–440` (Xiaomi RN02 1.0.43) — WAN 检查在 network.<wan>.disabled=1 时直接返回。
- Evidence: `usr/sbin/ipv6.sh:283–286` (Xiaomi RN02 1.0.43) — 旧 IPv6 配置迁移在 PPPoE 时设 disabled=1。
- Evidence: `usr/sbin/ipv6.sh:301` (Xiaomi RN02 1.0.43) — 迁移函数将 disabled 写入 wan6 接口。

#### network/interface/mtu

设置接口最大传输单元。通用以太网和隧道不要混用：L2TP 虽读取此字段，随后仍改用当前 WAN 的 MTU/MRU 减 40；PPPoE 实际由 mru 控制。

- Compact help: 字节；L2TP 会按 WAN 重算，PPPoE 的实际参数由 mru 控制。
- Unit: 字节
- Condition: 取决于 proto 和承载链路；PPPoE 还需检查原生 mru。
- Apply impact: 过大可能造成分片或路径 MTU 故障，过小增加开销；L2TP 中手动 mtu 可能被重算。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/admin_network/proto_static.lua:170–177` (Xiaomi RN02 1.0.43) — 静态协议表单将 mtu 标记为 Override MTU；1500 只是 placeholder。
- Evidence: `lib/netifd/proto/l2tp.sh:65–66` (Xiaomi RN02 1.0.43) — L2TP 读取 mtu。
- Evidence: `lib/netifd/proto/l2tp.sh:78–95` (Xiaomi RN02 1.0.43) — L2TP 根据 WAN 的 mtu/mru 及 VLAN 开销计算最终 MTU，再减 40。
- Evidence: `lib/netifd/proto/ppp.sh:165` (Xiaomi RN02 1.0.43) — 通用 PPP 使用 mru 同时生成 pppd 的 mtu 和 mru。
- Evidence: `live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh:165` (Xiaomi RN02 1.0.64) — 当前公开脚本与 1.0.43 逐字节相同；通用 PPP 使用 mru 同时生成 pppd 的 mtu 和 mru。

#### network/interface/metric

设置接口路由的度量值，较小值通常优先。公共协议配置声明为整数，DHCP 表单约束为非负整数；表单的 0 占位符不是已证明的运行缺省值。

- Format / range: 非负整数。
- Condition: 与 defaultroute、同目标的其他路由及多 WAN 策略配合。
- Apply impact: 与其他接口竞争相同目标路由时影响选择顺序；不直接限制带宽。
- Evidence: `docs/field-help-network-research.md:29` (Xiaomi RN02 1.0.43) — netifd 接口参数表声明 metric 的原生输入类型。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172704.
- Evidence: `lib/netifd/netifd-proto.sh:26` (Xiaomi RN02 1.0.43) — 公共协议配置把 metric 声明为整数。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/admin_network/proto_dhcp.lua:137–144` (Xiaomi RN02 1.0.43) — DHCP 表单将 metric 标注为 Use gateway metric，datatype 为 uinteger，0 为 placeholder。
- Evidence: `docs/field-help-network-research.md:167` (Xiaomi RN02 1.0.43) — 接口创建解析真实参数表并消费 metric 槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 27008.

#### network/interface/macaddr

覆盖接口的 MAC 地址。启动初始化只对 LAN/WAN 系列且非 CPE 接口处理：缺少此项时，从 getmac 读取硬件地址并写入配置。

- Source fallback: LAN/WAN 非 CPE 且未配置时：getmac 返回的硬件地址（若非空）。
- Format / range: 原生 MAC 地址格式。
- Flags: generated, hardware-dependent
- Apply impact: 改动可能改变 DHCP 租约和运营商的设备识别；不可使同一二层网络出现重复 MAC。
- Evidence: `lib/miwifi/lib_network.sh:34–38` (Xiaomi RN02 1.0.43) — MAC 初始化只处理 lan/wan 系列并跳过 wantype=cpe。
- Evidence: `lib/miwifi/lib_network.sh:40–45` (Xiaomi RN02 1.0.43) — macaddr 为空且硬件 getmac 非空时，写入 macaddr 并提交 network。

#### network/interface/username

设置 PPP 或 L2TP 认证用户名。通用 PPP 仅在用户名非空时，才同时向 pppd 提交 user 与 password。

- Flags: credential
- Condition: 用于 PPP/PPPoE/PPTP/L2TP；与 password 配套。
- Apply impact: 错误用户名会导致认证失败；不会改变接口设备或 IP 网段。
- Evidence: `lib/netifd/proto/ppp.sh:69` (Xiaomi RN02 1.0.43) — 通用 PPP 声明 username 字符串。
- Evidence: `lib/netifd/proto/ppp.sh:158` (Xiaomi RN02 1.0.43) — username 非空时向 pppd 传 user 与 password。
- Evidence: `lib/netifd/proto/l2tp.sh:73` (Xiaomi RN02 1.0.43) — L2TP 在用户名非空时生成 user/password 认证参数。
- Evidence: `live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh:69` (Xiaomi RN02 1.0.64) — 当前公开脚本与 1.0.43 逐字节相同；通用 PPP 声明 username 字符串。

#### network/interface/password

设置拨号或隧道的认证密码。它由 PPP/L2TP 处理器传给 pppd；不是面板登录密码，也不是 Wi-Fi 密钥。

- Flags: credential
- Condition: 与 username、运营商或 VPN 账户配合。
- Apply impact: 错误密码会使拨号或隧道认证失败；保存后需等接口重新协商才可验证。
- Evidence: `lib/netifd/proto/ppp.sh:70` (Xiaomi RN02 1.0.43) — 通用 PPP 声明 password 字符串。
- Evidence: `lib/netifd/proto/ppp.sh:158` (Xiaomi RN02 1.0.43) — 用户名非空时连同 password 交给 pppd。
- Evidence: `lib/netifd/proto/l2tp.sh:73` (Xiaomi RN02 1.0.43) — L2TP 将 password 加入认证参数。
- Evidence: `live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh:70` (Xiaomi RN02 1.0.64) — 当前公开脚本与 1.0.43 逐字节相同；通用 PPP 声明 password 字符串。

#### network/interface/ac

筛选 PPPoE 接入集中器名称。值非空才会给 rp-pppoe 插件添加 rp_pppoe_ac；空值不会生成此筛选参数。

- Condition: 仅 PPPoE，且需 rp-pppoe.so 插件。
- Apply impact: 错误名称可能导致找不到接入集中器；不设置时保留插件自身的选择行为。
- Evidence: `lib/netifd/proto/ppp.sh:238` (Xiaomi RN02 1.0.43) — PPPoE setup 从 JSON 读取 ac。
- Evidence: `lib/netifd/proto/ppp.sh:252` (Xiaomi RN02 1.0.43) — ac 非空时添加 rp_pppoe_ac 参数。
- Evidence: `live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh:238` (Xiaomi RN02 1.0.64) — 当前公开脚本与 1.0.43 逐字节相同；PPPoE setup 从 JSON 读取 ac。

#### network/interface/service

筛选 PPPoE 服务名。值非空时作为 rp_pppoe_service 交给插件，不能用它给接口命名。

- Condition: 仅 PPPoE；名称由运营商服务决定。
- Apply impact: 服务名不匹配可能使 PPPoE 发现或拨号失败。
- Evidence: `lib/netifd/proto/ppp.sh:239` (Xiaomi RN02 1.0.43) — PPPoE setup 读取 service。
- Evidence: `lib/netifd/proto/ppp.sh:253` (Xiaomi RN02 1.0.43) — 非空 service 生成 rp_pppoe_service 参数。
- Evidence: `live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh:239` (Xiaomi RN02 1.0.64) — 当前公开脚本与 1.0.43 逐字节相同；PPPoE setup 读取 service。

#### network/interface/keepalive

组合值包含 LCP 探测失败次数和间隔，可用空格或逗号分隔。通用 PPP 缺省为 5 20；只填次数时，间隔回退为 5。L2TP 不共享该整体缺省。

- Source fallback: 通用 PPP：5 20；单值的间隔回退为 5。
- Unit: 失败次数 / 秒
- Condition: 用于 PPP 系列；失败次数小于 1 时通用 PPP 不生成探测参数。
- Apply impact: 用于判断对端失联；次数小或间隔短会更快重连，也更容易在暂时丢包时断线。
- Evidence: `lib/netifd/proto/ppp.sh:133–140` (Xiaomi RN02 1.0.43) — 通用 PPP keepalive 为空回退为 5 20；分离失败次数与间隔，单值间隔回退为 5。
- Evidence: `lib/netifd/proto/ppp.sh:148` (Xiaomi RN02 1.0.43) — 非空失败次数生成 lcp-echo-interval 与 lcp-echo-failure。
- Evidence: `lib/netifd/proto/l2tp.sh:69–72` (Xiaomi RN02 1.0.43) — L2TP 分离间隔，单值回退为 5；仅非空 keepalive 生成 LCP 参数。
- Evidence: `live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh:133–140` (Xiaomi RN02 1.0.64) — 当前公开脚本与 1.0.43 逐字节相同；通用 PPP keepalive 为空回退为 5 20；分离失败次数与间隔，单值间隔回退为 5。

#### network/interface/demand

设置通用 PPP 按需拨号的空闲时间。仅大于 0 时启用 demand、idle 与预编译活动流量过滤器；空值或 0 不生成按需参数。

- Source fallback: 0（只证明不启用 demand；不承诺其他重连策略）。
- Unit: 秒
- Format / range: 非负整数；正值启用按需拨号。
- Condition: 用于通用 PPP；persist、maxfail 等仍独立影响重连。
- Apply impact: 正值允许空闲后断线，后续流量触发拨号；可能增加首次请求的等待时间。
- Evidence: `lib/netifd/proto/ppp.sh:109–113` (Xiaomi RN02 1.0.43) — demand 为空按 0 判断；大于 0 时生成 demand idle 参数，否则清空。
- Evidence: `lib/netifd/proto/ppp.sh:156` (Xiaomi RN02 1.0.43) — 生成的 demand 参数被传给 pppd。
- Evidence: `live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh:109–113` (Xiaomi RN02 1.0.64) — 当前公开脚本与 1.0.43 逐字节相同；demand 为空按 0 判断；大于 0 时生成 demand idle 参数，否则清空。

#### network/interface/reqaddress

控制 DHCPv6 客户端的地址请求策略：try 尝试、force 要求、none 不请求。非空值转换为 odhcp6c 的 -N 参数。

- Format / range: try / force / none。
- Condition: 仅 dhcpv6；与 reqprefix 独立。
- Apply impact: 影响是否请求 IPv6 地址；与前缀请求独立，不请求地址并不等于不请求委派前缀。
- Evidence: `lib/netifd/proto/dhcpv6.sh:10` (Xiaomi RN02 1.0.43) — reqaddress 声明只允许 try、force、none。
- Evidence: `lib/netifd/proto/dhcpv6.sh:132` (Xiaomi RN02 1.0.43) — 非空 reqaddress 生成 -N 参数。

#### network/interface/reqprefix

控制 DHCPv6 委派前缀请求。auto 或空值转为请求长度 0；no 不生成 -P 参数；数字长度依源声明为 0–64。

- Source fallback: auto 等效行为：请求长度 0。
- Unit: 位（数字值）
- Format / range: auto / no / 0–64。
- Condition: 仅 dhcpv6；下游还需 delegate、ip6assign 及 IPv6 服务。
- Apply impact: 影响上游是否以及按什么长度提供下游前缀；上游仍可拒绝或分配不同长度。
- Evidence: `lib/netifd/proto/dhcpv6.sh:11` (Xiaomi RN02 1.0.43) — reqprefix 声明允许 auto、no 或 0–64。
- Evidence: `lib/netifd/proto/dhcpv6.sh:134–135` (Xiaomi RN02 1.0.43) — 空值或 auto 转为 0，除 no 外生成 -P 前缀请求参数。

### device

#### network/device/name

设置设备名称。核心 UCI device 分支要求 name 非空，再按 type 选择实现，以该名称创建或更新设备对象；接口引用此名称取得承载设备。

- Flags: hardware-dependent
- Condition: interface.device、bridge-vlan.device 等引用需同步。
- Apply impact: 名称变更会改变接口或 VLAN 的引用目标；其他章节仍用旧名时可能失去承载设备。
- Evidence: `sbin/devstatus:10–12` (Xiaomi RN02 1.0.43) — 设备状态查询把命令参数作为 name 提交到 network.device status。
- Evidence: `etc/init.d/network:34` (Xiaomi RN02 1.0.43) — network 服务运行 netifd 来管理网络。
- Evidence: `docs/field-help-network-research.md:183` (Xiaomi RN02 1.0.43) — 核心 device 章节读取非空 name，并用于创建/更新设备对象。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 65172.

#### network/device/type

选择设备实现类型。netifd 设备参数表接收 type 字符串，二进制还包含 bridge、8021q、8021ad 设备实现对象；普通网卡与虚拟设备的创建条件不同。

- Flags: hardware-dependent
- Condition: bridge 使用 ports；8021q/8021ad 使用 ifname 和 vid。
- Apply impact: 改变设备类型会重建网桥或 VLAN，可能中断依赖它的接口。
- Evidence: `docs/field-help-network-research.md:78` (Xiaomi RN02 1.0.43) — 设备参数表将 type 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173584.
- Evidence: `docs/field-help-network-research.md:143` (Xiaomi RN02 1.0.43) — netifd 二进制包含 8021q 类型名称。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 162245.
- Evidence: `docs/field-help-network-research.md:144` (Xiaomi RN02 1.0.43) — netifd 二进制包含 8021ad 类型名称。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 162238.
- Evidence: `docs/field-help-network-research.md:165` (Xiaomi RN02 1.0.43) — 8021q 与 8021ad 对象绑定同一 VLAN 参数描述符和实现回调。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 177700.
- Evidence: `docs/field-help-network-research.md:183` (Xiaomi RN02 1.0.43) — device 章节读取type并选择设备实现，缺少name则不继续。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 65172.

#### network/device/ports

设置 bridge 的成员设备列表。网桥参数表明确将 ports 解析为数组，与逻辑接口的 IP 地址列表无关。

- Flags: hardware-dependent
- Condition: type=bridge；成员需是存在的设备，VLAN 成员标签另由 bridge-vlan 配置。
- Apply impact: 成员加入同一二层广播域；移除当前管理端口会断开连接。
- Evidence: `docs/field-help-network-research.md:109` (Xiaomi RN02 1.0.43) — 网桥参数表将 ports 声明为 array。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173852.
- Evidence: `docs/field-help-network-research.md:161` (Xiaomi RN02 1.0.43) — 网桥重配回调实际解析并保存 ports 参数。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 85052.

#### network/device/ifname

指定 802.1Q/802.1ad VLAN 的父设备。VLAN 参数表接受字符串，与旧式 interface.ifname 的逻辑接口成员列表不是同一层设置。

- Flags: hardware-dependent
- Condition: type=8021q 或 8021ad，并配合 vid。
- Apply impact: VLAN 流量在所选父设备上传送；父设备改错会让该 VLAN 的接口失去连接。
- Evidence: `docs/field-help-network-research.md:126` (Xiaomi RN02 1.0.43) — VLAN 参数表将 ifname 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 174208.
- Evidence: `docs/field-help-network-research.md:165` (Xiaomi RN02 1.0.43) — 8021q 与 8021ad 对象绑定同一 VLAN 参数描述符和实现回调。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 177700.

#### network/device/vid

设置 802.1Q/802.1ad VLAN 标签标识。源参数表以字符串解析 vid；面板数字输入只覆盖常规 VLAN 编号。

- Format / range: 常规可用 VLAN ID 为 1–4094；参数表未证明更窄的硬件范围。
- Flags: hardware-dependent
- Condition: type=8021q/8021ad 与父设备 ifname；上游需支持相同标签。
- Apply impact: 改变标签会让报文进入不同 VLAN；两端的标签约定必须一致。
- Evidence: `docs/field-help-network-research.md:127` (Xiaomi RN02 1.0.43) — VLAN 参数表将 vid 声明为 string，而不是整数。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 174216.
- Evidence: `docs/field-help-network-research.md:165` (Xiaomi RN02 1.0.43) — 8021q 与 8021ad 对象绑定同一 VLAN 参数描述符和实现回调。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 177700.

#### network/device/mtu

设置设备层最大传输单元。netifd 设备表将 mtu 作为整数接收；这不等同于 PPP 协议内部的 mru。

- Unit: 字节
- Format / range: 核心消费仅接受大于 67 的值；链路/硬件上限另行检查。
- Flags: hardware-dependent
- Condition: 受底层网卡、封装与上游链路限制；接口 mtu 另有独立配置。
- Apply impact: 会影响所有使用该设备的逻辑接口；路径不支持时可能出现大包丢失或分片。
- Evidence: `docs/field-help-network-research.md:79` (Xiaomi RN02 1.0.43) — 设备参数表将 mtu 声明为 int32。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173592.
- Evidence: `docs/field-help-network-research.md:164` (Xiaomi RN02 1.0.43) — 通用设备参数消费读取并应用 mtu 对应槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 72092.

#### network/device/macaddr

为设备层设置 MAC 地址覆盖。设备参数表接收字符串；厂商 LAN/WAN 接口的硬件地址回填不证明这里也有相同回退。

- Format / range: 原生 MAC 地址格式。
- Flags: hardware-dependent
- Condition: 设备驱动需允许地址修改；与接口层 macaddr 一起检查。
- Apply impact: 影响引用此设备的二层身份；重复 MAC 可能造成交换表抖动和连接异常。
- Evidence: `docs/field-help-network-research.md:81` (Xiaomi RN02 1.0.43) — 设备参数表将 macaddr 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173608.
- Evidence: `docs/field-help-network-research.md:164` (Xiaomi RN02 1.0.43) — 通用设备参数消费读取并应用 macaddr 对应槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 72092.

#### network/device/ipv6

控制此设备的 IPv6 能力，设备参数表接收布尔值。netifd 包含 per-device disable_ipv6 内核开关路径；这是设备层设置，不是 PPP 的 auto 模式。

- Condition: 内核 IPv6 支持及引用此设备的逻辑接口。
- Apply impact: 停用会影响该设备上的 IPv6 地址与相关接口；不会直接关闭 IPv4。
- Evidence: `docs/field-help-network-research.md:84` (Xiaomi RN02 1.0.43) — 设备参数表将 ipv6 声明为 bool/int8。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173632.
- Evidence: `docs/field-help-network-research.md:142` (Xiaomi RN02 1.0.43) — netifd 包含按设备写入 disable_ipv6 的 sysctl 路径。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 166234.
- Evidence: `docs/field-help-network-research.md:164` (Xiaomi RN02 1.0.43) — 通用设备参数消费读取并应用 ipv6 对应槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 72092.

#### network/device/stp

在 bridge 上控制生成树协议。网桥参数表接收布尔值，核心含 stp_state 写入目标；用于防止冗余二层链路形成环路。

- Source fallback: 0（网桥重配初始化值，未提供该项时保留）。
- Condition: type=bridge；拓扑中的其他交换设备也需考虑 STP。
- Apply impact: 启用可能改变端口转发状态及收敛时间；停用后不能依赖 STP 阻断环路。
- Evidence: `docs/field-help-network-research.md:110` (Xiaomi RN02 1.0.43) — 网桥参数表将 stp 声明为 bool/int8。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173860.
- Evidence: `docs/field-help-network-research.md:136` (Xiaomi RN02 1.0.43) — netifd 包含网桥 stp_state 内核属性路径。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 166274.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/network.lua:612–638` (Xiaomi RN02 1.0.43) — LuCI 网络模型从 brctl show 读取 STP 状态并记录 yes/false。
- Evidence: `docs/field-help-network-research.md:161` (Xiaomi RN02 1.0.43) — 网桥重配回调实际解析并保存 stp 参数。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 85052.
- Evidence: `docs/field-help-network-research.md:162` (Xiaomi RN02 1.0.43) — 网桥重配在覆盖参数前将 stp 初始化为 0。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 85116.

#### network/device/igmp_snooping

在网桥上根据 IGMP 成员关系限制 IPv4 组播转发。网桥表接受布尔值，核心含 multicast_snooping 属性路径。

- Source fallback: 0（网桥重配初始化值，未提供该项时保留）。
- Condition: type=bridge；与网络中的 IGMP 查询器、multicast_querier 及 IPTV 拓扑相关。
- Apply impact: 可减少无关端口的组播流量；成员信息或查询器缺失时可能影响 IPTV/组播接收。
- Evidence: `docs/field-help-network-research.md:113` (Xiaomi RN02 1.0.43) — 网桥参数表将 igmp_snooping 声明为 bool/int8。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173884.
- Evidence: `docs/field-help-network-research.md:139` (Xiaomi RN02 1.0.43) — netifd 包含网桥 multicast_snooping 属性路径。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 166372.
- Evidence: `docs/field-help-network-research.md:161` (Xiaomi RN02 1.0.43) — 网桥重配回调实际解析并保存 igmp_snooping 参数。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 85052.
- Evidence: `docs/field-help-network-research.md:162` (Xiaomi RN02 1.0.43) — 网桥重配在覆盖参数前将 igmp_snooping 初始化为 0。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 85116.

#### network/device/multicast_querier

控制网桥是否主动发送组播成员查询。参数表将此项作为布尔值接收，核心含 multicast_querier 内核属性路径。

- Source fallback: 未单独设置时：跟随 igmp_snooping；两者都未设置则为 0。
- Condition: type=bridge；主要与组播侦听、IGMP/MLD 查询有关。
- Apply impact: 可维持组播成员信息；与外部查询器的角色应协调，避免误判组播服务问题。
- Evidence: `docs/field-help-network-research.md:118` (Xiaomi RN02 1.0.43) — 网桥参数表将 multicast_querier 声明为 bool/int8。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173924.
- Evidence: `docs/field-help-network-research.md:140` (Xiaomi RN02 1.0.43) — netifd 包含网桥 multicast_querier 属性路径。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 166426.
- Evidence: `docs/field-help-network-research.md:161` (Xiaomi RN02 1.0.43) — 网桥重配回调实际解析并保存 multicast_querier 参数。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 85052.
- Evidence: `docs/field-help-network-research.md:163` (Xiaomi RN02 1.0.43) — igmp_snooping 同时赋给 querier，后续 multicast_querier 显式值可覆盖。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 85252.

#### network/device/bridge_empty

允许无成员时保持网桥存在。网桥参数表接受布尔值；访客网络脚本也生成同名选项，但在旧式 interface 章节中。

- Source fallback: 0（网桥重配初始化值，未提供该项时保留）。
- Condition: type=bridge；与 ports 或后续动态成员配合。
- Apply impact: 即使没有有线成员，桥设备仍可作为后续无线成员的接入点；不会自动增加成员。
- Evidence: `docs/field-help-network-research.md:117` (Xiaomi RN02 1.0.43) — 网桥参数表将 bridge_empty 声明为 bool/int8。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173916.
- Evidence: `usr/sbin/guestwifi.sh:87–91` (Xiaomi RN02 1.0.43) — 厂商访客 network 创建旧式 interface 网桥，并设 bridge_empty=1。
- Evidence: `docs/field-help-network-research.md:161` (Xiaomi RN02 1.0.43) — 网桥重配回调实际解析并保存 bridge_empty 参数。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 85052.
- Evidence: `docs/field-help-network-research.md:162` (Xiaomi RN02 1.0.43) — 网桥重配在覆盖参数前将 bridge_empty 初始化为 0。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 85116.

#### network/device/vlan_filtering

控制网桥是否按 VLAN 成员关系过滤报文。网桥参数表接受布尔值，核心含 vlan_filtering 内核属性路径。

- Source fallback: 0（网桥重配初始化值，未提供该项时保留）。
- Condition: type=bridge；与 bridge-vlan 的 VLAN、端口标签和 local 配合。
- Apply impact: 启用后缺失的 VLAN/端口成员可能被隔离，包含管理流量；关闭则不能依赖此桥实施 VLAN 隔离。
- Evidence: `docs/field-help-network-research.md:124` (Xiaomi RN02 1.0.43) — 网桥参数表将 vlan_filtering 声明为 bool/int8。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173972.
- Evidence: `docs/field-help-network-research.md:141` (Xiaomi RN02 1.0.43) — netifd 包含网桥 vlan_filtering 属性路径。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 167086.
- Evidence: `docs/field-help-network-research.md:161` (Xiaomi RN02 1.0.43) — 网桥重配回调实际解析并保存 vlan_filtering 参数。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 85052.
- Evidence: `docs/field-help-network-research.md:162` (Xiaomi RN02 1.0.43) — 网桥重配在覆盖参数前将 vlan_filtering 初始化为 0。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 85116.

#### network/device/ageing_time

设置动态学习的网桥 MAC 转发表项老化时间。网桥参数表接受整数，核心含 ageing_time 属性目标；不会设置 ARP 或 DHCP 租约期限。

- Unit: 秒
- Condition: type=bridge；仅影响动态学习的 MAC 表项。
- Apply impact: 值较短会更快忘记静默终端的端口并增加未知单播泛洪；较长保留旧端口映射更久。
- Evidence: `docs/field-help-network-research.md:114` (Xiaomi RN02 1.0.43) — 网桥参数表将 ageing_time 声明为 int32。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173892.
- Evidence: `docs/field-help-network-research.md:138` (Xiaomi RN02 1.0.43) — netifd 包含网桥 ageing_time 内核属性路径。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 167180.
- Evidence: `docs/field-help-network-research.md:161` (Xiaomi RN02 1.0.43) — 网桥重配回调实际解析并保存 ageing_time 参数。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 85052.
- Evidence: `docs/field-help-network-research.md:177` (Xiaomi RN02 1.0.43) — 实际写入前将 ageing_time 乘100，输入为秒。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 141060.

#### network/device/priority

设置网桥的 STP 桥优先级，不是 IP 路由优先级。网桥参数表接受整数，核心含 bridge/priority 属性路径。

- Source fallback: 32767（网桥重配初始化的 16 位值）。
- Format / range: 桥优先级为 16 位值（0–65535）；硬件/拓扑规则另行检查。
- Condition: type=bridge；启用 stp 后才参与生成树选举。
- Apply impact: 参与根桥选举；较小优先级可使该桥更容易成为根桥，可能改变二层路径。
- Evidence: `docs/field-help-network-research.md:112` (Xiaomi RN02 1.0.43) — 网桥参数表将 priority 声明为 int32。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173876.
- Evidence: `docs/field-help-network-research.md:137` (Xiaomi RN02 1.0.43) — netifd 包含网桥 priority 内核属性路径。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 167136.
- Evidence: `docs/field-help-network-research.md:161` (Xiaomi RN02 1.0.43) — 网桥重配回调实际解析并保存 priority 参数。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 85052.
- Evidence: `docs/field-help-network-research.md:162` (Xiaomi RN02 1.0.43) — priority 初始化指令写入16位32767，非运行配置采样。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 85116.

#### network/device/disabled

面板把此项解释为停用设备，但 RN02 设备参数表使用 enabled 布尔项，没有在该表找到 disabled。不能套用 interface 或无线 disabled 的效果。

- Compact help: 设备表使用 enabled；此 disabled 字段的停用效果未在 1.0.43 证实。
- Flags: version-dependent
- Apply impact: 保存会保留此原生字段；是否停用设备尚未证实，不能据此判断设备已停止。
- Trace result: 已检查 netifd 设备与网桥参数表、lib/network/config.sh、lib/miwifi 和反编译网络 Lua；未找到 device.disabled 的设备消费，已找到不同键 device.enabled。
- Evidence: `docs/field-help-network-research.md:83` (Xiaomi RN02 1.0.43) — 设备参数表的启用键为 enabled，类型为 bool/int8。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173624.
- Evidence: `etc/init.d/network:34` (Xiaomi RN02 1.0.43) — 网络设备由 netifd 服务管理。

### bridge-vlan

#### network/bridge-vlan/device

指定此 VLAN 条目所属网桥。核心在 bridge-vlan 分支按 device 名查找设备对象，并检查其网桥状态后读取 VLAN 成员参数。

- Condition: 应引用 type=bridge 的设备；桥需配置相应端口及 VLAN 过滤。
- Apply impact: 绑定到错误网桥会让 VLAN 规则不作用于预期端口；可能影响管理流量的 VLAN 归属。
- Evidence: `docs/field-help-network-research.md:135` (Xiaomi RN02 1.0.43) — netifd 二进制包含 bridge-vlan 类型名称。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 159091.
- Evidence: `docs/field-help-network-research.md:74` (Xiaomi RN02 1.0.43) — bridge-vlan 参数表接收 VLAN 编号。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173552.
- Evidence: `etc/init.d/network:34` (Xiaomi RN02 1.0.43) — 网络服务启动 netifd。
- Evidence: `docs/field-help-network-research.md:178` (Xiaomi RN02 1.0.43) — bridge-vlan 消费按 device 名查找并要求网桥设备。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 67432.

#### network/bridge-vlan/vlan

设置桥接 VLAN 成员条目的 VLAN 编号。netifd 的 bridge-vlan 表以整数解析 vlan；它不是旧交换芯片的 VLAN 表索引。

- Format / range: 核心接受 1–4095；常规业务 VLAN ID 为 1–4094，4095 为协议保留值。
- Condition: 关联 device 必须是桥；与 ports、local 及桥 vlan_filtering 配合。
- Apply impact: 相同编号的成员共享该 VLAN；改动会改变端口和本机收发的二层隔离。
- Evidence: `docs/field-help-network-research.md:74` (Xiaomi RN02 1.0.43) — bridge-vlan 参数表将 vlan 声明为 int32。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173552.
- Evidence: `docs/field-help-network-research.md:172` (Xiaomi RN02 1.0.43) — 真实桥接 VLAN 消费解析 vlan。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 67564.

#### network/bridge-vlan/ports

设置桥接 VLAN 的端口列表及标签方式。核心按冒号拆标记：缺省去标签，t 改为带标签，* 设为端口主 VLAN；u 不额外改变缺省去标签状态。

- Source fallback: 端口未写标签标记：去标签；没有自动成员端口回退。
- Flags: hardware-dependent
- Condition: 成员必须属于目标桥；与 VLAN 编号及对端标签方式匹配。
- Apply impact: 错误标签或主 VLAN 会让终端进入错误网络，或让其无法接收预期报文。
- Evidence: `docs/field-help-network-research.md:76` (Xiaomi RN02 1.0.43) — bridge-vlan 参数表将 ports 声明为 array。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173568.
- Evidence: `docs/field-help-network-research.md:172` (Xiaomi RN02 1.0.43) — 真实桥接 VLAN 消费解析 ports。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 67564.
- Evidence: `docs/field-help-network-research.md:173` (Xiaomi RN02 1.0.43) — 核心按冒号拆端口标记：缺省untagged，t设tagged，*设PVID。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 68020.

#### network/bridge-vlan/local

控制路由器本机是否参与此桥接 VLAN。bridge-vlan 参数表将 local 解析为布尔值，与端口间转发设置分开。

- Source fallback: 1（创建 bridge-vlan 条目时未提供 local 的初始化值）。
- Condition: 与桥 VLAN 及本机 VLAN 子接口配合；不等同于防火墙 input 策略。
- Apply impact: 关闭本机参与可能让该 VLAN 的终端不能访问路由器服务；成员端口之间仍由桥规则决定转发。
- Evidence: `docs/field-help-network-research.md:75` (Xiaomi RN02 1.0.43) — bridge-vlan 参数表将 local 声明为 bool/int8。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173560.
- Evidence: `docs/field-help-network-research.md:172` (Xiaomi RN02 1.0.43) — 真实桥接 VLAN 消费解析 local。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 67564.

### switch

#### network/switch/name

选择 swconfig 要加载的交换芯片。setup_switch_dev 先读 name，空值使用章节名；若同名网卡存在，先将其启用再加载 network。

- Source fallback: 未设置时：该 switch 章节名称。
- Flags: legacy, hardware-dependent
- Condition: 设备必须可由 swconfig 驱动识别。
- Apply impact: 选错芯片会把配置送到错误设备；此路径影响物理端口的转发。
- Evidence: `lib/network/switch.sh:4–9` (Xiaomi RN02 1.0.43) — setup_switch_dev 从章节读 name，空值回退为章节名；先启用同名网卡再运行 swconfig dev name load network。

#### network/switch/reset

在加载旧式交换芯片配置时请求重置。RN02 的 network 校验将其定义为布尔值，实际选项由 swconfig 和芯片驱动应用。

- Flags: legacy, hardware-dependent
- Condition: 旧式 swconfig 交换芯片及其驱动提供 reset 选项。
- Apply impact: 重置可能短暂中断芯片端口的流量并清理运行状态；不等同于路由器恢复出厂设置。
- Evidence: `etc/init.d/network:134` (Xiaomi RN02 1.0.43) — network switch 校验声明 reset 为 bool。
- Evidence: `lib/network/switch.sh:9` (Xiaomi RN02 1.0.43) — 交换芯片配置交给 swconfig dev name load network。

#### network/switch/enable_vlan

请求交换芯片启用 VLAN 隔离。network 的 switch 校验接受布尔值，随后由 swconfig 加载整个 network 配置。

- Flags: legacy, hardware-dependent
- Condition: swconfig 驱动需支持 enable_vlan；与 switch_vlan 配合。
- Apply impact: 影响物理端口隔离；切换后需让 switch_vlan 成员与 CPU 口正确匹配，否则会失去连接。
- Evidence: `etc/init.d/network:133` (Xiaomi RN02 1.0.43) — network switch 校验声明 enable_vlan 为 bool。
- Evidence: `lib/network/switch.sh:9` (Xiaomi RN02 1.0.43) — setup_switch_dev 运行 swconfig dev name load network。

#### network/switch/enable_mirror_rx

面板将此项解释为接收流量镜像开关。交换芯片配置经 swconfig 转交驱动；RN02 的脚本校验和厂商端口配置未声明此镜像键。

- Compact help: 接收镜像请求；RN02 静态消费链未证实驱动支持此键。
- Flags: legacy, hardware-dependent, version-dependent
- Condition: 需芯片驱动支持，并配合镜像来源和监控端口。
- Apply impact: 若驱动实现此键，会把接收报文复制到监控端口，增加端口负载并暴露流量副本；当前支持未证实。
- Trace result: 已搜索 etc/init.d/network、lib/network/switch.sh、lib/miwifi 的端口/芯片脚本及全部可读 Lua；未找到 switch.enable_mirror_rx 的读写。swconfig 动态驱动属性仍可能提供此键，未在线查询。
- Evidence: `lib/network/switch.sh:9` (Xiaomi RN02 1.0.43) — 交换芯片选项经 swconfig load network 下发。
- Evidence: `etc/init.d/network:130–136` (Xiaomi RN02 1.0.43) — switch 校验只显式声明 name、enable、enable_vlan、reset 及 MIB 选项。

#### network/switch/enable_mirror_tx

面板将此项解释为发送流量镜像开关。当前可见 swconfig 加载链未单独声明或读取此镜像键，效果由芯片驱动决定。

- Compact help: 发送镜像请求；此芯片驱动是否接受该键尚未静态证实。
- Flags: legacy, hardware-dependent, version-dependent
- Condition: 需驱动支持，并配合 mirror_source_port、mirror_monitor_port。
- Apply impact: 若驱动支持，会复制发送流量到监控端口，可能占用监控链路带宽；未证明 RN02 会启用该功能。
- Trace result: 已搜索 network init、lib/network/switch.sh、厂商 lib/miwifi 端口脚本及反编译 Lua；未找到 switch.enable_mirror_tx 的具体消费。未运行 swconfig 或读取运行时属性。
- Evidence: `lib/network/switch.sh:9` (Xiaomi RN02 1.0.43) — 交换芯片选项由 swconfig load network 交给驱动。
- Evidence: `etc/init.d/network:130–136` (Xiaomi RN02 1.0.43) — 可见 switch 校验声明不包含镜像发送开关。

#### network/switch/mirror_source_port

面板语义为被镜像的芯片端口编号，不是 Linux 网卡序号。静态加载链未证明 RN02 驱动读取此字段。

- Compact help: 芯片镜像来源端口；实际支持和端口编号范围未证实。
- Flags: legacy, hardware-dependent, version-dependent
- Condition: 依赖芯片端口映射、镜像方向开关及驱动属性。
- Apply impact: 若驱动接受，会决定哪一端口的流量被复制；编号错误可能监控到其他端口或无数据。
- Trace result: 已检查 switch 加载、network 校验、lib/miwifi/lib_port_map.sh 及架构端口脚本和反编译 Lua；未找到 switch.mirror_source_port 的解析或应用，不能提供硬件范围。
- Evidence: `lib/network/switch.sh:9` (Xiaomi RN02 1.0.43) — 交换芯片配置通过 swconfig load network 应用。
- Evidence: `etc/init.d/network:130–136` (Xiaomi RN02 1.0.43) — switch 的脚本校验声明未包含 mirror_source_port。

#### network/switch/mirror_monitor_port

面板语义为接收镜像副本的芯片监控端口。静态消费者未证明该键是否由 RN02 驱动提供，不能按普通 LAN 口序号推定。

- Compact help: 芯片镜像监控端口；实际支持和硬件编号范围未证实。
- Flags: legacy, hardware-dependent, version-dependent
- Condition: 需镜像方向、来源端口及芯片驱动支持。
- Apply impact: 若驱动支持，此端口会收到流量副本；监控口承载业务时可能出现额外负载或流量泄露。
- Trace result: 已搜索 lib/network/switch.sh、network init、lib/miwifi 端口脚本和全部可读 Lua；未定位 switch.mirror_monitor_port 的读取或硬件属性表，未在线探测端口。
- Evidence: `lib/network/switch.sh:9` (Xiaomi RN02 1.0.43) — swconfig 接收整个 network 的交换芯片配置。
- Evidence: `etc/init.d/network:130–136` (Xiaomi RN02 1.0.43) — switch 的脚本校验未列 mirror_monitor_port。

### switch_vlan

#### network/switch_vlan/device

指定旧式 swconfig VLAN 条目所属交换芯片。厂商端口映射会生成 switch_vlan 章节并把芯片名称写入 device。

- Flags: legacy, generated, hardware-dependent
- Condition: 名称需对应 switch 章节中的交换芯片。
- Apply impact: 条目送给所选芯片；名称错误会使成员划分不作用于预期端口。
- Evidence: `etc/init.d/network:142` (Xiaomi RN02 1.0.43) — switch_vlan 校验将 device 声明为 string。
- Evidence: `lib/miwifi/lib_port_map.sh:54–61` (Xiaomi RN02 1.0.43) — 厂商 VLAN 生成函数创建 switch_vlan 并写入 device、ports、vlan、vid。

#### network/switch_vlan/vlan

设置旧式交换芯片的 VLAN 表条目编号，不应直接当作 bridge-vlan 的标签号。厂商生成路径恰好把 vlan 和 vid 写为同一值，但二者仍是独立键。

- Format / range: 非负整数；芯片表容量范围未在该脚本声明。
- Flags: legacy, generated, hardware-dependent
- Condition: 与 device、vid、ports 配合；由 swconfig 驱动解释表索引。
- Apply impact: 决定加载哪条 VLAN 表记录；改动可能改变物理端口和 CPU 口间的连通性。
- Evidence: `etc/init.d/network:143` (Xiaomi RN02 1.0.43) — switch_vlan 校验把 vlan 声明为 uinteger。
- Evidence: `lib/miwifi/lib_port_map.sh:58–59` (Xiaomi RN02 1.0.43) — 厂商通用生成路径把 vlan 和 vid 同时设置为函数参数 vid。

#### network/switch_vlan/vid

设置交换芯片 VLAN 表记录的实际标签标识。厂商生成代码单独写入 vid；RN02 架构脚本还保留一个 vid=0 的内部条目，面板范围不是固件全部内部用途。

- Format / range: 常规业务 VLAN ID 为 1–4094；厂商内部脚本另使用 0。
- Flags: legacy, generated, hardware-dependent
- Condition: 与 vlan 表索引、端口成员、CPU 口的网卡 VLAN 标签配合。
- Apply impact: 标签与父网卡 VLAN 子接口需一致；修改可能隔离物理端口或上游链路。
- Evidence: `lib/miwifi/lib_port_map.sh:59` (Xiaomi RN02 1.0.43) — 厂商 switch_vlan 生成路径单独写入 vid。
- Evidence: `lib/miwifi/arch/lib_arch_port_map.sh:48–55` (Xiaomi RN02 1.0.43) — 架构脚本生成 vlan0，设置 vlan=0、vid=0 和内部成员端口。

#### network/switch_vlan/ports

设置旧式交换芯片的端口编号列表及标签标记。network 校验要求 list(ports)，厂商映射还会把 CPU 口加入成员列表。

- Flags: legacy, generated, hardware-dependent
- Condition: 端口编号由芯片/板级映射决定；t 等原生标签格式需保留。
- Apply impact: 端口成员决定 VLAN 可达性；漏掉 CPU 口可能让终端不能访问路由器或上网。
- Evidence: `etc/init.d/network:144` (Xiaomi RN02 1.0.43) — switch_vlan 校验把 ports 声明为 list(ports)。
- Evidence: `lib/miwifi/lib_port_map.sh:143–148` (Xiaomi RN02 1.0.43) — 厂商 VLAN 生成收集成员端口，并追加 cpu_port 后配置 VLAN。

### route

#### network/route/interface

选择此 IPv4 静态路由的出口逻辑接口。netifd 路由表接收字符串；填写 network 中的接口名称而不是物理端口编号。

- Condition: 逻辑接口必须存在；网关应由该接口可达。
- Apply impact: 该接口提供 IPv4 路由的承载设备；接口不存在或未就绪时，目标路径可能不可用。
- Evidence: `etc/init.d/network:78` (Xiaomi RN02 1.0.43) — route 校验将 interface 声明为 string。
- Evidence: `docs/field-help-network-research.md:40` (Xiaomi RN02 1.0.43) — 核心路由参数表将 interface 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172852.
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 interface 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.

#### network/route/target

设置 IPv4 路由匹配的目标网络或主机前缀。前缀越长，匹配越具体；不是路由器本机地址。

- Format / range: IPv4 地址或 CIDR；前缀长度 0–32。
- Condition: IPv4 分开填写地址时可配 netmask；IPv6 使用自身前缀长度。
- Apply impact: 改变哪些 IPv4 目标走此路径；覆盖当前管理目标的路由可能改变回程。
- Evidence: `etc/init.d/network:79` (Xiaomi RN02 1.0.43) — route 的 target 校验要求 cidr4。
- Evidence: `docs/field-help-network-research.md:41` (Xiaomi RN02 1.0.43) — 核心路由参数表将 target 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172860.
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 target 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.

#### network/route/gateway

设置 IPv4 静态路由的下一跳。核心参数表接收 gateway 字符串，源校验限制对应地址族。

- Format / range: IPv4 网关地址。
- Condition: 下一跳需由 interface 到达；特殊链路外网关可检查 onlink。
- Apply impact: 报文经该下一跳离开接口；错误或不可达的网关会使匹配的 IPv4 目的地失联。
- Evidence: `etc/init.d/network:81` (Xiaomi RN02 1.0.43) — route gateway 校验要求 ip4addr。
- Evidence: `docs/field-help-network-research.md:43` (Xiaomi RN02 1.0.43) — 核心路由参数表将 gateway 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172876.
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 gateway 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.

#### network/route/metric

为此 IPv4 路由设置非负度量。相同目标和前缀长度的多条路由可用度量区分优先级。

- Format / range: 非负整数。
- Condition: 只有在目标/前缀等路由条件相当时比较；与路由表选择独立。
- Apply impact: 影响同目标 IPv4 路由的选择；不会改变策略规则的查表优先级。
- Evidence: `etc/init.d/network:82` (Xiaomi RN02 1.0.43) — route metric 校验要求 uinteger。
- Evidence: `docs/field-help-network-research.md:44` (Xiaomi RN02 1.0.43) — 核心路由参数表将 metric 声明为 int32。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172884.
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 metric 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.

#### network/route/table

把此 IPv4 路由放入指定路由表。可用表编号或名称；路由表名称需在系统中能够解析。

- Format / range: 源校验：数字 0–65535 或表名称。
- Condition: 与 rule.lookup、内核查表规则及其他表中的路由配合。
- Apply impact: 不在当前查询链中的表不会自动成为 IPv4 默认出口；策略 rule/rule6 可通过 lookup 使用它。
- Evidence: `etc/init.d/network:84` (Xiaomi RN02 1.0.43) — route table 校验允许 0–65535 的数字或字符串名称。
- Evidence: `docs/field-help-network-research.md:47` (Xiaomi RN02 1.0.43) — 核心路由参数表将 table 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172908.
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 table 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.

#### network/route/type

选择此 IPv4 路由的动作类型。unicast 指向可转发路径；blackhole 静默丢弃、unreachable 返回不可达、prohibit 表示禁止；throw 让查询继续其他规则。

- Format / range: unicast / local / blackhole / unreachable / prohibit / throw（面板保留项）。
- Condition: type=local 表示本机目的地址，不是普通出口；网关参数只适用于相应路由类型。
- Apply impact: 特殊类型可以有意阻断或跳过 IPv4 路径，不能都按普通网关路由理解。
- Trace result: 已定位 route.type 的真实转换调用及多种特殊类型名称；本轮未逐项记录全部保留类型的数值和未设置回退，不提供统一缺省。
- Evidence: `docs/field-help-network-research.md:50` (Xiaomi RN02 1.0.43) — 核心路由参数表将 type 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172932.
- Evidence: `docs/field-help-network-research.md:147` (Xiaomi RN02 1.0.43) — netifd 二进制包含 blackhole 路由/动作类型名称。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 165308.
- Evidence: `docs/field-help-network-research.md:150` (Xiaomi RN02 1.0.43) — netifd 二进制包含 throw 路由/动作类型名称。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 165327.
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 type 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.
- Evidence: `docs/field-help-network-research.md:182` (Xiaomi RN02 1.0.43) — 实际路由type通过核心类型转换函数，失败进入日志错误路径。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 43272.
- Evidence: `docs/field-help-network-research.md:184` (Xiaomi RN02 1.0.43) — 真实路由type转换函数比较特殊类型名称并转换为内核类型数值。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 126876.

#### network/route/source

设置此路由的来源地址或来源前缀参数。核心按路由地址族解析来源地址，并保存可选前缀长度；不能拿它替代策略规则的 src 条件。

- Condition: 与接口已有地址及同族路由配合；不能替代 rule/rule6.src。
- Apply impact: 可能影响 IPv4 源地址选择或源相关路由条件；错误源参数会破坏回程或查表匹配。
- Trace result: 已定位 route.source 的地址及来源前缀解析；最终在 IPv4/IPv6 中映射为内核首选来源地址还是源相关路由的差异尚未逐项核对，因此不承诺两族等价。
- Evidence: `docs/field-help-network-research.md:48` (Xiaomi RN02 1.0.43) — 核心路由参数表将 source 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172916.
- Evidence: `lib/netifd/netifd-proto.sh:313` (Xiaomi RN02 1.0.43) — 协议路由更新在 source 非空时向核心提交 source 字符串。
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 source 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.
- Evidence: `docs/field-help-network-research.md:180` (Xiaomi RN02 1.0.43) — 静态 route.source 地址按族解析，保存可选来源前缀长度；无/时IPv4为32、IPv6为128。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42936.

#### network/route/mtu

为此 IPv4 路由设置路径 MTU。源校验接受非负整数；这是路由层属性，不是网卡设备或 PPP 接收单元设置。

- Unit: 字节
- Condition: 受底层设备及真实路径 MTU 限制。
- Apply impact: 限制匹配此路由的 IPv4 报文尺寸；太小降低效率，太大可能导致路径 MTU 故障。
- Evidence: `etc/init.d/network:83` (Xiaomi RN02 1.0.43) — route mtu 校验要求 uinteger。
- Evidence: `docs/field-help-network-research.md:45` (Xiaomi RN02 1.0.43) — 核心路由参数表将 mtu 声明为 int32。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172892.
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 mtu 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.

#### network/route/onlink

把此 IPv4 下一跳视为直连，即使它不落在已配置的接口前缀内。核心路由表明确接收布尔参数 onlink。

- Condition: 需要 gateway 和正确的 interface；链路层仍必须能到达下一跳。
- Apply impact: 允许特殊下一跳的路由安装，但不会让本来无法到达的 IPv4 网关自动可达。
- Evidence: `docs/field-help-network-research.md:49` (Xiaomi RN02 1.0.43) — 核心路由参数表将 onlink 声明为 bool/int8。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172924.
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 onlink 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.
- Evidence: `docs/field-help-network-research.md:181` (Xiaomi RN02 1.0.43) — onlink 非零给实际路由对象设置独立标记。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42980.

#### network/route/disabled

标记此 IPv4 静态路由不使用。核心路由参数表单独声明 disabled 布尔项；不会停用其关联接口。

- Condition: 仅作用于该路由；可与其他表或动态路由并存。
- Apply impact: 停用后流量只能使用其他匹配路由；若无替代路径，相应 IPv4 目标会不可达。
- Evidence: `docs/field-help-network-research.md:52` (Xiaomi RN02 1.0.43) — 核心路由参数表将 disabled 声明为 bool/int8。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172948.
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 disabled 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.

#### network/route/netmask

设置 IPv4 target 的掩码，用于确定静态路由匹配范围；如果 target 已用 CIDR，还应保持两者表达一致。

- Format / range: IPv4 掩码，等价前缀长度为 0–32。
- Condition: 与 IPv4 target 配合；不适用于 route6。
- Apply impact: 掩码过宽可能截走其他网段的流量；过窄则仅匹配部分目标。
- Evidence: `etc/init.d/network:80` (Xiaomi RN02 1.0.43) — route 的 netmask 校验要求 netmask4。
- Evidence: `docs/field-help-network-research.md:42` (Xiaomi RN02 1.0.43) — 核心路由参数表将 netmask 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172868.

### route6

#### network/route6/interface

选择此 IPv6 静态路由的出口逻辑接口。netifd 路由表接收字符串；填写 network 中的接口名称而不是物理端口编号。

- Condition: 逻辑接口必须存在；网关应由该接口可达。
- Apply impact: 该接口提供 IPv6 路由的承载设备；接口不存在或未就绪时，目标路径可能不可用。
- Evidence: `etc/init.d/network:90` (Xiaomi RN02 1.0.43) — route6 校验将 interface 声明为 string。
- Evidence: `docs/field-help-network-research.md:40` (Xiaomi RN02 1.0.43) — 核心路由参数表将 interface 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172852.
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 interface 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.

#### network/route6/target

设置 IPv6 路由匹配的目标网络或主机前缀。前缀越长，匹配越具体；不是路由器本机地址。

- Format / range: IPv6 地址或 CIDR；前缀长度 0–128。
- Condition: IPv6 无单独 netmask 字段；前缀写入 target。
- Apply impact: 改变哪些 IPv6 目标走此路径；覆盖当前管理目标的路由可能改变回程。
- Evidence: `etc/init.d/network:91` (Xiaomi RN02 1.0.43) — route6 的 target 校验要求 cidr6。
- Evidence: `docs/field-help-network-research.md:41` (Xiaomi RN02 1.0.43) — 核心路由参数表将 target 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172860.
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 target 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.

#### network/route6/gateway

设置 IPv6 静态路由的下一跳。核心参数表接收 gateway 字符串，源校验限制对应地址族。

- Format / range: IPv6 网关地址。
- Condition: 下一跳需由 interface 到达；特殊链路外网关可检查 onlink。
- Apply impact: 报文经该下一跳离开接口；错误或不可达的网关会使匹配的 IPv6 目的地失联。
- Evidence: `etc/init.d/network:92` (Xiaomi RN02 1.0.43) — route6 gateway 校验要求 ip6addr。
- Evidence: `docs/field-help-network-research.md:43` (Xiaomi RN02 1.0.43) — 核心路由参数表将 gateway 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172876.
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 gateway 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.

#### network/route6/metric

为此 IPv6 路由设置非负度量。相同目标和前缀长度的多条路由可用度量区分优先级。

- Format / range: 非负整数。
- Condition: 只有在目标/前缀等路由条件相当时比较；与路由表选择独立。
- Apply impact: 影响同目标 IPv6 路由的选择；不会改变策略规则的查表优先级。
- Evidence: `etc/init.d/network:93` (Xiaomi RN02 1.0.43) — route6 metric 校验要求 uinteger。
- Evidence: `docs/field-help-network-research.md:44` (Xiaomi RN02 1.0.43) — 核心路由参数表将 metric 声明为 int32。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172884.
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 metric 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.

#### network/route6/table

把此 IPv6 路由放入指定路由表。可用表编号或名称；路由表名称需在系统中能够解析。

- Format / range: 源校验：数字 0–65535 或表名称。
- Condition: 与 rule6.lookup、内核查表规则及其他表中的路由配合。
- Apply impact: 不在当前查询链中的表不会自动成为 IPv6 默认出口；策略 rule/rule6 可通过 lookup 使用它。
- Evidence: `etc/init.d/network:95` (Xiaomi RN02 1.0.43) — route6 table 校验允许 0–65535 的数字或字符串名称。
- Evidence: `docs/field-help-network-research.md:47` (Xiaomi RN02 1.0.43) — 核心路由参数表将 table 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172908.
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 table 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.

#### network/route6/type

选择此 IPv6 路由的动作类型。unicast 指向可转发路径；blackhole 静默丢弃、unreachable 返回不可达、prohibit 表示禁止；throw 让查询继续其他规则。

- Format / range: unicast / local / blackhole / unreachable / prohibit / throw（面板保留项）。
- Condition: type=local 表示本机目的地址，不是普通出口；网关参数只适用于相应路由类型。
- Apply impact: 特殊类型可以有意阻断或跳过 IPv6 路径，不能都按普通网关路由理解。
- Trace result: 已定位 route6.type 的真实转换调用及多种特殊类型名称；本轮未逐项记录全部保留类型的数值和未设置回退，不提供统一缺省。
- Evidence: `docs/field-help-network-research.md:50` (Xiaomi RN02 1.0.43) — 核心路由参数表将 type 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172932.
- Evidence: `docs/field-help-network-research.md:147` (Xiaomi RN02 1.0.43) — netifd 二进制包含 blackhole 路由/动作类型名称。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 165308.
- Evidence: `docs/field-help-network-research.md:150` (Xiaomi RN02 1.0.43) — netifd 二进制包含 throw 路由/动作类型名称。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 165327.
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 type 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.
- Evidence: `docs/field-help-network-research.md:182` (Xiaomi RN02 1.0.43) — 实际路由type通过核心类型转换函数，失败进入日志错误路径。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 43272.
- Evidence: `docs/field-help-network-research.md:184` (Xiaomi RN02 1.0.43) — 真实路由type转换函数比较特殊类型名称并转换为内核类型数值。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 126876.

#### network/route6/source

设置此路由的来源地址或来源前缀参数。核心按路由地址族解析来源地址，并保存可选前缀长度；不能拿它替代策略规则的 src 条件。

- Condition: 与接口已有地址及同族路由配合；不能替代 rule/rule6.src。
- Apply impact: 可能影响 IPv6 源地址选择或源相关路由条件；错误源参数会破坏回程或查表匹配。
- Trace result: 已定位 route6.source 的地址及来源前缀解析；最终在 IPv4/IPv6 中映射为内核首选来源地址还是源相关路由的差异尚未逐项核对，因此不承诺两族等价。
- Evidence: `docs/field-help-network-research.md:48` (Xiaomi RN02 1.0.43) — 核心路由参数表将 source 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172916.
- Evidence: `lib/netifd/netifd-proto.sh:313` (Xiaomi RN02 1.0.43) — 协议路由更新在 source 非空时向核心提交 source 字符串。
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 source 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.
- Evidence: `docs/field-help-network-research.md:180` (Xiaomi RN02 1.0.43) — 静态 route.source 地址按族解析，保存可选来源前缀长度；无/时IPv4为32、IPv6为128。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42936.

#### network/route6/mtu

为此 IPv6 路由设置路径 MTU。源校验接受非负整数；这是路由层属性，不是网卡设备或 PPP 接收单元设置。

- Unit: 字节
- Condition: 受真实路径 MTU 限制；IPv6 链路正常使用还需考虑 1280 字节最低 MTU。
- Apply impact: 限制匹配此路由的 IPv6 报文尺寸；太小降低效率，太大可能导致路径 MTU 故障。
- Evidence: `etc/init.d/network:94` (Xiaomi RN02 1.0.43) — route6 mtu 校验要求 uinteger。
- Evidence: `docs/field-help-network-research.md:45` (Xiaomi RN02 1.0.43) — 核心路由参数表将 mtu 声明为 int32。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172892.
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 mtu 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.

#### network/route6/onlink

把此 IPv6 下一跳视为直连，即使它不落在已配置的接口前缀内。核心路由表明确接收布尔参数 onlink。

- Condition: 需要 gateway 和正确的 interface；链路层仍必须能到达下一跳。
- Apply impact: 允许特殊下一跳的路由安装，但不会让本来无法到达的 IPv6 网关自动可达。
- Evidence: `docs/field-help-network-research.md:49` (Xiaomi RN02 1.0.43) — 核心路由参数表将 onlink 声明为 bool/int8。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172924.
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 onlink 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.
- Evidence: `docs/field-help-network-research.md:181` (Xiaomi RN02 1.0.43) — onlink 非零给实际路由对象设置独立标记。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42980.

#### network/route6/disabled

标记此 IPv6 静态路由不使用。核心路由参数表单独声明 disabled 布尔项；不会停用其关联接口。

- Condition: 仅作用于该路由；可与其他表或动态路由并存。
- Apply impact: 停用后流量只能使用其他匹配路由；若无替代路径，相应 IPv6 目标会不可达。
- Evidence: `docs/field-help-network-research.md:52` (Xiaomi RN02 1.0.43) — 核心路由参数表将 disabled 声明为 bool/int8。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 172948.
- Evidence: `docs/field-help-network-research.md:170` (Xiaomi RN02 1.0.43) — 真实路由消费解析 disabled 参数槽。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 42256.

### rule

#### network/rule/in

按入站逻辑接口匹配此 IPv4 策略规则。源校验接收字符串；它筛选报文进入的接口，不是设置出口。

- Condition: 需已有 network 逻辑接口；与 out、src/dest 等条件一起匹配。
- Apply impact: 只让来自该接口的 IPv4 流量命中此规则；名称错误可能使规则不匹配。
- Evidence: `etc/init.d/network:101` (Xiaomi RN02 1.0.43) — rule 校验声明 in 为 string。
- Evidence: `docs/field-help-network-research.md:53` (Xiaomi RN02 1.0.43) — 策略规则参数表将 in 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173028.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 in 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule/out

按出站逻辑接口匹配此 IPv4 策略规则，不直接指定转发网卡。路由出口仍由 lookup 或 action 决定。

- Condition: 需已有逻辑接口及对应方向的规则条件。
- Apply impact: 限制此规则匹配的 IPv4 流量范围；不要用此项替代静态路由的 interface。
- Evidence: `etc/init.d/network:102` (Xiaomi RN02 1.0.43) — rule 校验声明 out 为 string。
- Evidence: `docs/field-help-network-research.md:54` (Xiaomi RN02 1.0.43) — 策略规则参数表将 out 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173036.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 out 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule/src

按 IPv4 来源地址或前缀匹配规则。来源条件筛选发送者，与路由条目 source 的源地址选择参数不同。

- Format / range: IPv4 CIDR 前缀；前缀长度 0–32。
- Condition: 与 dest、in/out、mark 一起匹配；invert 可反转条件。
- Apply impact: 决定哪些 IPv4 发送者使用本规则；过宽前缀可能把无关终端引向同一出口或阻断动作。
- Evidence: `etc/init.d/network:103` (Xiaomi RN02 1.0.43) — rule src 校验要求 cidr4。
- Evidence: `docs/field-help-network-research.md:56` (Xiaomi RN02 1.0.43) — 策略规则参数表将 src 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173052.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 src 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule/dest

按 IPv4 目标地址或前缀匹配规则。它选择要处理的目标流量，不会创建该目标的路由。

- Format / range: IPv4 CIDR 前缀。
- Condition: 与 lookup 表内的路由、优先级及其他匹配条件配合。
- Apply impact: 决定访问哪些 IPv4 目的地时触发此查表或动作；对应表仍需有可用路由。
- Evidence: `etc/init.d/network:104` (Xiaomi RN02 1.0.43) — rule dest 校验要求 cidr4。
- Evidence: `docs/field-help-network-research.md:57` (Xiaomi RN02 1.0.43) — 策略规则参数表将 dest 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173060.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 dest 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule/priority

设置此 IPv4 策略规则的处理顺序，数值较小先处理。核心规则表接收整数；此项不同于路由 metric。

- Format / range: 非负整数；源参数表未声明硬件式上限。
- Condition: 应与其他内核规则协调；goto 使用此优先级作为跳转目标。
- Apply impact: 可以让该规则先于其他 IPv4 查表规则生效，改变出口或阻断结果。
- Evidence: `docs/field-help-network-research.md:58` (Xiaomi RN02 1.0.43) — 策略规则参数表将 priority 声明为 int32。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173068.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 priority 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule/lookup

匹配后查询指定 IPv4 路由表。可填写数字编号或名称；它不会自动把路由加入该表。

- Format / range: 源校验：数字 0–65535 或路由表名称。
- Condition: 配合 route.table 和该表的有效路由；与特殊 action 的组合需核对。
- Apply impact: 使匹配的 IPv4 流量查不同的路由表；表内无有效路径时，后续结果取决于其他规则。
- Evidence: `etc/init.d/network:108` (Xiaomi RN02 1.0.43) — rule lookup 校验允许 0–65535 或字符串。
- Evidence: `docs/field-help-network-research.md:61` (Xiaomi RN02 1.0.43) — 策略规则参数表将 lookup 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173092.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 lookup 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule/mark

按数据包标记匹配此 IPv4 规则。使用原生标记及可选掩码；本规则只匹配标记，不会主动给报文打标。

- Condition: 需防火墙、QoS 或其他路径先设置 packet mark；保留原生值/掩码格式。
- Apply impact: 仅已被其他机制标记的 IPv4 流量会走此规则；错误掩码可能扩大或缩小匹配范围。
- Evidence: `etc/init.d/network:106` (Xiaomi RN02 1.0.43) — rule mark 校验接收 string。
- Evidence: `docs/field-help-network-research.md:60` (Xiaomi RN02 1.0.43) — 策略规则参数表将 mark 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173084.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 mark 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule/invert

反转此 IPv4 策略规则的匹配结果。源校验与核心参数表均声明布尔值；它反转条件，不反转查表结果。

- Condition: 需一起检查 src、dest、in、out、mark 等条件。
- Apply impact: 原本不匹配的 IPv4 流量可能转为命中，尤其宽泛条件下会影响更多连接。
- Evidence: `etc/init.d/network:107` (Xiaomi RN02 1.0.43) — rule invert 校验要求 bool。
- Evidence: `docs/field-help-network-research.md:55` (Xiaomi RN02 1.0.43) — 策略规则参数表将 invert 声明为 bool/int8。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173044.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 invert 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule/action

设置 IPv4 策略规则的特殊动作。基线校验明确列出 prohibit、unreachable、blackhole、throw；面板另保留 unicast，不能据此断定它是固件可直接使用的动作。

- Compact help: 源校验支持禁止、不可达、黑洞与继续；普通查表用 lookup，unicast 未验证。
- Format / range: 基线校验：prohibit / unreachable / blackhole / throw。
- Condition: 与 lookup 或 goto 的组合由策略规则实现决定。
- Apply impact: 可拒绝、丢弃或继续 IPv4 查表；正常按路由表选路使用 lookup，而不是仅选 unicast。
- Trace result: 已检查 rule.action 校验与核心字符串表；未在规则校验中找到 unicast，保留面板现有选项但不把它声明为已验证动作。
- Evidence: `etc/init.d/network:110` (Xiaomi RN02 1.0.43) — rule action 校验只显式允许 prohibit、unreachable、blackhole、throw。
- Evidence: `docs/field-help-network-research.md:62` (Xiaomi RN02 1.0.43) — 策略规则参数表将 action 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173100.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 action 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule/goto

匹配此 IPv4 规则后跳到指定优先级的规则。值是策略规则 priority，不是路由表编号。

- Format / range: 源校验：0–65535。
- Condition: 需对应目标 priority；与 lookup/action 的组合应核对。
- Apply impact: 改变 IPv4 规则链的执行位置；无有效跳转目标时不能保证得到预期路径。
- Evidence: `etc/init.d/network:109` (Xiaomi RN02 1.0.43) — rule goto 校验限制 0–65535。
- Evidence: `docs/field-help-network-research.md:63` (Xiaomi RN02 1.0.43) — 策略规则参数表将 goto 声明为 int32。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173108.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 goto 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule/suppress_prefixlength

查询路由表时，抑制前缀长度不大于此阈值的 IPv4 路由结果。例如阈值 0 可排除该次查询的默认路由，保留更具体路径。

- Compact help: 抑制不长于阈值的 IPv4 路由；有效前缀长度为 0–32，面板上限保留。
- Unit: 位
- Format / range: 地址族前缀长度：0–32；面板统一上限 128 不代表 IPv4 都有效。
- Condition: 通常与 lookup 配合；只抑制此次规则的路由结果。
- Apply impact: 使当前查表忽略较宽的 IPv4 路由；若后续没有替代规则，目标可能失去出口。
- Evidence: `docs/field-help-network-research.md:64` (Xiaomi RN02 1.0.43) — 策略规则参数表将 suppress_prefixlength 声明为 int32。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173116.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 suppress_prefixlength 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule/disabled

面板将此项解释为停用 IPv4 策略规则。与 route.disabled 不同，基线规则参数表及 init 校验未列 disabled，不能据此承诺规则会被停用。

- Compact help: IPv4 规则表未找到 disabled；此项的停用效果尚未证实。
- Flags: version-dependent
- Apply impact: 保存可保留原生字段，但此 IPv4 规则是否退出查表链未证实；不要将它当作已验证的停用状态。
- Trace result: 已检查 netifd 的完整 12 项 rule 参数表、etc/init.d/network 的 rule 校验，以及可读网络 shell/Lua；未找到 rule.disabled 的独立消费，不能套用 route.disabled 的支持。
- Evidence: `docs/field-help-network-research.md:58` (Xiaomi RN02 1.0.43) — 已检查的规则参数表包含 priority。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173068.
- Evidence: `etc/init.d/network:100–110` (Xiaomi RN02 1.0.43) — rule 的校验声明匹配、查表及动作字段。

### rule6

#### network/rule6/in

按入站逻辑接口匹配此 IPv6 策略规则。源校验接收字符串；它筛选报文进入的接口，不是设置出口。

- Condition: 需已有 network 逻辑接口；与 out、src/dest 等条件一起匹配。
- Apply impact: 只让来自该接口的 IPv6 流量命中此规则；名称错误可能使规则不匹配。
- Evidence: `etc/init.d/network:116` (Xiaomi RN02 1.0.43) — rule6 校验声明 in 为 string。
- Evidence: `docs/field-help-network-research.md:53` (Xiaomi RN02 1.0.43) — 策略规则参数表将 in 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173028.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 in 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule6/out

按出站逻辑接口匹配此 IPv6 策略规则，不直接指定转发网卡。路由出口仍由 lookup 或 action 决定。

- Condition: 需已有逻辑接口及对应方向的规则条件。
- Apply impact: 限制此规则匹配的 IPv6 流量范围；不要用此项替代静态路由的 interface。
- Evidence: `etc/init.d/network:117` (Xiaomi RN02 1.0.43) — rule6 校验声明 out 为 string。
- Evidence: `docs/field-help-network-research.md:54` (Xiaomi RN02 1.0.43) — 策略规则参数表将 out 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173036.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 out 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule6/src

按 IPv6 来源地址或前缀匹配规则。来源条件筛选发送者，与路由条目 source 的源地址选择参数不同。

- Format / range: IPv6 CIDR 前缀；前缀长度 0–128。
- Condition: 与 dest、in/out、mark 一起匹配；invert 可反转条件。
- Apply impact: 决定哪些 IPv6 发送者使用本规则；过宽前缀可能把无关终端引向同一出口或阻断动作。
- Evidence: `etc/init.d/network:118` (Xiaomi RN02 1.0.43) — rule6 src 校验要求 cidr6。
- Evidence: `docs/field-help-network-research.md:56` (Xiaomi RN02 1.0.43) — 策略规则参数表将 src 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173052.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 src 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule6/dest

按 IPv6 目标地址或前缀匹配规则。它选择要处理的目标流量，不会创建该目标的路由。

- Format / range: IPv6 CIDR 前缀。
- Condition: 与 lookup 表内的路由、优先级及其他匹配条件配合。
- Apply impact: 决定访问哪些 IPv6 目的地时触发此查表或动作；对应表仍需有可用路由。
- Evidence: `etc/init.d/network:119` (Xiaomi RN02 1.0.43) — rule6 dest 校验要求 cidr6。
- Evidence: `docs/field-help-network-research.md:57` (Xiaomi RN02 1.0.43) — 策略规则参数表将 dest 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173060.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 dest 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule6/priority

设置此 IPv6 策略规则的处理顺序，数值较小先处理。核心规则表接收整数；此项不同于路由 metric。

- Format / range: 非负整数；源参数表未声明硬件式上限。
- Condition: 应与其他内核规则协调；goto 使用此优先级作为跳转目标。
- Apply impact: 可以让该规则先于其他 IPv6 查表规则生效，改变出口或阻断结果。
- Evidence: `docs/field-help-network-research.md:58` (Xiaomi RN02 1.0.43) — 策略规则参数表将 priority 声明为 int32。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173068.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 priority 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule6/lookup

匹配后查询指定 IPv6 路由表。可填写数字编号或名称；它不会自动把路由加入该表。

- Format / range: 源校验：数字 0–65535 或路由表名称。
- Condition: 配合 route6.table 和该表的有效路由；与特殊 action 的组合需核对。
- Apply impact: 使匹配的 IPv6 流量查不同的路由表；表内无有效路径时，后续结果取决于其他规则。
- Evidence: `etc/init.d/network:123` (Xiaomi RN02 1.0.43) — rule6 lookup 校验允许 0–65535 或字符串。
- Evidence: `docs/field-help-network-research.md:61` (Xiaomi RN02 1.0.43) — 策略规则参数表将 lookup 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173092.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 lookup 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule6/mark

按数据包标记匹配此 IPv6 规则。使用原生标记及可选掩码；本规则只匹配标记，不会主动给报文打标。

- Condition: 需防火墙、QoS 或其他路径先设置 packet mark；保留原生值/掩码格式。
- Apply impact: 仅已被其他机制标记的 IPv6 流量会走此规则；错误掩码可能扩大或缩小匹配范围。
- Evidence: `etc/init.d/network:121` (Xiaomi RN02 1.0.43) — rule6 mark 校验接收 string。
- Evidence: `docs/field-help-network-research.md:60` (Xiaomi RN02 1.0.43) — 策略规则参数表将 mark 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173084.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 mark 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule6/invert

反转此 IPv6 策略规则的匹配结果。源校验与核心参数表均声明布尔值；它反转条件，不反转查表结果。

- Condition: 需一起检查 src、dest、in、out、mark 等条件。
- Apply impact: 原本不匹配的 IPv6 流量可能转为命中，尤其宽泛条件下会影响更多连接。
- Evidence: `etc/init.d/network:122` (Xiaomi RN02 1.0.43) — rule6 invert 校验要求 bool。
- Evidence: `docs/field-help-network-research.md:55` (Xiaomi RN02 1.0.43) — 策略规则参数表将 invert 声明为 bool/int8。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173044.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 invert 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule6/action

设置 IPv6 策略规则的特殊动作。基线校验明确列出 prohibit、unreachable、blackhole、throw；面板另保留 unicast，不能据此断定它是固件可直接使用的动作。

- Compact help: 源校验支持禁止、不可达、黑洞与继续；普通查表用 lookup，unicast 未验证。
- Format / range: 基线校验：prohibit / unreachable / blackhole / throw。
- Condition: 与 lookup 或 goto 的组合由策略规则实现决定。
- Apply impact: 可拒绝、丢弃或继续 IPv6 查表；正常按路由表选路使用 lookup，而不是仅选 unicast。
- Trace result: 已检查 rule6.action 校验与核心字符串表；未在规则校验中找到 unicast，保留面板现有选项但不把它声明为已验证动作。
- Evidence: `etc/init.d/network:125` (Xiaomi RN02 1.0.43) — rule6 action 校验只显式允许 prohibit、unreachable、blackhole、throw。
- Evidence: `docs/field-help-network-research.md:62` (Xiaomi RN02 1.0.43) — 策略规则参数表将 action 声明为 string。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173100.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 action 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule6/goto

匹配此 IPv6 规则后跳到指定优先级的规则。值是策略规则 priority，不是路由表编号。

- Format / range: 源校验：0–65535。
- Condition: 需对应目标 priority；与 lookup/action 的组合应核对。
- Apply impact: 改变 IPv6 规则链的执行位置；无有效跳转目标时不能保证得到预期路径。
- Evidence: `etc/init.d/network:124` (Xiaomi RN02 1.0.43) — rule6 goto 校验限制 0–65535。
- Evidence: `docs/field-help-network-research.md:63` (Xiaomi RN02 1.0.43) — 策略规则参数表将 goto 声明为 int32。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173108.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 goto 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule6/suppress_prefixlength

查询路由表时，抑制前缀长度不大于此阈值的 IPv6 路由结果。例如阈值 0 可排除该次查询的默认路由，保留更具体路径。

- Unit: 位
- Format / range: 地址族前缀长度：0–128。
- Condition: 通常与 lookup 配合；只抑制此次规则的路由结果。
- Apply impact: 使当前查表忽略较宽的 IPv6 路由；若后续没有替代规则，目标可能失去出口。
- Evidence: `docs/field-help-network-research.md:64` (Xiaomi RN02 1.0.43) — 策略规则参数表将 suppress_prefixlength 声明为 int32。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173116.
- Evidence: `docs/field-help-network-research.md:171` (Xiaomi RN02 1.0.43) — 真实策略规则消费保存 suppress_prefixlength 槽及标志。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 50252.

#### network/rule6/disabled

面板将此项解释为停用 IPv6 策略规则。与 route.disabled 不同，基线规则参数表及 init 校验未列 disabled，不能据此承诺规则会被停用。

- Compact help: IPv6 规则表未找到 disabled；此项的停用效果尚未证实。
- Flags: version-dependent
- Apply impact: 保存可保留原生字段，但此 IPv6 规则是否退出查表链未证实；不要将它当作已验证的停用状态。
- Trace result: 已检查 netifd 的完整 12 项 rule 参数表、etc/init.d/network 的 rule6 校验，以及可读网络 shell/Lua；未找到 rule6.disabled 的独立消费，不能套用 route.disabled 的支持。
- Evidence: `docs/field-help-network-research.md:58` (Xiaomi RN02 1.0.43) — 已检查的规则参数表包含 priority。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 173068.
- Evidence: `etc/init.d/network:115–125` (Xiaomi RN02 1.0.43) — rule6 的校验声明匹配、查表及动作字段。

### globals

#### network/globals/ula_prefix

设置网络的 IPv6 唯一本地地址前缀。核心拆分地址与前缀长度并建立全局前缀；dnsmasq 还用它筛选路由器名称的本地 IPv6 记录。

- Format / range: 核心前缀处理接受长度1–64；ULA通常取fd00::/8内的/48，非运行缺省。
- Condition: 下游地址分配还需 ip6assign；DNS 主机名生成另受 dnsmasq 配置影响。
- Apply impact: 会影响本地 IPv6 地址规划及路由器名称的本地 DNS 记录；ULA 本身不提供公网路由。
- Evidence: `etc/init.d/dnsmasq:452` (Xiaomi RN02 1.0.43) — dnsmasq 读取 network globals 的 ula_prefix。
- Evidence: `etc/init.d/dnsmasq:460–465` (Xiaomi RN02 1.0.43) — 对匹配 ULA 前缀的 LAN IPv6 地址添加路由器主机名记录。
- Evidence: `docs/field-help-network-research.md:174` (Xiaomi RN02 1.0.43) — 核心 globals 分支读取 ula_prefix 并交给前缀处理函数。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 67260.
- Evidence: `docs/field-help-network-research.md:179` (Xiaomi RN02 1.0.43) — 核心解析IPv6前缀并要求长度1–64，更新全局前缀对象。
  - Stock binary: `sbin/netifd`; SHA-256 `7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`; offset 44148.

#### network/globals/packet_steering

面板将此项解释为 CPU 间网络处理分流：0 停用、1 启用、2 所有 CPU。基线文本消费者和 netifd 参数表未找到此键；RN02 另有 ECM/NSS 加速路径，不能视为同一开关。

- Compact help: CPU 分流选项；1.0.43 未定位此键的消费，不等同于 ECM/NSS 加速。
- Flags: version-dependent, hardware-dependent
- Apply impact: 是否改变 CPU 分流尚未证实；不能据此判断硬件加速已停用或启用，也不能保证吞吐变化。
- Trace result: 已搜索 etc/init.d、lib、sbin、usr/sbin、usr/share 的可读消费者和全部可读反编译 Lua，并检查 netifd 参数表；未找到 globals.packet_steering 的输入或应用。
- Evidence: `lib/miwifi/arch/lib_arch_network.sh:10–20` (Xiaomi RN02 1.0.43) — 架构初始化操作的是 ecm.global.acceleration_engine，不是 packet_steering。
- Evidence: `etc/init.d/network:34` (Xiaomi RN02 1.0.43) — network 启动 netifd。

## wireless

### wifi-device

#### wireless/wifi-device/type

选择负责启动这块射频的驱动。原厂按此值调用 scan_<驱动> 与 enable/disable_<驱动>；它不是无线网卡名称。

- Flags: hardware-dependent, version-dependent
- Condition: 应与 /lib/wifi 中的厂商驱动脚本匹配。
- Apply impact: 改错会找不到驱动处理函数，使该射频及其无线网络无法启动。
- Evidence: `sbin/wifi:218–223` (Xiaomi RN02 1.0.43) — 读取 wifi-device.type 后按驱动名分派扫描与启停；无对应脚本时报告不支持。
- Evidence: `lib/wifi/qcawificfg80211.sh:1560–1561` (Xiaomi RN02 1.0.43) — 兼容处理会把旧 qcawifi 驱动类型改为 qcawificfg80211。

#### wireless/wifi-device/path

按硬件路径定位无线射频。原厂 QCA 实际读取 phy 与 macaddr，尚未找到 wifi-device.path 的读取；填写路径不能保证改变射频绑定。

- Compact help: 通用硬件路径字段；原厂 QCA 定位读取 phy/macaddr，尚未找到 path 消费。
- Flags: hardware-dependent, version-dependent
- Condition: 已验证的 QCA 定位入口是 phy 或 macaddr。
- Apply impact: 原厂射频绑定是否受 path 改动影响尚未确认；已验证的定位过程使用 phy/macaddr。
- Trace result: 已检索 sbin/wifi、lib/wifi、lib/netifd、初始化脚本及反编译 XQWifiUtil；未发现读取 wifi-device 的 path 选项。出现的 path 是程序或固件文件路径，不能证明此字段生效。
- Evidence: `lib/wifi/qcawificfg80211.sh:613–624` (Xiaomi RN02 1.0.43) — 射频查找读取 macaddr 与 phy，并用 /sys/class/net/wifi* 的地址匹配。

#### wireless/wifi-device/phy

指定厂商物理射频接口标识。QCA 脚本要求它对应 /sys/class/net 中的设备；未填写时可通过 macaddr 查找 wifi*。

- Compact help: 原厂使用 /sys/class/net 中的射频标识；通用 phy0 示例不保证适用于 QCA。
- Flags: generated, hardware-dependent
- Condition: 与 macaddr、真实 QCA 射频设备相符；厂商 wifi* 标识不等于通用 phy0 示例。
- Apply impact: 标识不存在会使射频查找失败，随后该射频的接口无法正常创建。
- Evidence: `lib/wifi/qcawificfg80211.sh:613–626` (Xiaomi RN02 1.0.43) — phy 为空且有 macaddr 时按系统网卡地址匹配 wifi*；目标不存在时返回失败。
- Evidence: `lib/wifi/qcawificfg80211.sh:7836–7841` (Xiaomi RN02 1.0.43) — 创建 VAP 时将 phy 作为 wlandev，并从该网卡的 phy80211/name 获取内核 wiphy。

#### wireless/wifi-device/macaddr

用于按硬件 MAC 地址查找物理射频。此处不是为每个 SSID 指定 BSSID；缺失时脚本从已找到的 phy 网卡读取地址。

- Compact help: 按硬件地址定位射频；不是设置此射频下所有 SSID 的 BSSID。
- Flags: generated, hardware-dependent
- Condition: 主要在 phy 未指定时参与定位。
- Apply impact: 改成不匹配的地址可能找不到射频；已有 phy 时该字段并不直接执行网卡 MAC 修改。
- Evidence: `lib/wifi/qcawificfg80211.sh:613–629` (Xiaomi RN02 1.0.43) — macaddr 转为小写后用于匹配 /sys/class/net/wifi* 地址；缺失时由 phy 的 address 补入。

#### wireless/wifi-device/band

通用模型使用 2g/5g/6g；原厂 QCA 脚本却读取数值 band，并把 3 用于 6 GHz 判断。两种表示法不能视为可互换。

- Compact help: 通用频段字符串与原厂数值 band 不可视为可互换；3 用于厂商 6 GHz 判断。
- Source fallback: 0（QCA 脚本缺省值；不代表固定频段）
- Format / range: 取值编码由驱动决定；已证实厂商 3 表示 6 GHz 检查分支。
- Flags: hardware-dependent, version-dependent
- Condition: 关联 hwmode、channel 和真实射频能力；不要由下拉选项推断硬件有 6 GHz。
- Apply impact: 换成通用字符串可能破坏厂商数值比较，影响信道和加密检查；这不是为 RN02 增加新射频的方法。
- Evidence: `lib/netifd/netifd-wireless.sh:71–79` (Xiaomi RN02 1.0.43) — 通用 netifd 将 2g、5g、6g、60g 映射为 hwmode。
- Evidence: `lib/wifi/qcawificfg80211.sh:7481–7486` (Xiaomi RN02 1.0.43) — 厂商 VAP 初始化读取 band，缺失时使用 0。
- Evidence: `lib/wifi/qcawificfg80211.sh:7775–7781` (Xiaomi RN02 1.0.43) — TKIP 分支把 band=3 识别为 6 GHz，并在未强制允许时跳过。

#### wireless/wifi-device/hwmode

指定射频协议族，不仅是旧 11b/11g/11a。厂商还按 11ac、11axg/11axa、11beg/11bea 与 htmode 组合选择实际模式。

- Compact help: 原厂仍用 hwmode 选择 HT/VHT/HE/EHT 协议族；保留现有厂商 11ax/11be 值。
- Source fallback: auto（QCA 读取缺省）
- Flags: hardware-dependent, version-dependent
- Condition: 与 htmode、厂商 ax 开关、硬件 hwmodes 相配；保留现有 11ax/11be 厂商值。
- Apply impact: 改动会改变 VAP 的协议模式与客户端兼容性；ax=0 时厂商会覆盖为 11ng 或 11ac。
- Evidence: `lib/wifi/qcawificfg80211.sh:7890–7906` (Xiaomi RN02 1.0.43) — hwmode 缺失用 auto；ax=0 时按硬件频段改为 11ng/11ac。
- Evidence: `lib/wifi/qcawificfg80211.sh:7926–7945` (Xiaomi RN02 1.0.43) — hwmode 与 htmode 联合映射到 HT/VHT/HE/EHT 驱动模式。
- Evidence: `lib/wifi/qcawificfg80211.sh:7986–8008` (Xiaomi RN02 1.0.43) — 11beg/11bea 分支识别 HT 与 EHT 宽度值。

#### wireless/wifi-device/channel

选择无线射频的主工作信道。原厂 QCA 将 auto/AUTO 转成自动选择值 0；信道 165 会强制采用 HT20，不能维持宽频模式。

- Source fallback: 0 / auto（脚本缺省与自动信道表示）
- Unit: 信道编号
- Format / range: auto、AUTO、0 或硬件/监管域允许的信道；没有通用连续范围。
- Flags: hardware-dependent
- Condition: 受 country、band、hwmode、厂商 bw 与 htmode 共同影响。
- Condition: MLO 接口更新另行协调伙伴链路；TWT 使用独立 twt_responder，不由信道值启用。
- Apply impact: 厂商更新逻辑把信道改变列为重启该射频全部 VAP 的条件，已连接客户端可能需要重连。
- Evidence: `lib/wifi/qcawificfg80211.sh:7483–7486` (Xiaomi RN02 1.0.43) — channel 缺失用 0，auto/AUTO 归一化为 0。
- Evidence: `lib/wifi/qcawificfg80211.sh:7598–7600` (Xiaomi RN02 1.0.43) — 信道 165 强制 htmode=HT20。
- Evidence: `lib/wifi/qcawificfg80211.sh:5944–5956` (Xiaomi RN02 1.0.43) — channel、ax 或 bw 改变会设置 restart_all。
- Evidence: `lib/wifi/qcawificfg80211.sh:6436–6461` (Xiaomi RN02 1.0.43) — 信道、认证及 twt_responder 等变化进入同一接口更新流程；其中有接口下线与伙伴 MLO 链路协调。

#### wireless/wifi-device/htmode

请求无线信道宽度与扩展方向。原厂按厂商 bw、信道和硬件最大宽度重算；例如 HT20 表示 20 MHz 请求，保存值不保证直接生效。

- Compact help: 原厂会按 bw、信道和硬件能力重算宽度；保存值不保证直接生效。
- Source fallback: auto（读取缺省；之后可能被 bw/硬件重算）
- Unit: 模式中宽度数字为 MHz
- Format / range: 由 hwmode 和驱动分支决定；不要假定所有 VHT/HE/EHT 下拉值都适用于原厂。
- Flags: hardware-dependent, version-dependent
- Condition: 关联厂商 bw、ax、channel、standby_htmode 与硬件最大宽度。
- Condition: MLO 绑定在 wifi-iface.mld；TWT 为独立 twt_responder，HE/EHT 宽度本身不等于开启 TWT。
- Apply impact: 有效宽度会影响无线速率与占用频谱；厂商 bw 改动会触发全部 VAP 更新，不能保证只改 htmode 就采用所选宽度。
- Evidence: `lib/wifi/qcawificfg80211.sh:7481–7486` (Xiaomi RN02 1.0.43) — QCA 初始化读取 htmode，缺失用 auto。
- Evidence: `lib/wifi/qcawificfg80211.sh:7515–7538` (Xiaomi RN02 1.0.43) — 读取厂商 bw，并据频段和信道重算 htmode。
- Evidence: `lib/wifi/qcawificfg80211.sh:7575–7593` (Xiaomi RN02 1.0.43) — 5 GHz 宽度分支使用 HT80/HT160；自动分支可读取 5g_maxchwidth。
- Evidence: `lib/wifi/qcawificfg80211.sh:7986–8019` (Xiaomi RN02 1.0.43) — EHT 带宽取值与 11beg/11bea 联合映射，包含 EHT320。
- Evidence: `lib/wifi/qcawificfg80211.sh:7839–7849` (Xiaomi RN02 1.0.43) — MLO 接口创建另由 mld 关联选择 mld_iface 或 mld_addr。
- Evidence: `lib/wifi/qcawificfg80211.sh:9785–9786` (Xiaomi RN02 1.0.43) — TWT 接口响应由独立 twt_responder 读取并下发。

#### wireless/wifi-device/country

设置监管国家/地区。QCA 支持国家代码或数值监管标识：数字开头走 setCountryID，其余走 setCountry。

- Compact help: 原厂支持国家代码或数值监管标识；数字开头走 setCountryID。
- Source fallback: 未填写且存在启用 AP 类接口时下发 156；其他模式此函数不改国家。
- Format / range: 驱动认可的国家代码或数值监管标识。
- Flags: hardware-dependent
- Condition: 与 VAP 的 mode、channel 和厂商功率控制关联。
- Apply impact: 会改变射频监管配置；信道与功率必须符合设备及当地规则，修改不保证驱动接受。
- Evidence: `lib/wifi/qcawificfg80211.sh:4803–4826` (Xiaomi RN02 1.0.43) — country 为空时调用 set_default_country；非空按数字或代码选择驱动命令。
- Evidence: `lib/wifi/qcawificfg80211.sh:1104–1112` (Xiaomi RN02 1.0.43) — 缺省函数跳过停用 VAP；有 AP 类接口时实际下发 setCountryID 156。

#### wireless/wifi-device/txpower

射频功率请求，通用单位为 dBm。原厂虽读取它，但启动时主要按厂商 txpwr 档位及 misc 中的最大功率重算并下发。

- Compact help: 功率以 dBm 表示；原厂主要按 txpwr 档位及最大功率重算，不保证直接采用此值。
- Unit: dBm（厂商另有半 dBm 编码）
- Format / range: 实际范围由驱动、监管域及厂商最大功率配置决定；未证实固定 0–40 范围。
- Flags: hardware-dependent, version-dependent
- Condition: 主要关联 txpwr、misc.wireless.if_2g_maxpower/if_5g_maxpower、country；接口须已启动。
- Apply impact: 不能保证直接编辑 txpower 就得到该功率；实际输出还受射频、监管域和厂商功率档位影响。
- Evidence: `lib/wifi/qcawificfg80211.sh:4837–4841` (Xiaomi RN02 1.0.43) — 射频初始化读取 txpower，没有在该读取处指定缺省值。
- Evidence: `lib/wifi/qcawificfg80211.sh:9694–9700` (Xiaomi RN02 1.0.43) — 厂商 txpwr 的 mid/min/其他分支分别取最大功率减 1、减 3 或最大功率。
- Evidence: `lib/wifi/qcawificfg80211.sh:9703–9709` (Xiaomi RN02 1.0.43) — 半 dBm 编码另行转换；CN/156 分支才在此处通过 iwconfig 下发计算结果。

#### wireless/wifi-device/disabled

停用整个物理射频，不只是其中一个 SSID。wifi 入口见到 1 时切换为 disable 操作。

- Source fallback: 0（厂商更新比较缺省）
- Condition: 覆盖其下 wifi-iface 是否启用；MLO 的可用链路也依赖关联射频。
- Apply impact: 该射频上的全部无线接口可能下线；通过此射频访问管理面板的连接也可能断开。
- Evidence: `sbin/wifi:212–221` (Xiaomi RN02 1.0.43) — 遍历 wifi-device 时读取 disabled=1 并改为 disable，然后分派驱动启停。
- Evidence: `lib/wifi/qcawificfg80211.sh:5991–6002` (Xiaomi RN02 1.0.43) — 射频 disabled 缺失按 0 比较，状态改变分别调用 disable 或 enable。

#### wireless/wifi-device/legacy_rates

通用无线配置中表示允许旧式低速率。此固件未找到 legacy_rates 的配置读取，不能据此确认 RN02 会改变基础速率。

- Compact help: 通用旧速率开关；尚未找到原厂读取 legacy_rates 的路径。
- Flags: legacy, version-dependent
- Condition: 旧速率行为通常关联 hwmode 与基础速率，但原厂映射未证实。
- Apply impact: 尚不能确认修改会改变原厂速率集或旧客户端兼容性。
- Trace result: 已检索 sbin/wifi、lib/wifi、lib/netifd、初始化脚本、全部可读 Lua/反编译 Lua；未出现 legacy_rates。已找到 basic_rate/basic_rates 生成路径，但不能证明它与此字段相连。
- Evidence: `lib/wifi/hostapd.sh:108` (Xiaomi RN02 1.0.43) — 通用 hostapd 设备声明的是 basic_rate 数组。
- Evidence: `lib/wifi/hostapd.sh:138–144` (Xiaomi RN02 1.0.43) — basic_rate 列表经转换后写为 hostapd basic_rates。
- Evidence: `lib/wifi/qcawificfg80211.sh:9539–9540` (Xiaomi RN02 1.0.43) — 厂商另读取 dis_legacy 并下发同名命令；不是 legacy_rates 选项。

#### wireless/wifi-device/noscan

通用字段用于跳过 40 MHz 共存扫描。netifd 公共设备声明包含 noscan，但原厂 QCA 的共存控制读取 disablecoext。

- Compact help: 公共模型声明此字段；原厂共存命令读取 disablecoext，未证实 noscan 映射。
- Flags: hardware-dependent, version-dependent
- Condition: 与 2.4 GHz 的 bw/channel 及厂商 disablecoext 路径相关；未证实 noscan 到该命令的转换。
- Apply impact: 此字段是否影响 RN02 共存扫描尚未确认；已验证的厂商入口是另一个配置项 disablecoext。
- Trace result: 已检索 sbin/wifi、lib/wifi、lib/netifd 和反编译 Lua；仅发现 netifd 的 noscan 声明，未找到读取 noscan 后影响 QCA 共存扫描的消费者。
- Evidence: `lib/netifd/netifd-wireless.sh:365–367` (Xiaomi RN02 1.0.43) — 公共设备配置声明 noscan 字符串字段。
- Evidence: `lib/wifi/qcawificfg80211.sh:8126–8130` (Xiaomi RN02 1.0.43) — QCA 从 wifi-iface 读取 disablecoext 并下发；强制 11NGHT40 时另下发 1。

#### wireless/wifi-device/beacon_int

通用设备级信标间隔。公共 hostapd 生成器会写入 beacon_int，但原厂主要 VAP 生成路径读取的是 wifi-iface.bintval。

- Compact help: 通用设备信标间隔；原厂主 VAP 路径读取的是 iface.bintval。
- Unit: TU（802.11 时间单位，1 TU = 1024 微秒）
- Flags: version-dependent
- Condition: 信标间隔与 DTIM 周期共同影响组播通知节奏；厂商入口是 iface.bintval。
- Apply impact: 此设备级字段能否控制原厂 VAP 尚未确认；不能把通用生成器的支持等同于 RN02 主启动路径已接入。
- Trace result: 已追踪 lib/wifi/hostapd.sh、qcawificfg80211.sh、wifi 入口和初始化脚本；找到公共 beacon_int 生成器，但未找到 QCA 主 VAP 路径读取 wifi-device.beacon_int；已验证该路径读 bintval。
- Evidence: `lib/wifi/hostapd.sh:112` (Xiaomi RN02 1.0.43) — 公共设备配置声明 beacon_int 整数。
- Evidence: `lib/wifi/hostapd.sh:123–144` (Xiaomi RN02 1.0.43) — 公共生成器读取 beacon_int，非空时写入同名 hostapd 配置。
- Evidence: `lib/wifi/hostapd.sh:1574–1581` (Xiaomi RN02 1.0.43) — 原厂 VAP 生成器从 wifi-iface 读取 bintval。
- Evidence: `lib/wifi/hostapd.sh:2292–2295` (Xiaomi RN02 1.0.43) — VAP 路径把 bintval 写为 beacon_int；同处另读取 dtim_period。

#### wireless/wifi-device/distance

通用长距离链路参数，通常用于 ACK 超时计算。此 QCA 脚本读取 distance 后明确输出“不支持此驱动”。

- Compact help: 原厂 QCA 脚本明确表示不支持 distance；不能据此调整 ACK 超时。
- Unit: 米（通用字段语义）
- Flags: hardware-dependent, legacy
- Condition: 行为取决于驱动；此证据仅适用于原厂 QCA 路径。
- Apply impact: 本版本这条驱动路径没有把 distance 下发为 ACK 超时；填入距离不会据此完成链路优化。
- Evidence: `lib/wifi/qcawificfg80211.sh:4970–4977` (Xiaomi RN02 1.0.43) — 读取 distance；非空时输出 distance option not supported on this driver。

### wifi-iface

#### wireless/wifi-iface/device

把此无线网络挂到同文档中的 wifi-device 章节。扫描器据此把 wifi-iface 收集到所属射频的 VAP 列表。

- Flags: hardware-dependent
- Condition: 必须对应实际 wifi-device；MLO 链路另需匹配 mld 关联。
- Apply impact: 换射频会改变所用频段、信道和驱动能力；引用不存在的章节可能使此网络无法建立。
- Evidence: `lib/wifi_interface_helper.sh:94–98` (Xiaomi RN02 1.0.43) — wifi-iface.device 被读取，并把该 iface 追加到对应 device 的 vifs。
- Evidence: `lib/wifi/qcawificfg80211.sh:3942–3948` (Xiaomi RN02 1.0.43) — MLO 链路从 iface.device 寻找射频的 mldphy_name。

#### wireless/wifi-iface/network

连接到 network 文档中的逻辑网络，可使用原生列表。缺省并非一律 lan：帮助函数会尝试按 ifname 查找已有网络配置。

- Condition: 引用 /etc/config/network 中的逻辑接口；STA 桥接需匹配 WDS/厂商桥接模式。
- Apply impact: 会改变无线流量所在桥接网络及其地址分配路径；关联错网络可能无法访问 LAN 或管理面板。
- Evidence: `lib/wifi_interface_helper.sh:10–21` (Xiaomi RN02 1.0.43) — 读取 network；为空时按 ifname 调用 find_config。
- Evidence: `lib/wifi_interface_helper.sh:46–51` (Xiaomi RN02 1.0.43) — 对关联网络逐项调用 setup_interface。
- Evidence: `lib/wifi/wpa_supplicant.sh:186–189` (Xiaomi RN02 1.0.43) — STA 桥接需要 wds/extap/qwrap 等条件，否则拒绝桥接。

#### wireless/wifi-iface/ifname

内核无线接口名，也是 hostapd/supplicant 配置与状态路径的定位依据。厂商在缺省或 SON 分支中会自动生成它。

- Flags: generated, hardware-dependent
- Condition: 与 device、驱动生成规则及厂商服务引用保持一致。
- Apply impact: 改名会改变创建和关联配置的目标；厂商更新也用 ifname 匹配旧 VAP，可能删除重建接口。
- Evidence: `lib/wifi/qcawificfg80211.sh:1021–1026` (Xiaomi RN02 1.0.43) — SON 分支写入生成名称；其他分支使用生成名作为 ifname 缺省。
- Evidence: `lib/wifi/qcawificfg80211.sh:6009–6021` (Xiaomi RN02 1.0.43) — 更新用旧、新 ifname 匹配接口，未匹配或停用时加入删除列表。
- Evidence: `lib/wifi/qcawificfg80211.sh:7836–7841` (Xiaomi RN02 1.0.43) — ifname 用于 wlanconfig 与 iw 接口创建。

#### wireless/wifi-iface/mode

确定接口角色：AP 提供接入，STA 连接上级，其余模式走对应驱动分支。原厂 mesh 被转为 AP 类型，不能直接等同通用 802.11s。

- Compact help: 选择接口角色；原厂 mesh 创建为 AP 类型，不等同通用 802.11s。
- Flags: hardware-dependent, version-dependent
- Condition: STA 桥接依赖 wds/extap；AP 与 STA 的认证配置由不同服务消费。
- Condition: MLO 链路角色还受 wifi-iface.mld 和 wifi-mld.role 影响；mode 不能替代 MLO/TWT 开关。
- Apply impact: 切换角色会改变认证服务和桥接条件，并可能重建接口；原有客户端连接可能中断。
- Evidence: `lib/wifi/qcawificfg80211.sh:1049–1052` (Xiaomi RN02 1.0.43) — 厂商扫描接受 ap/sta/adhoc/monitor/mesh 等模式并收集 VAP。
- Evidence: `lib/wifi/qcawificfg80211.sh:7794–7801` (Xiaomi RN02 1.0.43) — 创建前把 ap 转为 __ap、sta 转为 managed，并把 mesh 也转为 __ap。
- Evidence: `lib/wifi/qcawificfg80211.sh:3932–3938` (Xiaomi RN02 1.0.43) — MLO 的 link_mode=sta 会使所选 wlanmode 为 managed。
- Evidence: `lib/wifi/qcawificfg80211.sh:9785–9786` (Xiaomi RN02 1.0.43) — TWT 由独立 twt_responder 选项下发。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQWifiUtil.lua:12386–12411` (Xiaomi RN02 1.0.43) — set_twt_hostap 对所选 wifinet 写入 twt_responder，并保存提交 wireless。

#### wireless/wifi-iface/ssid

AP 广播或 STA 匹配的无线网络名。原厂分别写入 hostapd 的 ssid 与 supplicant 的 network 块。

- Unit: 字节（不是字符数）
- Format / range: 802.11 SSID 最多 32 字节；厂商界面另有按频段长度策略，脚本此处未校验上限。
- Condition: MLO STA 同组链路需要一致 SSID；hidden 只控制可见性，不替代名称。
- Apply impact: 改名会让旧名称的客户端需要重新选择网络；MLO STA 伙伴链路名称不一致会触发配置不匹配。
- Evidence: `lib/wifi/hostapd.sh:628` (Xiaomi RN02 1.0.43) — hostapd BSS 配置读取 iface.ssid。
- Evidence: `lib/wifi/hostapd.sh:768` (Xiaomi RN02 1.0.43) — SSID 被写为 hostapd ssid。
- Evidence: `lib/wifi/wpa_supplicant.sh:598–603` (Xiaomi RN02 1.0.43) — supplicant network 块写入 ssid。
- Evidence: `lib/wifi/qcawificfg80211.sh:4398–4415` (Xiaomi RN02 1.0.43) — MLO STA 读取各链路 SSID，并在与已记录名称不一致时标记失败。

#### wireless/wifi-iface/encryption

选择认证/加密配置。原厂解析加密字符串；WPA3/增强开放还涉及独立 sae/owe、sae_password 与 PMF 选项，不保证单改通用名称就等价。

- Compact help: 原厂 WPA3 等还需 sae/owe 与 PMF 伴随参数；不保证单改通用加密名称就等价。
- Source fallback: none（脚本缺省；并非建议使用开放网络）
- Flags: hardware-dependent, version-dependent
- Condition: PSK/WEP 关联 key；企业认证关联 RADIUS；SAE/OWE 关联厂商独立开关与 ieee80211w。
- Condition: MLO STA 各链路认证参数须一致；TWT 仍是独立设置，没有从加密值自动开启的证据。
- Apply impact: 会改变客户端认证及密码要求；不支持的组合可能跳过或销毁 VAP，导致网络无法上线。
- Evidence: `lib/wifi/hostapd.sh:290–291` (Xiaomi RN02 1.0.43) — 读取 encryption，缺失用 none，并转为小写。
- Evidence: `lib/wifi/hostapd.sh:415–465` (Xiaomi RN02 1.0.43) — 解析 WPA 版本及 TKIP/CCMP/GCMP 字符串组件。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQWifiUtil.lua:4855–4875` (Xiaomi RN02 1.0.43) — 厂商 ccmp 方案同步 sae、sae_password、ieee80211w；psk2+ccmp 方案设置混合认证相关值。
- Evidence: `lib/wifi/qcawificfg80211.sh:4418–4425` (Xiaomi RN02 1.0.43) — MLO STA 链路 encryption 不一致时报告 MLD ENC Mismatch 并失败。
- Evidence: `lib/wifi/qcawificfg80211.sh:6884–6889` (Xiaomi RN02 1.0.43) — SAE/OWE 启用时拒绝 TKIP/WEP。
- Evidence: `lib/wifi/qcawificfg80211.sh:9785–9786` (Xiaomi RN02 1.0.43) — TWT 响应另读 twt_responder，并非从 encryption 推导。

#### wireless/wifi-iface/key

按认证方案提供密码或原生密钥。PSK 分支将 64 字节内容按预共享密钥输出，否则作为口令；WEP 时可用 1–4 选择 key1–key4。

- Format / range: PSK 标准口令 8–63 字节或 64 位十六进制；厂商密码接口检查 8–63 字节，底层另支持 64 字节分支。
- Flags: credential
- Condition: 与 encryption 配套；WEP 数字值选择密钥槽，SAE 可另用 sae_password；MLO STA 密钥须一致。
- Apply impact: 密钥与客户端不一致会导致认证失败；厂商某些加密分支发现 key 与替代密钥来源都为空时会销毁该 VAP。
- Evidence: `lib/wifi/hostapd.sh:514–520` (Xiaomi RN02 1.0.43) — PSK 分支读取 key；长度 64 写为 wpa_psk，其他非空值写为 wpa_passphrase。
- Evidence: `lib/wifi/hostapd.sh:544–560` (Xiaomi RN02 1.0.43) — WEP 的 key 为 1–4 时读取对应密钥槽并减 1 作为默认索引，其他值作为直接密钥。
- Evidence: `lib/wifi/qcawificfg80211.sh:6874–6882` (Xiaomi RN02 1.0.43) — PSK/WEP 等分支检查 key 和 wpa_psk_file；均为空时销毁 VAP。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQWifiUtil.lua:4855–4871` (Xiaomi RN02 1.0.43) — 厂商纯 SAE 路径改用 sae_password；混合路径同时保留 key 与 sae_password。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQWifiUtil.lua:4424–4437` (Xiaomi RN02 1.0.43) — 厂商非开放/非 wep-open 密码校验要求长度不少于 8 且不超过 63。

#### wireless/wifi-iface/key1

WEP 的第 1 组密钥，生成器映射为 wep_key0。只有 encryption 走 WEP 且 key 使用 1–4 槽位选择时才遍历读取。

- Flags: credential, legacy
- Condition: WEP 认证；key=1 选中本槽。原厂还会按 force_wep 检查是否允许 WEP。
- Apply impact: key=1 时它成为发送默认密钥；不匹配会使 WEP 认证或解密失败。
- Evidence: `lib/wifi/hostapd.sh:548–556` (Xiaomi RN02 1.0.43) — key=1–4 时循环读取 key1–key4，并以 idx-1 生成 wep_key0–3。
- Evidence: `lib/wifi/wpa_supplicant.sh:199–208` (Xiaomi RN02 1.0.43) — supplicant 同样读取四个 WEP 槽位并以 key-1 选择发送索引。

#### wireless/wifi-iface/key2

WEP 的第 2 组密钥，映射为 wep_key1。厂商企业认证辅助路径也可能把它作为第二认证服务器共享密钥的旧式回退。

- Flags: credential, legacy
- Condition: WEP 的 key=2；另需注意企业认证 auth_secret2 的回退路径。
- Apply impact: key=2 时用于 WEP 默认发送密钥；若第二 RADIUS 密钥未单列，也可能影响备用服务器认证。
- Evidence: `lib/wifi/hostapd.sh:548–556` (Xiaomi RN02 1.0.43) — 槽位循环把 key2 映射为索引 1，默认发送索引按 key-1。
- Evidence: `lib/wifi/hostapd.sh:173–175` (Xiaomi RN02 1.0.43) — auth_secret2 缺失时读取 key2，再生成第二服务器 shared_secret。

#### wireless/wifi-iface/key3

WEP 的第 3 组密钥，映射为 wep_key2；仅在 WEP 槽位模式下被读取，不是第三组 WPA 密码。

- Flags: credential, legacy
- Condition: encryption 为 WEP 且 key 为槽位选择；key=3 选中本槽。
- Apply impact: key=3 时选为发送默认密钥；与对端不一致会导致 WEP 解密失败。
- Evidence: `lib/wifi/hostapd.sh:548–556` (Xiaomi RN02 1.0.43) — key=1–4 时循环读 key1–key4，并按 idx-1 输出 WEP 密钥与默认索引。
- Evidence: `lib/wifi/wpa_supplicant.sh:199–208` (Xiaomi RN02 1.0.43) — supplicant 槽位循环生成 wep_key0–3 与 wep_tx_keyidx。

#### wireless/wifi-iface/key4

WEP 的第 4 组密钥，映射为 wep_key3；只用于 WEP 槽位配置。

- Flags: credential, legacy
- Condition: encryption 为 WEP；key=4 选中本槽；底层是否允许 WEP 受厂商 force_wep 检查。
- Apply impact: key=4 时成为默认发送密钥；不能把它当作 WPA 备用密码来轮换。
- Evidence: `lib/wifi/hostapd.sh:548–556` (Xiaomi RN02 1.0.43) — 槽位循环覆盖 idx=4，按 idx-1 输出并按 key-1 选择默认密钥。
- Evidence: `lib/wifi/wpa_supplicant.sh:199–208` (Xiaomi RN02 1.0.43) — supplicant 读取 idx=1–4 的密钥槽并设置发送索引。

#### wireless/wifi-iface/disabled

停用此 wifi-iface。厂商在 VAP 创建与启动阶段读到 1 会跳过，不会因此停用同射频其他 SSID。

- Source fallback: 0（创建与启动读取缺省）
- Condition: 所属 device 也必须启用；MLO 更新另外协调同组链路。
- Apply impact: 该 SSID 或上联接口会下线；若是回程 STA/MLO 链路，可能连带失去上级连接。
- Evidence: `lib/wifi/qcawificfg80211.sh:7748–7749` (Xiaomi RN02 1.0.43) — VAP 初始化读取 disabled，缺省 0；非 0 跳过。
- Evidence: `lib/wifi/qcawificfg80211.sh:9263–9264` (Xiaomi RN02 1.0.43) — VAP 启动阶段再次读取 disabled，非 0 返回。
- Evidence: `lib/wifi/qcawificfg80211.sh:5769–5782` (Xiaomi RN02 1.0.43) — MLO 更新比较 iface.disabled、mlo_enable 与上联变化，并标记需更新的链路。

#### wireless/wifi-iface/hidden

控制 SSID 的信标/扫描可见性。厂商同时下发 hide_ssid 与 hostapd ignore_broadcast_ssid；这不是认证或加密措施。

- Source fallback: 0（显示 SSID）
- Condition: 保留 ssid 与正确密码；隐藏网络的发现行为依客户端实现。
- Apply impact: 客户端可能需要手动填写名称；厂商更新把 hidden 变化纳入接口重新配置流程。
- Evidence: `lib/wifi/qcawificfg80211.sh:8108–8112` (Xiaomi RN02 1.0.43) — hidden 缺省 0，下发 hide_ssid；值为 1 时还处理 dynamicbeacon。
- Evidence: `lib/wifi/hostapd.sh:2296–2297` (Xiaomi RN02 1.0.43) — hostapd BSS 写入 ignore_broadcast_ssid。
- Evidence: `lib/wifi/qcawificfg80211.sh:6436–6440` (Xiaomi RN02 1.0.43) — hidden_changed 与认证、信道等变化触发接口更新处理。

#### wireless/wifi-iface/isolate

阻止 AP 类接口内客户端通过无线桥接直接互访。QCA 使用 isolate 的反值设置 ap_bridge；wrap 模式还可能被射频隔离强制覆盖。

- Source fallback: 0（不要求接口隔离；wrap 可能覆盖）
- Condition: 主要作用于 AP/mesh/wrap 类路径；关联射频 ap_isolation_enabled。
- Apply impact: 改变同一 BSS 客户端的互通；不等同于为不同逻辑网络配置完整防火墙隔离。
- Evidence: `lib/wifi/qcawificfg80211.sh:9268–9273` (Xiaomi RN02 1.0.43) — 读取 isolate 缺省 0；射频隔离启用时 wrap 强制 isolate=1。
- Evidence: `lib/wifi/qcawificfg80211.sh:9360–9364` (Xiaomi RN02 1.0.43) — AP 类模式将 isolate 的反值下发到 ap_bridge。

#### wireless/wifi-iface/wds

启用四地址无线桥接。厂商把 1/on/enabled 视为启用，并在非 Multi-AP 分支执行 iw set 4addr on。

- Source fallback: 0（supplicant 读取缺省；QCA 未匹配启用值也转为 0）
- Condition: 主要关联 STA、network 桥接与对端支持；map 非 0 时由 supplicant 关联后启用四地址。
- Apply impact: 会改变 STA 桥接条件；对端需支持相应四地址连接，否则可能无法传递下游流量。
- Evidence: `lib/wifi/qcawificfg80211.sh:8172–8185` (Xiaomi RN02 1.0.43) — wds 启用值转换为 1；map=0 时设置 4addr，随后下发驱动 wds。
- Evidence: `lib/wifi/wpa_supplicant.sh:180–189` (Xiaomi RN02 1.0.43) — supplicant 将 wds 缺省为 0，STA 桥接检查允许 wds=1。

#### wireless/wifi-iface/wmm

无线多媒体优先级开关。QCA 驱动层读取 wmm 并下发，但原厂 hostapd BSS 生成路径另固定写 wmm_enabled=1。

- Compact help: QCA 下发 wmm，但 hostapd BSS 固定宣告开启；不保证单关此项就关闭 WMM。
- Flags: hardware-dependent, version-dependent
- Condition: 驱动 wmm 与 hostapd 宣告分别处理；不要将单个字段视为全链路开关。
- Apply impact: 驱动命令可变化，但不能保证关闭此字段就会关闭对客户端宣告的 WMM；两层配置可能不一致。
- Evidence: `lib/wifi/qcawificfg80211.sh:8527–8528` (Xiaomi RN02 1.0.43) — 读取布尔 wmm，并在非空时下发同名驱动命令。
- Evidence: `lib/wifi/hostapd.sh:2289–2290` (Xiaomi RN02 1.0.43) — 原厂 BSS 生成器固定输出 wmm_enabled=1。

#### wireless/wifi-iface/ieee80211r

启用快速 BSS 切换。AP 生成器据此增加 FT 认证算法及漫游域参数；STA 路径另下发 ft 驱动选项。

- Source fallback: 0（AP 读取缺省）
- Condition: 同一漫游域的 SSID、认证方式、mobility_domain 与 R0/R1 配置须协调；MLO 还有桥接 FDB 处理。
- Apply impact: 改变漫游认证方式；需其他 AP、认证参数和客户端配合，单独开启不能保证无缝漫游。
- Evidence: `lib/wifi/hostapd.sh:298` (Xiaomi RN02 1.0.43) — AP 配置把 ieee80211r 缺省为 0。
- Evidence: `lib/wifi/hostapd.sh:821–838` (Xiaomi RN02 1.0.43) — 启用时据 SAE/Suite-B/普通认证选择 FT 算法。
- Evidence: `lib/wifi/hostapd.sh:1290–1313` (Xiaomi RN02 1.0.43) — FT 分支读取 mobility_domain 与密钥持有者等参数。
- Evidence: `lib/wifi/qcawificfg80211.sh:9563–9565` (Xiaomi RN02 1.0.43) — STA 模式读取 ieee80211r 并下发 ft。
- Evidence: `lib/wifi/qcawificfg80211.sh:2696–2716` (Xiaomi RN02 1.0.43) — MLD 且 ieee80211r 非空时，桥接辅助路径处理本地 FDB 项。

#### wireless/wifi-iface/ieee80211k

通用字段表示无线资源测量/邻居信息。此固件已找到厂商 rrm 命令入口，但未找到 ieee80211k 选项到该入口的映射。

- Compact help: 通用无线测量开关；原厂读取 rrm，尚未找到 ieee80211k 映射。
- Flags: version-dependent
- Condition: 与支持测量的 AP/客户端及厂商 rrm 配置相关；不等同于 ieee80211r。
- Apply impact: 不能确认切换此字段会改变 RN02 的测量或邻居报告；不应当作已验证的漫游开关。
- Trace result: 已检索 sbin/wifi、lib/wifi、lib/netifd、初始化脚本及全部反编译 Lua，未发现 ieee80211k。hostapd ELF 可找到 rrm_neighbor_report 选项字符串，但无证据表明此 UCI 字段被转换到它；rrm 是已验证的独立厂商读取。
- Evidence: `lib/wifi/qcawificfg80211.sh:8900–8901` (Xiaomi RN02 1.0.43) — 厂商读取 wifi-iface.rrm 并下发 rrm。

#### wireless/wifi-iface/ieee80211w

控制受保护管理帧（PMF）：0 停用、1 可选、2 必须。原厂 SAE/OWE/Suite-B 分支可能强制或提升有效值。

- Compact help: PMF 的 0/1/2 为停用/可选/必须；SAE、OWE 等可能提升有效值。
- Source fallback: 0（一般路径）；SAE/OWE/Suite-B 有条件覆盖。
- Format / range: 0 / 1 / 2（PMF 协议含义）
- Condition: 关联 encryption、sae、owe、suite_b；MLO STA 各链路需一致，实际值以生成配置为准。
- Apply impact: 设为必须可能使不支持 PMF 的客户端无法连接；它也参与 MLO STA 伙伴链路的一致性检查。
- Evidence: `lib/wifi/hostapd.sh:796` (Xiaomi RN02 1.0.43) — 一般 AP 读取 ieee80211w 缺省 0。
- Evidence: `lib/wifi/hostapd.sh:888–905` (Xiaomi RN02 1.0.43) — SAE 分支对 PSK 的 0 提升为 1，其他非企业分支设为 2。
- Evidence: `lib/wifi/hostapd.sh:919–925` (Xiaomi RN02 1.0.43) — OWE 非 WPA/PSK 分支读取 ieee80211w，缺省为 2。
- Evidence: `lib/wifi/hostapd.sh:958` (Xiaomi RN02 1.0.43) — 最终值写入 hostapd ieee80211w。
- Evidence: `lib/wifi/qcawificfg80211.sh:4448–4455` (Xiaomi RN02 1.0.43) — MLO STA 的 PMF 参数不一致会报告 MLD PMF Mismatch。

#### wireless/wifi-iface/bssid

指定 STA/Ad-Hoc 的目标 AP 地址。supplicant 网络块会写入 bssid；QCA 在客户端/Ad-Hoc 分支也用 iwconfig ap 设置目标。

- Flags: hardware-dependent
- Condition: 主要用于 mode=sta/adhoc；须匹配上级 SSID 与认证，MLO 另有 preferred_ap_mld_addr。
- Apply impact: 会限制所连接的上级；目标地址错误或上级更换 BSSID 后可能无法关联，不是 AP 自身地址设置。
- Evidence: `lib/wifi/wpa_supplicant.sh:521–523` (Xiaomi RN02 1.0.43) — 读取 bssid，非空时生成 bssid=<地址>。
- Evidence: `lib/wifi/wpa_supplicant.sh:601–604` (Xiaomi RN02 1.0.43) — 目标 bssid 插入 supplicant network 块。
- Evidence: `lib/wifi/qcawificfg80211.sh:8446–8453` (Xiaomi RN02 1.0.43) — sta/adhoc 分支读取 bssid 并通过 iwconfig ap 下发。

#### wireless/wifi-iface/macfilter

按 maclist 设置第二套厂商 MAC ACL。allow 走允许列表，deny 走拒绝列表；其他值在列表非空时仍走拒绝。

- Compact help: allow/deny 使用第二套 ACL；其他值加非空列表仍走拒绝，disable 不保证停用。
- Format / range: allow / deny；其他值的行为依列表是否为空，未证实显式 disable 命令。
- Flags: hardware-dependent, version-dependent
- Condition: 与 maclist 配套；厂商 backhaul AP 跳过该路径。
- Apply impact: 会改变客户端接入权限；在此脚本中“disable”加非空列表并不保证停用过滤，可能仍拒绝列表内设备。
- Evidence: `lib/wifi/qcawificfg80211.sh:8631–8638` (Xiaomi RN02 1.0.43) — 厂商回程 AP 接口被排除在这段 MAC 过滤设置之外。
- Evidence: `lib/wifi/qcawificfg80211.sh:8649–8661` (Xiaomi RN02 1.0.43) — allow/deny 分别下发 maccmd_sec 1/2；其他值在 maclist 非空时也下发 2。

#### wireless/wifi-iface/maclist

MAC ACL 的客户端地址列表。原厂在非回程 AP 分支先清空第二套 ACL，再逐项调用 addmac_sec。

- Compact help: 非空列表先清空再写第二套 ACL；空列表不保证清除旧运行时项。
- Format / range: 原生 MAC 地址列表；不是一个 IP 地址池。
- Flags: version-dependent
- Condition: 与 macfilter 一起配置；回程 AP 排除在该段处理之外。
- Apply impact: 配合 macfilter 可阻止客户端接入；空列表时此分支不执行清空，不能保证旧运行时列表立刻消失。
- Evidence: `lib/wifi/qcawificfg80211.sh:8632–8638` (Xiaomi RN02 1.0.43) — 按厂商回程接口名称判断是否跳过 MAC ACL。
- Evidence: `lib/wifi/qcawificfg80211.sh:8640–8647` (Xiaomi RN02 1.0.43) — 非空 maclist 先下发 maccmd_sec 3，再遍历地址并调用 addmac_sec。
- Evidence: `lib/wifi/qcawificfg80211.sh:8657–8659` (Xiaomi RN02 1.0.43) — macfilter 未匹配 allow/deny 且 maclist 非空时采用拒绝策略。

#### wireless/wifi-iface/maxassoc

通用字段表示最多关联客户端数。原厂同类控制读取的是 maxsta，并按 2.4/5 GHz 的 misc 最大站点配置覆盖；未找到 maxassoc 映射。

- Compact help: 通用客户端上限字段；原厂读取 maxsta，尚未找到 maxassoc 映射。
- Unit: 客户端数量（通用字段语义）
- Flags: hardware-dependent, version-dependent
- Condition: 原厂已验证入口为 maxsta 与 misc.wireless.if_2g_maxsta/if_5g_maxsta。
- Apply impact: 不能确认修改 maxassoc 会限制 RN02 客户端数量；不能把厂商 maxsta 逻辑的上限或缺省搬到此字段。
- Trace result: 已检索 sbin/wifi、lib/wifi、lib/netifd、初始化脚本与全部反编译 Lua，未发现 maxassoc 或到 maxsta/max_num_sta 的映射。hostapd ELF 有 max_num_sta 字符串，但它不能证明 maxassoc 被消费。
- Evidence: `lib/wifi/qcawificfg80211.sh:8703–8710` (Xiaomi RN02 1.0.43) — 读取 maxsta；随后按频段读取 misc 的最大站点配置，再下发 maxsta 命令。

#### wireless/wifi-iface/dtim_period

DTIM 通知相隔的信标周期数。AP 类路径为空时补入缺省，并向驱动和 hostapd 下发。

- Source fallback: 1；厂商 ap_lp_iot 模式为 41。
- Unit: 信标周期数
- Format / range: 802.11 DTIM 周期 1–255；此脚本未自行检查范围。
- Condition: 用于 AP 类模式；与信标间隔（厂商 bintval）共同决定通知间隔。
- Apply impact: 改变省电客户端接收缓存组播/广播的通知节奏；值越大通常等待越久，但具体耗电和时延取决于客户端。
- Evidence: `lib/wifi/qcawificfg80211.sh:9378–9385` (Xiaomi RN02 1.0.43) — ap_lp_iot 缺省 DTIM 为 41，其他 AP 类模式为 1，缺失时写入。
- Evidence: `lib/wifi/qcawificfg80211.sh:8542–8543` (Xiaomi RN02 1.0.43) — dtim_period 非空时下发同名驱动命令。
- Evidence: `lib/wifi/hostapd.sh:2294–2295` (Xiaomi RN02 1.0.43) — hostapd 生成读取 dtim_period，缺省 1，并写入配置。

#### wireless/wifi-iface/auth_server

AP 企业认证使用的 RADIUS 认证服务器。原厂将其输出为 auth_server_addr，缺失时还尝试旧字段 server。

- Flags: version-dependent
- Condition: AP 企业 WPA/EAP 或 802.1X 认证；配合 auth_port 与 auth_secret。
- Apply impact: 服务器无法访问会使依赖它的企业认证失败；PSK/开放网络不因此自动变为 RADIUS 认证。
- Evidence: `lib/wifi/hostapd.sh:155–157` (Xiaomi RN02 1.0.43) — auth_server 为空时回退 server，并生成 auth_server_addr。
- Evidence: `lib/wifi/hostapd.sh:540–542` (Xiaomi RN02 1.0.43) — 企业 WPA 分支调用 RADIUS 参数生成函数。

#### wireless/wifi-iface/auth_port

RADIUS 认证服务器端口。原厂先读 auth_port，再尝试旧 port，仍为空时使用 1812。

- Source fallback: 1812（auth_port 与旧 port 都为空时）
- Unit: UDP 端口号
- Format / range: 1–65535（网络端口范围；脚本未单独校验）
- Condition: 与 auth_server 配套，仅在 RADIUS 参数生成分支使用。
- Apply impact: 端口需与服务器一致；改错会导致认证请求无法到达，客户端无法完成企业认证。
- Evidence: `lib/wifi/hostapd.sh:160–163` (Xiaomi RN02 1.0.43) — auth_port 为空时读取 port，最终回退 1812，再写入 auth_server_port。

#### wireless/wifi-iface/auth_secret

与 RADIUS 认证服务器约定的共享密钥。生成器读取此字段，缺失时回退 key；它不是 Wi-Fi PSK 的通用替代名称。

- Flags: credential
- Condition: AP 企业认证/802.1X；配合 auth_server、auth_port，留意 key 回退。
- Apply impact: 共享密钥与服务器不一致会导致 RADIUS 校验失败，客户端无法完成相应企业认证。
- Evidence: `lib/wifi/hostapd.sh:164–166` (Xiaomi RN02 1.0.43) — auth_secret 缺失时读取 key，写为 auth_server_shared_secret。
- Evidence: `lib/wifi/hostapd.sh:540–542` (Xiaomi RN02 1.0.43) — 企业认证分支调用 RADIUS 参数生成器。

#### wireless/wifi-iface/acct_server

企业认证 AP 的 RADIUS 计费服务器。只有非空时才写入 acct_server_addr，与认证服务器地址分别配置。

- Condition: 企业认证 RADIUS 生成分支；配合 acct_port 与 acct_secret。
- Apply impact: 会改变计费消息目标；计费可达性和认证可达性是两条配置路径，不保证单改此项启用完整计费。
- Evidence: `lib/wifi/hostapd.sh:176–177` (Xiaomi RN02 1.0.43) — 非空 acct_server 写为 acct_server_addr。
- Evidence: `lib/wifi/hostapd.sh:540–542` (Xiaomi RN02 1.0.43) — 企业 WPA 分支调用包含计费参数的 RADIUS 生成器。

#### wireless/wifi-iface/acct_port

RADIUS 计费端口。此脚本仅当 acct_port 非空才输出；虽然出现 1813 回退表达式，但外层非空条件使空值不会在此补成 1813。

- Compact help: 非空才生成计费端口；源码此处未证实空值默认 1813。
- Unit: UDP 端口号
- Format / range: 1–65535（网络端口范围；脚本未单独校验）
- Flags: version-dependent
- Condition: acct_server、acct_secret 与企业 RADIUS 生成分支；不要把源码的条件表达式误当确定缺省。
- Apply impact: 必须与计费服务器监听端口一致；未填写时该生成器省略端口，后续服务如何补值未由此处证明。
- Evidence: `lib/wifi/hostapd.sh:178–180` (Xiaomi RN02 1.0.43) — acct_port 非空才执行 ${acct_port:-1813}，并且非空才写入 acct_server_port。

#### wireless/wifi-iface/acct_secret

与 RADIUS 计费服务器约定的共享密钥。原厂仅在非空时输出 acct_server_shared_secret，没有从认证密钥回退的代码。

- Flags: credential
- Condition: 配合 acct_server、acct_port；企业 RADIUS 参数生成分支。
- Apply impact: 与计费服务器不一致会导致计费报文校验失败；它与 auth_secret 不应假定相同。
- Evidence: `lib/wifi/hostapd.sh:181–182` (Xiaomi RN02 1.0.43) — 读取 acct_secret，非空时写为 acct_server_shared_secret。

#### wireless/wifi-iface/mesh_id

通用 802.11s 网络标识字段。原厂有同名驱动命令，但其参数来自 xiaoqiang.common.NETWORK_ID，而非已证实的 wifi-iface.mesh_id。

- Compact help: 通用 802.11s 标识；原厂同名命令读取 NETWORK_ID，未证实此 UCI 字段消费。
- Flags: version-dependent
- Condition: 通用语义用于 802.11s；厂商 Mesh 则涉及 miwifi_mesh、NETWORK_ID 与回程配置。
- Apply impact: 此字段是否影响 Xiaomi Mesh 归属尚未确认；厂商 NETWORK_ID 是另一个配置入口，不等同 SSID。
- Trace result: 已检索 lib/wifi、lib/mimesh、usr/sbin Mesh 脚本、netifd 与反编译 Lua；未找到读取 wifi-iface.mesh_id。存在的 mesh_id 驱动命令读取的是 xiaoqiang.common.NETWORK_ID，不证明这个字段被消费。
- Evidence: `lib/wifi/qcawificfg80211.sh:8136–8137` (Xiaomi RN02 1.0.43) — 厂商接口另读取 miwifi_mesh 开关。
- Evidence: `lib/wifi/qcawificfg80211.sh:8142–8148` (Xiaomi RN02 1.0.43) — 按 mesh_cmd 版本读取 NETWORK_ID，并以 0x 前缀下发 mesh_id 命令。
- Evidence: `lib/wifi/qcawificfg80211.sh:7800` (Xiaomi RN02 1.0.43) — 厂商创建路径把 mode=mesh 映射为 __ap。

#### wireless/wifi-iface/mesh_fwding

通用 802.11s 节点转发开关。原厂 mesh 接口创建为 AP 类型；目前未找到 wifi-iface.mesh_fwding 到驱动或 supplicant 的传递。

- Compact help: 通用 802.11s 转发开关；尚未找到它控制 Xiaomi Mesh 回程的路径。
- Flags: version-dependent
- Condition: 通用 802.11s 模式语义不等于原厂 Xiaomi Mesh 回程；需相应模式和消费者支持。
- Apply impact: 此开关是否影响 Xiaomi Mesh 回程转发尚未确认；通用 802.11s 语义不能证明原厂转发路径会停止。
- Trace result: 已检索 sbin/wifi、lib/wifi、lib/mimesh、netifd、初始化及全部反编译 Lua，未出现 mesh_fwding。wpa_supplicant ELF 存在 mesh_fwding 解析字符串，但没有找到此 UCI 字段到该后端选项的转换。
- Evidence: `lib/wifi/qcawificfg80211.sh:7794–7801` (Xiaomi RN02 1.0.43) — 厂商创建接口时把 mesh 等 AP 类模式映射为 __ap。
- Evidence: `lib/wifi/wpa_supplicant.sh:598–605` (Xiaomi RN02 1.0.43) — supplicant network 生成块写入模式、SSID、BSSID 与认证等参数。

## dhcp

### dnsmasq

#### dhcp/dnsmasq/domainneeded

只转发含域名部分的 DNS 查询；短主机名优先留在本地解析。

- Source fallback: 0（未启用）
- Condition: 作用于 dnsmasq 的 DNS 查询转发。
- Apply impact: 会减少向上游泄露短主机名；依赖短名的外部查询可能不再得到答复。
- Evidence: `etc/init.d/dnsmasq:1041` (Xiaomi RN02 1.0.43) — domainneeded 被转换为 --domain-needed。
- Evidence: `etc/init.d/dnsmasq:159–167` (Xiaomi RN02 1.0.43) — append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。
- Evidence: `docs/field-help-dhcp-research.md:80` (Xiaomi RN02 1.0.43) — 内置帮助说明不转发没有域名部分的查询。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 207858.

#### dhcp/dnsmasq/boguspriv

通用 dnsmasq 用此项过滤未在本地找到的私网反向查询；本固件读取此项的代码已被注释，不能确认开关生效。

- Compact help: 原厂读取代码已注释；不能确认修改此项会改变私网反向查询。
- Flags: version-dependent
- Apply impact: 仅修改此字段未必改变反向查询行为；不要据此判断私网 PTR 查询已经被拦截。
- Trace result: 已检索 1.0.43 dnsmasq 启动脚本、lib shell、厂商文本脚本与反编译 Lua；未找到仍执行的 boguspriv 配置读取。父级保存的 1.0.64 公共 dnsmasq 脚本也有相同注释。未覆盖额外配置文件或未反编译二进制。
- Evidence: `etc/init.d/dnsmasq:1252–1255` (Xiaomi RN02 1.0.43) — boguspriv 的 config_get_bool 行被注释；后续仅检查同名变量后追加 --bogus-priv。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/dnsmasq:1252–1255` (Xiaomi RN02 1.0.64) — 已保存的 1.0.64 公共脚本仍将 boguspriv 配置读取注释，只检查变量后追加 --bogus-priv。

#### dhcp/dnsmasq/filterwin2k

传入 --filterwin2k，过滤旧式 Windows 会触发的部分 DNS 查询。

- Source fallback: 0（未启用）
- Flags: legacy
- Apply impact: 可能减少无用上游请求，也可能影响仍依赖这些记录的旧客户端。
- Evidence: `etc/init.d/dnsmasq:1042` (Xiaomi RN02 1.0.43) — filterwin2k 被转换为 --filterwin2k。
- Evidence: `etc/init.d/dnsmasq:159–167` (Xiaomi RN02 1.0.43) — append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。
- Evidence: `docs/field-help-dhcp-research.md:81` (Xiaomi RN02 1.0.43) — 内置帮助说明不转发 Windows 主机的无用 DNS 请求。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 208005.

#### dhcp/dnsmasq/localise_queries

按接收查询的接口本地化 DNS 答复，适合多网段下有多个本地地址的主机记录。

- Source fallback: 0（未启用）
- Condition: 主要影响 dnsmasq 读取的本地主机记录。
- Apply impact: 多网段内同一主机名可能得到不同地址；不用于替换上游 DNS 服务器。
- Evidence: `etc/init.d/dnsmasq:1048` (Xiaomi RN02 1.0.43) — localise_queries 被转换为 --localise-queries。
- Evidence: `etc/init.d/dnsmasq:159–167` (Xiaomi RN02 1.0.43) — append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。
- Evidence: `docs/field-help-dhcp-research.md:82` (Xiaomi RN02 1.0.43) — 内置帮助说明根据收到查询的接口回答 DNS。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 210858.

#### dhcp/dnsmasq/rebind_protection

启用 DNS 重绑定保护，过滤上游答复中的 RFC1918 私网地址。需要解析到内网的域名可用 rebind_domain 指定例外。

- Source fallback: 1（启用）
- Condition: rebind_localhost 与 rebind_domain 仅在此项启用时读取。
- Apply impact: 会拦截部分确实指向内网的域名；可信例外应通过 rebind_domain 单独设置。
- Evidence: `etc/init.d/dnsmasq:1162–1168` (Xiaomi RN02 1.0.43) — rebind_protection 缺省为 1；启用后追加 --stop-dns-rebind，并记录丢弃上游 RFC1918 响应。
- Evidence: `docs/field-help-dhcp-research.md:102` (Xiaomi RN02 1.0.43) — 内置帮助说明解析时过滤私网地址以阻止 DNS 重绑定。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 212890.

#### dhcp/dnsmasq/rebind_localhost

允许 DNS 重绑定保护中的 127.0.0.0/8 回环地址例外。

- Source fallback: 0（不放行）
- Condition: 需启用 rebind_protection。
- Apply impact: 开启后，外部 DNS 可返回回环地址；不会同时放行全部 RFC1918 私网地址。
- Evidence: `etc/init.d/dnsmasq:1163–1175` (Xiaomi RN02 1.0.43) — rebind_localhost 位于重绑定保护分支内，缺省为 0；为真时追加 --rebind-localhost-ok，并说明允许 127.0.0.0/8。
- Evidence: `docs/field-help-dhcp-research.md:103` (Xiaomi RN02 1.0.43) — 内置帮助明确允许 127.0.0.0/8 重绑定例外。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 212951.

#### dhcp/dnsmasq/rebind_domain

按域名列表放行重绑定保护，否则可能被丢弃的私网 DNS 答复。

- Condition: 需启用 rebind_protection。
- Condition: 保留 dnsmasq 原生域名匹配语法和列表边界。
- Apply impact: 指定域名可解析到内网地址；例外范围过大会削弱重绑定保护。
- Evidence: `etc/init.d/dnsmasq:1177–1182` (Xiaomi RN02 1.0.43) — rebind_domain 的每项转换为 --rebind-domain-ok；日志说明允许该域名的 RFC1918 响应。
- Evidence: `docs/field-help-dhcp-research.md:104` (Xiaomi RN02 1.0.43) — 内置帮助说明对指定域名停用重绑定保护。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 213000.

#### dhcp/dnsmasq/local

传入 dnsmasq 的本地域匹配规则，例如 /lan/，该域不走普通上游转发。

- Condition: 与本地 hosts、DHCP 名称和 server 的域名转发规则配合。
- Apply impact: 本地域缺失的记录可能不会继续向公网查询；厂商 pdnsd 切换流程也会改动此项。
- Evidence: `etc/init.d/dnsmasq:1087` (Xiaomi RN02 1.0.43) — local 的非空值传入 --local。
- Evidence: `usr/sbin/sysapi:1008–1010` (Xiaomi RN02 1.0.43) — sysapi 的 pdnsd 关闭分支删除 local 并写入 resolvfile。
- Evidence: `docs/field-help-dhcp-research.md:105` (Xiaomi RN02 1.0.43) — 内置帮助说明永不向上游转发指定域名查询。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 209990.

#### dhcp/dnsmasq/domain

设置 DHCP 与本地主机使用的域名后缀，并可用于路由器自身的搜索域。

- Condition: expandhosts 控制 hosts 短名的扩展。
- Condition: 静态租约 dns=1 时也使用此后缀。
- Apply impact: 会改变 DHCP 客户端域名、本地记录的完整名称及路由器搜索路径。
- Evidence: `etc/init.d/dnsmasq:1086` (Xiaomi RN02 1.0.43) — domain 传入 --domain。
- Evidence: `etc/init.d/dnsmasq:372–375` (Xiaomi RN02 1.0.43) — 静态租约生成 hosts 记录时将 DOMAIN 追加到名称。
- Evidence: `etc/init.d/dnsmasq:1369–1373` (Xiaomi RN02 1.0.43) — localuse 与 ADD_LOCAL_DOMAIN 开启且 DOMAIN 非空时，在 resolv.conf 写入 search DOMAIN。
- Evidence: `docs/field-help-dhcp-research.md:106` (Xiaomi RN02 1.0.43) — 内置帮助说明指定 DHCP 租约分配的域名。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 210053.

#### dhcp/dnsmasq/expandhosts

给 hosts 文件中的短名称追加 domain 指定的域名后缀。

- Source fallback: 0（未启用）
- Condition: 需设置适合本网络的 domain。
- Apply impact: 同一 hosts 地址可用完整本地域名查询；domain 不正确会产生错误名称。
- Evidence: `etc/init.d/dnsmasq:1060` (Xiaomi RN02 1.0.43) — expandhosts 被转换为 --expand-hosts。
- Evidence: `etc/init.d/dnsmasq:159–167` (Xiaomi RN02 1.0.43) — append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。
- Evidence: `docs/field-help-dhcp-research.md:83` (Xiaomi RN02 1.0.43) — 内置帮助说明给 /etc/hosts 的简单名称追加域后缀。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 207951.

#### dhcp/dnsmasq/authoritative

将 dnsmasq 声明为其 DHCPv4 网络的权威地址服务器。

- Source fallback: 0（未启用）
- Condition: 需由 dnsmasq 实际提供 DHCP；maindhcp 可能让地址服务交给 odhcpd。
- Apply impact: 可更快处理旧地址请求；同网段还有其他 DHCP 服务器时可能出现错误拒绝或地址冲突。
- Evidence: `etc/init.d/dnsmasq:1039` (Xiaomi RN02 1.0.43) — authoritative 被转换为 --dhcp-authoritative。
- Evidence: `etc/init.d/dnsmasq:159–167` (Xiaomi RN02 1.0.43) — append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。
- Evidence: `docs/field-help-dhcp-research.md:84` (Xiaomi RN02 1.0.43) — 内置帮助说明假设本机是本地网络唯一 DHCP 服务器。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 209042.

#### dhcp/dnsmasq/readethers

读取 /etc/ethers 的 MAC 与地址映射，作为静态 DHCP 分配依据。

- Source fallback: 0（未启用）
- Condition: 需有可读取且格式正确的 /etc/ethers。
- Apply impact: ethers 内容会影响匹配客户端的固定地址；与 host 租约配置应保持一致。
- Evidence: `etc/init.d/dnsmasq:1049` (Xiaomi RN02 1.0.43) — readethers 被转换为 --read-ethers。
- Evidence: `etc/init.d/dnsmasq:1396` (Xiaomi RN02 1.0.43) — dnsmasq 的 jail 挂载包括 /etc/ethers。
- Evidence: `etc/init.d/dnsmasq:159–167` (Xiaomi RN02 1.0.43) — append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。
- Evidence: `docs/field-help-dhcp-research.md:85` (Xiaomi RN02 1.0.43) — 内置帮助说明读取文件中的 DHCP 静态主机信息；路径占位符没有作为默认值推断。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 211100.

#### dhcp/dnsmasq/leasefile

设置 dnsmasq 保存动态租约的文件，启动时会创建缺失文件并允许服务写入。

- Source fallback: /tmp/dhcp.leases
- Condition: 文件所在路径须可写；/tmp 下的租约不跨重启持久保存。
- Apply impact: 会改变租约保存和读取位置；不可写路径会影响租约持久记录。厂商设备列表也读取此路径。
- Evidence: `etc/init.d/dnsmasq:1115` (Xiaomi RN02 1.0.43) — leasefile 传入 --dhcp-leasefile，明确缺省为 /tmp/dhcp.leases。
- Evidence: `etc/init.d/dnsmasq:1144–1145` (Xiaomi RN02 1.0.43) — 缺失的租约文件通过 touch 创建。
- Evidence: `etc/init.d/dnsmasq:1397` (Xiaomi RN02 1.0.43) — leasefile 被加入 jail 可写挂载。

#### dhcp/dnsmasq/resolvfile

通用配置指定上游 DNS 文件；本固件启动时不读取这个值，而固定使用 /tmp/resolv.conf.auto。停止流程仍会读取此项。

- Compact help: 原厂启动时固定使用 /tmp/resolv.conf.auto；此字段仍参与停止处理。
- Flags: version-dependent
- Condition: noresolv=1 时不使用启动脚本的上游解析文件。
- Apply impact: 自定义路径不会按通用 OpenWrt 预期改变启动时的上游 DNS；也可能改变停止服务后恢复 resolv.conf 的判定。
- Trace result: 已核对 1.0.43 dnsmasq 启动、停止与 sysapi pdnsd 流程；resolvfile 有写入和停止读取，但启动读取被注释。父级保存的 1.0.64 公共 dnsmasq 脚本此分支相同；未检查 1.0.64 私有配置或实际解析效果。
- Evidence: `etc/init.d/dnsmasq:1148–1154` (Xiaomi RN02 1.0.43) — noresolv 未启用时，读取 resolvfile 的代码被注释，脚本固定使用 /tmp/resolv.conf.auto。
- Evidence: `etc/init.d/dnsmasq:1407–1412` (Xiaomi RN02 1.0.43) — 停止流程读取 resolvfile，并在其为 /tmp/resolv.conf.auto 且 noresolv=0 时默认恢复系统 resolv.conf。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/dnsmasq:1148–1154` (Xiaomi RN02 1.0.64) — 已保存的 1.0.64 公共脚本仍固定使用 /tmp/resolv.conf.auto，未恢复 resolvfile 的启动配置读取。

#### dhcp/dnsmasq/noresolv

不读取上游解析文件；仍可使用 server 等显式 DNS 转发规则。

- Source fallback: 0（读取解析文件）
- Condition: 启用时应检查 server 或其他显式上游来源。
- Apply impact: 如果没有可用的显式上游，非本地域名可能无法解析。
- Evidence: `etc/init.d/dnsmasq:1047` (Xiaomi RN02 1.0.43) — noresolv 被转换为 --no-resolv。
- Evidence: `etc/init.d/dnsmasq:1148–1156` (Xiaomi RN02 1.0.43) — noresolv 缺省为 0；仅在它不是 1 时添加固定上游解析文件。
- Evidence: `docs/field-help-dhcp-research.md:86` (Xiaomi RN02 1.0.43) — 内置帮助说明不读取 resolv.conf。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 209714.

#### dhcp/dnsmasq/nohosts

不读取系统 /etc/hosts；与额外 addnhosts 文件的开关分开。

- Source fallback: 0（读取系统 hosts）
- Apply impact: 系统 hosts 中的本地名称可能不再解析；不会自动停用脚本生成的额外主机记录。
- Evidence: `etc/init.d/dnsmasq:1043` (Xiaomi RN02 1.0.43) — nohosts 被转换为 --no-hosts。
- Evidence: `etc/init.d/dnsmasq:1105–1113` (Xiaomi RN02 1.0.43) — 启动脚本仍独立添加生成的 HOSTFILE 及 addnhosts。
- Evidence: `etc/init.d/dnsmasq:159–167` (Xiaomi RN02 1.0.43) — append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。

#### dhcp/dnsmasq/nonwildcard

启用 --bind-dynamic，只绑定当前允许的接口地址，并跟随地址变化。

- Source fallback: 1（启用）
- Condition: 监听范围还受 interface、notinterface、listen_address 影响。
- Apply impact: 有助于避免与其他 DNS 实例争用监听地址；接口选择不当会让客户端失去 DNS 服务。
- Evidence: `etc/init.d/dnsmasq:1064` (Xiaomi RN02 1.0.43) — nonwildcard 转换为 --bind-dynamic，明确缺省为 1。
- Evidence: `docs/field-help-dhcp-research.md:108` (Xiaomi RN02 1.0.43) — 内置帮助说明绑定已使用的接口并检查新接口。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 214766.

#### dhcp/dnsmasq/localservice

限制 DNS 查询来源为本机所连接的本地子网。

- Source fallback: 0（未启用）
- Condition: dnsmasq 的显式监听接口或地址设置可能影响该开关的适用范围。
- Apply impact: 跨路由远程客户端可能被拒绝；这是 DNS 来源限制，不等于 DHCP 地址池 ignore。
- Evidence: `etc/init.d/dnsmasq:1067` (Xiaomi RN02 1.0.43) — localservice 被转换为 --local-service。
- Evidence: `etc/init.d/dnsmasq:159–167` (Xiaomi RN02 1.0.43) — append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。
- Evidence: `docs/field-help-dhcp-research.md:87` (Xiaomi RN02 1.0.43) — 内置帮助说明只接受直接连接网络的查询。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 216010.

#### dhcp/dnsmasq/strictorder

要求 dnsmasq 按上游列表顺序尝试 DNS 服务器。

- Source fallback: 0（未启用）
- Condition: 检查 server 列表和自动上游 DNS 的顺序。
- Apply impact: 第一台上游较慢或故障时可能增加等待；与 allservers 的并发意图不同。
- Evidence: `etc/init.d/dnsmasq:1045` (Xiaomi RN02 1.0.43) — strictorder 被转换为 --strict-order。
- Evidence: `etc/init.d/dnsmasq:159–167` (Xiaomi RN02 1.0.43) — append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。
- Evidence: `docs/field-help-dhcp-research.md:88` (Xiaomi RN02 1.0.43) — 内置帮助说明严格按给定顺序使用名称服务器。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 209368.

#### dhcp/dnsmasq/allservers

向所有可用上游 DNS 服务器并发发送查询，采用先返回的答复。

- Source fallback: 0（未启用）
- Condition: 与 strictorder 的顺序查询策略一并检查。
- Apply impact: 增加上游查询量和隐私暴露范围；不同上游回答不一致时结果可能变化。
- Evidence: `etc/init.d/dnsmasq:1071` (Xiaomi RN02 1.0.43) — allservers 被转换为 --all-servers。
- Evidence: `etc/init.d/dnsmasq:159–167` (Xiaomi RN02 1.0.43) — append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。
- Evidence: `docs/field-help-dhcp-research.md:89` (Xiaomi RN02 1.0.43) — 内置帮助说明每次都向全部服务器发送 DNS 查询。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 213046.

#### dhcp/dnsmasq/logqueries

记录 DNS 查询，并使用 extra 格式携带更详细的查询上下文。

- Source fallback: 0（未启用）
- Apply impact: 日志会增长，并可包含客户端地址和查询域名；适合短期排错。
- Evidence: `etc/init.d/dnsmasq:1046` (Xiaomi RN02 1.0.43) — logqueries 被转换为 --log-queries=extra。
- Evidence: `etc/init.d/dnsmasq:159–167` (Xiaomi RN02 1.0.43) — append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。
- Evidence: `docs/field-help-dhcp-research.md:90` (Xiaomi RN02 1.0.43) — 内置帮助说明记录 DNS 查询。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 209644.

#### dhcp/dnsmasq/logdhcp

记录 DHCP 分配与处理的详细信息。

- Source fallback: 0（未启用）
- Condition: 仅对 dnsmasq 实际处理的 DHCP 有作用。
- Apply impact: 增加日志量，并可记录客户端标识与租约信息；不会直接扩大地址池。
- Evidence: `etc/init.d/dnsmasq:1068` (Xiaomi RN02 1.0.43) — logdhcp 被转换为 --log-dhcp。
- Evidence: `etc/init.d/dnsmasq:159–167` (Xiaomi RN02 1.0.43) — append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。
- Evidence: `docs/field-help-dhcp-research.md:91` (Xiaomi RN02 1.0.43) — 内置帮助说明额外 DHCP 日志。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 212801.

#### dhcp/dnsmasq/port

设置 dnsmasq 的 DNS 监听端口；0 表示关闭 DNS 功能，不等于停用 DHCP。

- Source fallback: 53（dnsmasq 内置帮助明确说明）
- Unit: 端口号
- Format / range: 0–65535；0 关闭 DNS
- Condition: 检查防火墙放行规则及其他 DNS 服务的端口占用。
- Apply impact: 客户端通常访问 53 端口；改为其他端口需要配套转发或客户端设置，DNS 功能关闭会影响普通解析。
- Evidence: `etc/init.d/dnsmasq:1080` (Xiaomi RN02 1.0.43) — port 的非空值传入 --port；此处未指定数值缺省。
- Evidence: `docs/field-help-dhcp-research.md:92` (Xiaomi RN02 1.0.43) — 内置帮助明确 DNS 监听端口默认是 53。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 209520.

#### dhcp/dnsmasq/queryport

设置上游 DNS 查询的来源端口。0 启用单端口复用模式，由系统或端口范围策略选取端口；不等于留空时的随机查询端口策略。

- Compact help: 0 开启单端口复用，不等于留空时的默认随机端口策略。
- Unit: 端口号
- Format / range: 0–65535
- Condition: 影响 DNS 上游请求；与监听 port 无关。
- Condition: 0 值选取端口时仍受 minport/maxport 范围策略影响；socket 按来源地址及接口条件匹配复用。
- Apply impact: 显式端口或 0 的复用模式会减少上游查询来源端口的变化，可能降低 DNS 抗伪造能力；复用受来源地址及接口条件限制，不代表所有接口共用一个端口。
- Evidence: `etc/init.d/dnsmasq:1083–1085` (Xiaomi RN02 1.0.43) — queryport 传入 --query-port，与 minport/maxport 分别配置；此处没有数值缺省。
- Evidence: `docs/field-help-dhcp-research.md:93` (Xiaomi RN02 1.0.43) — 内置帮助说明强制指定上游 DNS 查询的来源端口。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 209661.
- Evidence: `docs/field-help-queryport-review.md:35–50` (Xiaomi RN02 1.0.43) — query-port=0 分支将共享查询 socket 模式标志设为 1，不是省略选项或恢复默认。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 89592.
- Evidence: `docs/field-help-queryport-review.md:54–85` (Xiaomi RN02 1.0.43) — 共享查询 socket 列表按来源地址及接口/名称条件匹配，匹配时复用已有 socket；不是每次查询重新分配端口。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 112160.
- Evidence: `docs/field-help-queryport-review.md:89–104` (Xiaomi RN02 1.0.43) — 新建 socket 的来源端口为 0 时，有端口范围则在范围内选取；没有范围时由系统选取，之后可复用该 socket。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 107156.

#### dhcp/dnsmasq/cachesize

设置 DNS 缓存条目上限；0 关闭常规 DNS 缓存。

- Unit: 条
- Format / range: 非负整数
- Apply impact: 较大缓存会占用更多内存；较小或关闭缓存会增加重复查询及上游延迟。
- Evidence: `etc/init.d/dnsmasq:1078` (Xiaomi RN02 1.0.43) — cachesize 的非空值传入 --cache-size；此处没有数值缺省。
- Evidence: `docs/field-help-dhcp-research.md:94` (Xiaomi RN02 1.0.43) — 内置帮助说明缓存大小按条目计数；默认数值仅是未解析的占位符。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 207693.

#### dhcp/dnsmasq/dnsforwardmax

限制同时等待上游答复的 DNS 转发请求数量。

- Unit: 个并发请求
- Format / range: 非负整数；0 的具体处理未在启动脚本中说明。
- Apply impact: 上限过低时忙碌网络的 DNS 请求可能排队或失败；增大上限会增加资源占用。
- Evidence: `etc/init.d/dnsmasq:1079` (Xiaomi RN02 1.0.43) — dnsforwardmax 的非空值传入 --dns-forward-max；此处没有数值缺省。
- Evidence: `docs/field-help-dhcp-research.md:95` (Xiaomi RN02 1.0.43) — 内置帮助说明控制最大并发 DNS 查询数，默认数值未在该字符串解析。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 212009.

#### dhcp/dnsmasq/dhcpleasemax

设置 dnsmasq 同时记录的 DHCP 租约数量上限，不是每个地址池的地址数。

- Unit: 条租约
- Format / range: 非负整数；0 的具体处理未在启动脚本中说明。
- Condition: 仅用于 dnsmasq 提供的 DHCP 租约。
- Apply impact: 租约表耗尽时新客户端可能无法获得地址；应与各池 limit 和内存容量协调。
- Evidence: `etc/init.d/dnsmasq:1082` (Xiaomi RN02 1.0.43) — dhcpleasemax 的非空值传入 --dhcp-lease-max；此处没有数值缺省。
- Evidence: `docs/field-help-dhcp-research.md:96` (Xiaomi RN02 1.0.43) — 内置帮助说明控制最大 DHCP 租约数，默认数值未在该字符串解析。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 210800.

#### dhcp/dnsmasq/ednspacket_max

设置 dnsmasq 允许的 EDNS UDP DNS 数据包最大大小。

- Unit: 字节
- Format / range: 面板范围 512–65535；固件启动脚本未校验数值。
- Apply impact: 较大的 UDP 数据包可能被路径 MTU 或防火墙限制；过小可能增加截断与 TCP 重试。
- Evidence: `etc/init.d/dnsmasq:1081` (Xiaomi RN02 1.0.43) — ednspacket_max 的非空值传入 --edns-packet-max；此处没有数值缺省。
- Evidence: `docs/field-help-dhcp-research.md:97` (Xiaomi RN02 1.0.43) — 内置帮助说明最大支持的 EDNS.0 UDP 包大小，默认数值未在该字符串解析。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 209581.

#### dhcp/dnsmasq/server

指定上游 DNS 服务器，保留 /域名/服务器#端口 等 dnsmasq 原生匹配语法及每个列表项。

- Condition: noresolv=1 时尤其需要可用的显式上游。
- Condition: allservers 和 strictorder 影响上游选择策略。
- Apply impact: 可把特定域名送往不同上游；错误地址、端口或匹配规则会造成解析失败。
- Evidence: `etc/init.d/dnsmasq:181–183` (Xiaomi RN02 1.0.43) — append_server 将每项原样追加为 --server。
- Evidence: `etc/init.d/dnsmasq:1089` (Xiaomi RN02 1.0.43) — server 通过 config_list_foreach 逐项调用 append_server。
- Evidence: `docs/field-help-dhcp-research.md:98` (Xiaomi RN02 1.0.43) — 内置帮助说明配置可带域名匹配条件的上游服务器地址。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 209827.

#### dhcp/dnsmasq/address

按原生 /域名/IP地址 规则直接回答 DNS 查询，不需要访问上游。

- Condition: 区分固定 address 答复与 server 上游转发规则。
- Apply impact: 匹配域名及其范围内的查询会被重写；范围过大会让无关站点指向错误地址。
- Evidence: `etc/init.d/dnsmasq:189–191` (Xiaomi RN02 1.0.43) — append_address 将每项原样追加为 --address。
- Evidence: `etc/init.d/dnsmasq:1091` (Xiaomi RN02 1.0.43) — address 通过 config_list_foreach 逐项转换。

#### dhcp/dnsmasq/interface

按列表选择服务接口；先将逻辑网络名转换为设备名，无法转换时保留原值。

- Condition: 与 notinterface、listen_address、nonwildcard 一并检查。
- Condition: boot 阶段的生成路径不会读取此列表。
- Apply impact: 会改变 DNS/DHCP 可服务的网卡范围；漏选 LAN 可能让本地客户端失去服务。
- Evidence: `etc/init.d/dnsmasq:201–204` (Xiaomi RN02 1.0.43) — append_interface 使用 network_get_device；失败时沿用输入，再生成 --interface。
- Evidence: `etc/init.d/dnsmasq:1101–1104` (Xiaomi RN02 1.0.43) — 非 BOOT 分支读取 interface 与 notinterface 列表。
- Evidence: `docs/field-help-dhcp-research.md:99` (Xiaomi RN02 1.0.43) — 内置帮助说明选择监听接口。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 208551.

#### dhcp/dnsmasq/notinterface

按列表排除服务接口；逻辑网络名称会转换成设备名后传给 --except-interface。

- Condition: 应与 interface 的允许列表核对；boot 生成路径不读取此列表。
- Apply impact: 可避免在 WAN 等接口暴露服务；排除客户端所在接口会造成 DNS 或 DHCP 不可达。
- Evidence: `etc/init.d/dnsmasq:210–213` (Xiaomi RN02 1.0.43) — append_notinterface 将逻辑网络转换为设备，失败时沿用输入，并生成 --except-interface。
- Evidence: `etc/init.d/dnsmasq:1101–1104` (Xiaomi RN02 1.0.43) — 非 BOOT 分支逐项读取 notinterface。
- Evidence: `docs/field-help-dhcp-research.md:100` (Xiaomi RN02 1.0.43) — 内置帮助说明选择不监听的接口。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 208586.

#### dhcp/dnsmasq/listen_address

按列表指定本机 DNS 监听 IP 地址，值原样传给 dnsmasq。

- Condition: 与 interface、notinterface 和 nonwildcard 共同决定监听范围。
- Apply impact: 地址不属于本机或缺少客户端可达地址时，DNS 可能无法启动或无法访问。
- Evidence: `etc/init.d/dnsmasq:206–208` (Xiaomi RN02 1.0.43) — append_listenaddress 为每项生成 --listen-address。
- Evidence: `etc/init.d/dnsmasq:1088` (Xiaomi RN02 1.0.43) — listen_address 通过 config_list_foreach 读取。

#### dhcp/dnsmasq/addnhosts

指定额外 hosts 文件；启动脚本把路径加入服务挂载并逐项传给 dnsmasq。

- Condition: 每项是可读取的 hosts 文件路径，不是域名列表。
- Apply impact: 额外文件中的名称可覆盖正常上游解析预期；缺失或不可读文件会缺少相应记录。
- Evidence: `etc/init.d/dnsmasq:229–232` (Xiaomi RN02 1.0.43) — append_addnhosts 将路径加入 EXTRA_MOUNT，并追加 --addn-hosts。
- Evidence: `etc/init.d/dnsmasq:1113` (Xiaomi RN02 1.0.43) — addnhosts 列表逐项调用 append_addnhosts。
- Evidence: `docs/field-help-dhcp-research.md:101` (Xiaomi RN02 1.0.43) — 内置帮助说明额外读取 hosts 文件。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 208465.

#### dhcp/dnsmasq/dhcp_option

为此 dnsmasq 实例添加全局 DHCP 选项；保留编号、值、逗号及独立列表项。

- Condition: 接口段 dhcp_option 有网络标签；与全局选项共同检查。
- Apply impact: 可改变所有匹配客户端的 DNS、网关等网络参数；错误选项会让已获地址的客户端仍无法联网。
- Evidence: `etc/init.d/dnsmasq:1204–1205` (Xiaomi RN02 1.0.43) — dnsmasq 段调用 dhcp_option_add，网络标签为空；普通和强制选项各调用一次。
- Evidence: `etc/init.d/dnsmasq:769–791` (Xiaomi RN02 1.0.43) — dhcp_option_add 优先读取原生列表，兼容但警告旧式 option 字符串，并逐项生成 DHCP 选项。
- Evidence: `docs/field-help-dhcp-research.md:107` (Xiaomi RN02 1.0.43) — 内置帮助说明配置发给 DHCP 客户端的选项。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 209419.

### dhcp

#### dhcp/dhcp/interface

关联 /etc/config/network 的逻辑网络；dnsmasq 必须找到其设备与子网后才生成地址池。

- Condition: 关联网络必须存在并处于可用状态。
- Condition: ignore、force 与 DHCP 服务归属共同决定是否实际分配。
- Apply impact: 改错网络名会使地址池被跳过；除厂商 LAN AP 别名分支外，dnsmasq 仅处理 static 接口。
- Evidence: `etc/init.d/dnsmasq:574–587` (Xiaomi RN02 1.0.43) — interface 必须非空并可通过 network_get_device 获得设备；有别名时尝试对应 _alias 网络。
- Evidence: `etc/init.d/dnsmasq:599–606` (Xiaomi RN02 1.0.43) — LAN AP 别名分支使用别名子网，否则要求网络协议为 static。
- Evidence: `docs/field-help-dhcp-research.md:15` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 interface 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 67984.

#### dhcp/dhcp/start

地址池起点相对于接口子网的偏移量，不是完整 IPv4 地址。

- Source fallback: 100
- Unit: 个地址偏移
- Format / range: 须落在关联 IPv4 子网的可用地址范围；脚本未设置固定 65535 上限。
- Condition: 通常与 limit、接口 IPv4 地址和 netmask 一起计算。
- Condition: CPE 桥接模式可使用上游生成的固定地址范围。
- Apply impact: 会移动可分配地址范围；若同时存在厂商 startip/endip，偏移量路径会被绕过。
- Evidence: `etc/init.d/dnsmasq:640–645` (Xiaomi RN02 1.0.43) — start 的明确回退为 100；脚本也读取厂商 startip/endip。
- Evidence: `etc/init.d/dnsmasq:664–672` (Xiaomi RN02 1.0.43) — 没有完整 startip/endip 时，将 start 经 dhcp_calc 转换并传给 ipcalc.sh；有完整起止地址时直接使用它们。

#### dhcp/dhcp/limit

偏移量地址池最多包含的 IPv4 地址数量。

- Source fallback: 150
- Unit: 个地址
- Format / range: 非负整数；需适配子网，0 不走通常的减 1 路径。
- Condition: 与 start 和 netmask 共同计算范围；dynamicdhcp=0 将范围改为静态分配。
- Apply impact: 普通偏移池的末地址超过子网广播地址减 1 时会被裁剪，实际数量可能小于 limit；仍需避免覆盖已有静态地址。完整 startip/endip 走另一路径，不使用 limit，也不经过此裁剪。0 不能当作停用池。
- Evidence: `etc/init.d/dnsmasq:641–643` (Xiaomi RN02 1.0.43) — limit 明确回退为 150。
- Evidence: `etc/init.d/dnsmasq:664–667` (Xiaomi RN02 1.0.43) — 普通偏移池仅在 limit 大于 0 时减 1，随后将 start 和 limit 传给 ipcalc.sh 计算地址范围。
- Evidence: `bin/ipcalc.sh:49–55` (Xiaomi RN02 1.0.43) — 起点不低于 network+1；末地址按 start 加数量偏移计算，超过广播地址减 1 时被裁到该值。
- Evidence: `etc/init.d/dnsmasq:668–672` (Xiaomi RN02 1.0.43) — 完整 startip/endip 路径直接赋值 START/END，不使用 limit，也不调用普通偏移池的 ipcalc.sh。

#### dhcp/dhcp/leasetime

设置客户端可使用租用地址的时间。12h 表示 12 小时；原厂将此原生字符串传给 dnsmasq，特定客户端可另设 host.leasetime。

- Source fallback: 12h（普通地址池回退）
- Unit: 时间字符串：s、m、h 等单位或 infinite
- Condition: host.leasetime 可为特定客户端另设租期。
- Condition: 厂商 CPE 桥接路径使用 120 秒，不采用普通池回退。
- Apply impact: 更短的租约会增加续租请求；更长的租约会让旧地址占用和配置切换持续更久。
- Evidence: `etc/init.d/dnsmasq:640–643` (Xiaomi RN02 1.0.43) — 普通地址池路径中 leasetime 明确回退为 12h。
- Evidence: `etc/init.d/dnsmasq:628–636` (Xiaomi RN02 1.0.43) — CPE 桥接且获得上游地址时，将 leasetime 设置为 120。
- Evidence: `etc/init.d/dnsmasq:682–683` (Xiaomi RN02 1.0.43) — leasetime 被追加到 --dhcp-range。

#### dhcp/dhcp/ignore

让 dnsmasq 在关联设备上禁用 DHCP，并立即跳过该地址池。LAN 此值还会被原厂启动前处理按运行模式重写。

- Compact help: 停用地址池；LAN 值可能被原厂启动前处理按 AP/中继模式重写。
- Source fallback: 0（不忽略）
- Flags: generated, version-dependent
- Condition: interface 指定被忽略的接口；不用此项判断 odhcpd 的 IPv6 模式。
- Condition: LAN 启动前重写受 misc.features.SkipForceDhcp、厂商运行模式及 miwifi_force_ignore 影响。
- Apply impact: 客户端将不能从此池自动获址；DNS 监听不因此自动关闭。手动修改 LAN ignore 可能在启动前被 AP/中继模式及 miwifi_force_ignore 的判定覆盖。
- Evidence: `etc/init.d/dnsmasq:593–597` (Xiaomi RN02 1.0.43) — ignore 通过 append_bool 生成 --no-dhcp-interface，随后提前返回。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua:6445–6451` (Xiaomi RN02 1.0.43) — 厂商 IPv6 上游配置写入 dhcp 段 ignore=1。
- Evidence: `etc/init.d/dnsmasq:159–167` (Xiaomi RN02 1.0.43) — append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。
- Evidence: `etc/init.d/dnsmasq:1442–1457` (Xiaomi RN02 1.0.43) — 除 SkipForceDhcp=1 外，启动前按 AP/中继模式和 miwifi_force_ignore 重写并提交 dhcp.lan.ignore。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/dnsmasq:1450–1458` (Xiaomi RN02 1.0.64) — 1.0.64 仍重写 LAN ignore，并新增 NETMODE 为空、product=ap 且 ft_mode=0 时忽略 DHCP 的分支。

#### dhcp/dhcp/force

跳过同接口已有 DHCP 服务器的检测，仍生成本机地址池。

- Source fallback: 0（保留检测）
- Condition: 仍需 interface 可用、ignore=0 且服务提供 DHCP。
- Apply impact: 同一网段多个服务器可能竞争分配，造成网关或地址冲突；并非强制发送 DHCP 选项的开关。
- Evidence: `etc/init.d/dnsmasq:609–616` (Xiaomi RN02 1.0.43) — force 明确回退为 0；大于 0 时跳过 dhcp_check，否则发现其他服务器就返回。
- Evidence: `etc/init.d/dnsmasq:125–140` (Xiaomi RN02 1.0.43) — dhcp_check 检查设备状态并通过 udhcpc 发起一次 DHCP 检测，将结果保存到标记文件。

#### dhcp/dhcp/dynamicdhcp

允许向没有固定租约的客户端动态分配地址。关闭后地址池被设为 static。

- Source fallback: 1（动态分配）
- Condition: 静态分配需有相应 host 租约及可用地址。
- Apply impact: 关闭后未匹配静态租约的客户端可能无法自动联网；不会删除已有 host 配置。
- Evidence: `etc/init.d/dnsmasq:618–619` (Xiaomi RN02 1.0.43) — dynamicdhcp 明确回退为 1。
- Evidence: `etc/init.d/dnsmasq:674–679` (Xiaomi RN02 1.0.43) — 关闭 dynamicdhcp 时 IPv4 END 设为 static、IPv6 范围设为 ::,static；开启时建立动态 IPv6 范围。

#### dhcp/dhcp/netmask

覆盖 dnsmasq 地址池计算使用的 IPv4 子网掩码；未设置时取接口子网的掩码。

- Source fallback: 关联接口子网掩码
- Format / range: 普通偏移池保留 ipcalc.sh 接受的掩码格式；厂商完整 startip/endip 路径要求前缀长度 8–32。
- Condition: 与 start、limit 及厂商 startip/endip 路径相关。
- Apply impact: 与接口实际子网不一致会造成错误地址池或客户端路由；不会直接修改 network 中的接口掩码。
- Evidence: `etc/init.d/dnsmasq:639–640` (Xiaomi RN02 1.0.43) — netmask 缺省取 subnet 的斜线后部分。
- Evidence: `etc/init.d/dnsmasq:664–671` (Xiaomi RN02 1.0.43) — 偏移地址池把 netmask 交给 ipcalc.sh；完整起止地址路径可调用 dhcp_calc_netmask。
- Evidence: `etc/init.d/dnsmasq:551–560` (Xiaomi RN02 1.0.43) — dhcp_calc_netmask 仅接受 8–32 的前缀位数，否则使用 24，再转换为点分掩码。

#### dhcp/dhcp/dhcpv4

选择是否生成本接口的 IPv4 地址池；只有 disabled 会阻止 dnsmasq 输出 DHCPv4 范围。

- Format / range: 面板模式：disabled / server；dnsmasq 判断仅区分 disabled 与其他值。
- Condition: dnsmasq 实际处理 DHCP 的服务归属由 odhcpd.maindhcp 等决定。
- Condition: ignore=1 时在读取 dhcpv4 前已返回。
- Apply impact: 关闭后客户端不会从该池获得 IPv4 地址；IPv6 模式是另设字段。
- Evidence: `etc/init.d/dnsmasq:621–622` (Xiaomi RN02 1.0.43) — 脚本读取 dhcpv4 与 dhcpv6。
- Evidence: `etc/init.d/dnsmasq:682–684` (Xiaomi RN02 1.0.43) — 只要 dhcpv4 不等于 disabled，就追加 IPv4 --dhcp-range。
- Evidence: `docs/field-help-dhcp-research.md:25` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 dhcpv4 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68064.

#### dhcp/dhcp/dhcpv6

配置 DHCPv6 的服务器、中继或混合模式；厂商 IPv6 流程会按网络模式重写此项。

- Format / range: disabled / server / relay / hybrid
- Flags: version-dependent
- Condition: odhcpd 需启动且厂商 IPv6 模式不是 off/passthrough；AP/中继运行模式可能不启动它。
- Condition: 与 ra、master、接口 IPv6 前缀配合。
- Apply impact: 会改变 IPv6 地址及选项获取方式；中继还需可用上游。dnsmasq 后备路径只单独处理 disabled，不能代替 odhcpd 的完整中继行为。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua:6460–6465` (Xiaomi RN02 1.0.43) — 厂商上游配置写入 dhcpv6=relay。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua:8400–8404` (Xiaomi RN02 1.0.43) — LAN IPv6 模式流程将计算后的模式写入 dhcpv6。
- Evidence: `etc/init.d/dnsmasq:705–706` (Xiaomi RN02 1.0.43) — dnsmasq 的 IPv6 RA 分支在 dhcpv6=disabled 时改为纯 SLAAC 模式。
- Evidence: `docs/field-help-dhcp-research.md:26` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 dhcpv6 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68072.
- Evidence: `etc/init.d/odhcpd:8–24` (Xiaomi RN02 1.0.43) — odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。

#### dhcp/dhcp/ra

配置 IPv6 路由器通告模式；厂商 LAN IPv6 流程可设置 server 或 hybrid，上游配置可设置 relay。

- Format / range: disabled / server / relay / hybrid
- Condition: 由 odhcpd 正常处理；dnsmasq 接管时的支持范围不同。
- Condition: 厂商 IPv6 设置可能重写此值。
- Apply impact: 改变客户端的默认 IPv6 路由与前缀获取；错误通告可导致 IPv6 断网，即使 DHCPv4 仍正常。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua:6452–6458` (Xiaomi RN02 1.0.43) — 厂商上游写入 ra=relay。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua:8380–8383` (Xiaomi RN02 1.0.43) — LAN IPv6 模式将所选 ra 写入 dhcp 配置。
- Evidence: `etc/init.d/dnsmasq:687–724` (Xiaomi RN02 1.0.43) — dnsmasq 仅在其 DHCPv6 路径且 ra=server 时生成 RA 参数和相关 IPv6 范围。
- Evidence: `docs/field-help-dhcp-research.md:24` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 ra 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68056.
- Evidence: `etc/init.d/odhcpd:8–24` (Xiaomi RN02 1.0.43) — odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。

#### dhcp/dhcp/ndp

配置 odhcpd 的 IPv6 邻居发现代理模式；厂商 IPv6 中继流程会写入 relay 或 hybrid。

- Compact help: 已确认原厂中继写入；server 模式未取得可读实现证明。
- Format / range: disabled / server / relay / hybrid（面板模式；server 行为未在文本消费者中证明）
- Condition: 由 odhcpd 处理，不是 dnsmasq 的 DHCPv4 功能。
- Condition: 中继应与 master 上游和 ra/dhcpv6 模式一致。
- Apply impact: 可改变跨接口 IPv6 邻居可达性；错误代理模式会造成地址可见但流量不可达。
- Trace result: 检索 dnsmasq/odhcpd init、lib shell、厂商脚本与反编译 Lua：确认 ndp 的厂商写入及 odhcpd 二进制名称表；未获得 odhcpd NDP 分支的可读实现，不能仅凭面板选项确认 server 模式。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua:6466–6472` (Xiaomi RN02 1.0.43) — 厂商上游写入 ndp=relay。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua:8384–8390` (Xiaomi RN02 1.0.43) — LAN IPv6 流程写入所选 ndp 模式。
- Evidence: `docs/field-help-dhcp-research.md:27` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 ndp 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68080.
- Evidence: `etc/init.d/odhcpd:8–24` (Xiaomi RN02 1.0.43) — odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。

#### dhcp/dhcp/master

将此接口标记为 IPv6 中继上游；厂商生成上游 DHCP 段时同时写入 master=1 与 ignore=1。

- Compact help: 原厂 IPv6 中继上游标记，不是 DHCPv4 主服务器开关。
- Flags: generated
- Condition: 与 ra、dhcpv6、ndp 的中继配置及对应上游逻辑网络配合。
- Apply impact: 选错上游会让 IPv6 中继找不到正确服务；不是把此接口设为 DHCPv4 主服务器。
- Trace result: 确认厂商 Lua 的 master 上游标记写入及 odhcpd 二进制布尔类型表；dnsmasq 文本消费者不读取 master，未取得 odhcpd 内部中继选择实现。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua:6440–6451` (Xiaomi RN02 1.0.43) — 厂商上游配置连续写入 master=1、ignore=1。
- Evidence: `docs/field-help-dhcp-research.md:22` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 master 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68040.
- Evidence: `etc/init.d/odhcpd:8–24` (Xiaomi RN02 1.0.43) — odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。

#### dhcp/dhcp/ra_management

旧式 RA/DHCPv6 联动模式：0 生成无状态 DHCPv6，2 生成有状态分配，其余常规值生成 SLAAC 与 DHCPv6 混合。

- Compact help: 旧式 IPv6 联动项；dnsmasq 后备路径使用，原厂主流程写 ra_flags。
- Format / range: 面板 0 / 1 / 2；内部另用 3 表示仅 SLAAC。
- Flags: legacy
- Condition: dnsmasq 接管时需 ra=server 且启用 DHCPv6 能力；odhcpd 也有同名表项，内部兼容细节未验证。
- Apply impact: 仅应在旧配置兼容或 dnsmasq 接管 IPv6 时理解此项；不要把它与厂商主要写入的 ra_flags 当作等价字段。
- Evidence: `etc/init.d/dnsmasq:687–709` (Xiaomi RN02 1.0.43) — ra_management 在 dnsmasq DHCPv6 且 ra=server 的分支内使用；dhcpv6=disabled 时强制为 3。
- Evidence: `etc/init.d/dnsmasq:709–725` (Xiaomi RN02 1.0.43) — 0 对应 ra-stateless，2 对应地址 DHCP，3 对应 ra-only，其余对应 slaac。
- Evidence: `docs/field-help-dhcp-research.md:40` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 ra_management 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68184.

#### dhcp/dhcp/ra_default

选择默认路由通告策略。此项不能绕过原厂 WAN6 连通状态检查；检查未发现可用上联时，有效 Router Lifetime 会归零。

- Compact help: 默认路由通告仍受原厂 WAN6 检查限制；2 不保证始终提供有效默认路由。
- Format / range: 0 / 1 / 2；参与不同策略条件，最终有效期仍可被厂商 WAN6 检查覆盖。
- Flags: version-dependent
- Condition: 需提供 RA 并有适合的 IPv6 路由；AP/off/passthrough 模式可能不启动 odhcpd。
- Apply impact: 会改变客户端是否将本机作为 IPv6 默认路由；2 不保证始终提供有效默认路由。有效期归零不等于关闭 RA 中的前缀和其他选项。
- Trace result: 已取得 ra_default 的解析及策略分支、最终 Router Lifetime 的 WAN6 门控；未将面板的 2 标签推广为始终通告有效默认路由的保证。
- Evidence: `docs/field-help-dhcp-research.md:39` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 ra_default 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68176.
- Evidence: `etc/init.d/odhcpd:32–35` (Xiaomi RN02 1.0.43) — odhcpd init 直接启动 /usr/sbin/odhcpd，没有在此处解释 RA 策略。
- Evidence: `etc/init.d/odhcpd:8–24` (Xiaomi RN02 1.0.43) — odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。
- Evidence: `docs/field-help-queryport-review.md:119–140` (Xiaomi RN02 1.0.43) — ra_default 解析到接口策略字段；0、1、2 参与不同默认路由/前缀判定条件。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 18548.
- Evidence: `docs/field-help-queryport-review.md:143–161` (Xiaomi RN02 1.0.43) — 生成 RA 后执行 wan6_link_check.sh；检查返回 0 时强制把 Router Lifetime 写为 0。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 25920.
- Evidence: `usr/sbin/wan6_link_check.sh:6–23` (Xiaomi RN02 1.0.43) — WAN6 检查只在非 dedicated、接口已 up 且有 IPv6 网关时返回 1，否则返回 0。

#### dhcp/dhcp/ra_flags

设置 RA 标志列表：managed-config、other-config、home-agent 或 none。厂商 LAN IPv6 模式会重建列表。

- Format / range: 独立列表项：managed-config / other-config / home-agent / none
- Flags: generated, version-dependent
- Condition: 主要用于 odhcpd 的 RA 模式；dnsmasq 后备代码读取的是 ra_management。
- Apply impact: 可引导客户端使用 DHCPv6 获取地址或其他配置；手动值可能在更改厂商 IPv6 模式后被覆盖。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua:8303–8339` (Xiaomi RN02 1.0.43) — 厂商 IPv6 模式按分支选择 managed-config、other-config 或 none。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua:8412–8429` (Xiaomi RN02 1.0.43) — 厂商通过 set_list 写入 ra_flags、提交 dhcp 并重启 odhcpd。
- Evidence: `docs/field-help-dhcp-research.md:41` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 ra_flags 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68192.
- Evidence: `docs/field-help-dhcp-research.md:67` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 managed-config 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68360.
- Evidence: `docs/field-help-dhcp-research.md:68` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 other-config 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68368.
- Evidence: `docs/field-help-dhcp-research.md:69` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 home-agent 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68376.
- Evidence: `docs/field-help-dhcp-research.md:70` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 none 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68384.
- Evidence: `etc/init.d/odhcpd:8–24` (Xiaomi RN02 1.0.43) — odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。

#### dhcp/dhcp/ra_slaac

控制 RA 前缀的 Autonomous 位，让客户端可用 SLAAC 自动生成 IPv6 地址；原厂只对长度不大于 64 的前缀设置此位。

- Compact help: 有合适前缀时设置 RA Autonomous 位；前缀长度大于 64 时不会设置。
- Flags: version-dependent
- Condition: 需有 RA 通告与可用 IPv6 前缀；需另看 dhcpv6 模式。
- Apply impact: 关闭可能使依赖 SLAAC 的客户端无法自动生成 IPv6 地址；开启也不能代替合适的前缀或默认路由。
- Trace result: 已取得 ra_slaac 的布尔解析及前缀 Autonomous 位消费者；未追踪其他 DHCPv6 分配行为，不由此推断开关缺省值。
- Evidence: `docs/field-help-dhcp-research.md:42` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 ra_slaac 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68200.
- Evidence: `etc/init.d/dnsmasq:709–725` (Xiaomi RN02 1.0.43) — dnsmasq 后备路径通过 ra_management 选择 slaac/ra-only 等模式，并非读取 ra_slaac。
- Evidence: `etc/init.d/odhcpd:8–24` (Xiaomi RN02 1.0.43) — odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。
- Evidence: `docs/field-help-queryport-review.md:173` (Xiaomi RN02 1.0.43) — ra_slaac 解析为布尔字段；为真且前缀长度不大于 64 时设置 Autonomous 位 0x40。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 25776.

#### dhcp/dhcp/ra_mininterval

设置非请求式 RA 的最短间隔。原厂在值不大于 2 时改为 3，过大时结合有效最长间隔重新计算，不是原值直接发送。

- Compact help: RA 最短间隔会由原厂调整；不大于 2 时改为 3，过大时重新计算。
- Unit: 秒
- Format / range: 值不大于 2 时调整为 3；过大时重新计算，需结合有效 ra_maxinterval。
- Flags: version-dependent
- Condition: 由 odhcpd 的 RA 模式使用；与 ra_maxinterval 配合。
- Apply impact: 影响通告频率与网络开销；实际间隔可能不同于输入值，应同时检查最长间隔及默认路由有效期。
- Trace result: 已取得 ra_mininterval 的解析及定时调整分支；本次没有推断字段缺省值或完整随机通告分布。
- Evidence: `docs/field-help-dhcp-research.md:47` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 ra_mininterval 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68240.
- Evidence: `etc/init.d/odhcpd:26–30` (Xiaomi RN02 1.0.43) — odhcpd init 只对 LAN ra_maxinterval 缺失补值，没有在此处设置 ra_mininterval。
- Evidence: `etc/init.d/odhcpd:8–24` (Xiaomi RN02 1.0.43) — odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。
- Evidence: `docs/field-help-queryport-review.md:175` (Xiaomi RN02 1.0.43) — ra_mininterval 解析后参与定时生成；不大于 2 时改为 3，过大时重新计算。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 24368.

#### dhcp/dhcp/ra_maxinterval

设置 RA 通告的最长间隔。LAN 此项为空时原厂启动脚本写入 20；odhcpd 还会结合有效期调整，将有效最长间隔控制在 4–1800 秒。

- Compact help: LAN 缺失时启动脚本写 20；有效最长间隔还会结合有效期调整至 4–1800 秒。
- Source fallback: 20（仅 LAN 缺失时由启动脚本写入）
- Unit: 秒
- Format / range: 有效最长间隔为 4–1800 秒，先结合有效期收窄；不是输入值原样发送。
- Flags: generated
- Condition: 需实际启动 odhcpd；AP 模式、IPv6 off/passthrough 可提前返回。
- Condition: 应与 ra_mininterval 一起检查。
- Apply impact: 影响客户端发现路由器及刷新状态的速度；配置值会被调整，且 LAN 启动补值不代表所有接口都默认 20。
- Trace result: 已确认 LAN 的 ra_maxinterval 启动补值及 odhcpd 定时夹限分支；未从此推断其他接口缺省或完整随机通告分布。
- Evidence: `etc/init.d/odhcpd:26–30` (Xiaomi RN02 1.0.43) — odhcpd 启动脚本检查 dhcp.lan.ra_maxinterval，缺失时设为 20 并提交 dhcp。
- Evidence: `docs/field-help-dhcp-research.md:48` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 ra_maxinterval 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68248.
- Evidence: `etc/init.d/odhcpd:8–24` (Xiaomi RN02 1.0.43) — odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。
- Evidence: `docs/field-help-queryport-review.md:175` (Xiaomi RN02 1.0.43) — ra_maxinterval 参与定时生成；先结合有效期收窄，再将有效最长间隔控制在 4–1800 秒。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 24368.

#### dhcp/dhcp/ra_lifetime

设置 RA 中默认路由的有效时长，不是地址租期。原厂保留 0；正值至少为有效最长通告间隔，并限制到 9000，之后仍受 WAN6 检查。

- Compact help: 有效期会结合 RA 最长间隔夹限；原厂 WAN6 检查还可将它归零。
- Unit: 秒
- Format / range: 0 保持为 0；正值至少为有效 ra_maxinterval，且不超过 9000。WAN6 检查可最终归零。
- Flags: version-dependent
- Condition: 主要由 odhcpd 处理；dnsmasq 接管 IPv6 时这里的代码采用固定 7200。
- Condition: 结合 ra_default、RA 间隔和实际上游路由检查。
- Apply impact: 影响客户端保留本机默认 IPv6 路由的时间；可被有效间隔夹限或 WAN6 状态覆盖，不能据此保证始终有默认路由。
- Trace result: 已取得 ra_lifetime 的非负值夹限及最终 WAN6 门控；负值路径按有效最长间隔计算，没有据此推广为固定缺省值。
- Evidence: `docs/field-help-dhcp-research.md:49` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 ra_lifetime 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68256.
- Evidence: `etc/init.d/dnsmasq:693–701` (Xiaomi RN02 1.0.43) — dnsmasq 后备路径把 RA route lifetime 写死为 7200，并注释尚未转换灵活租期；不读取 ra_lifetime。
- Evidence: `etc/init.d/odhcpd:8–24` (Xiaomi RN02 1.0.43) — odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。
- Evidence: `docs/field-help-queryport-review.md:176` (Xiaomi RN02 1.0.43) — ra_lifetime 正值小于有效最长间隔时提升，较大值限制到 9000，0 保持为 0；负值另按最长间隔计算。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 24588.
- Evidence: `docs/field-help-queryport-review.md:143–161` (Xiaomi RN02 1.0.43) — 最终 Router Lifetime 在 WAN6 检查返回 0 时被归零。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 25920.
- Evidence: `usr/sbin/wan6_link_check.sh:6–23` (Xiaomi RN02 1.0.43) — WAN6 检查根据上游专用标记、接口 up 状态及 IPv6 网关返回连通标记。

#### dhcp/dhcp/ra_mtu

设置 RA 中告知客户端的链路 MTU，不直接修改网卡 MTU。原厂在值为 0 时读取接口 MTU，并将实际发送值提升到至少 1280。

- Compact help: 设置 RA 通告 MTU；0 取接口 MTU，发送值至少 1280，不直接改网卡。
- Unit: 字节
- Format / range: 发送值至少 1280；0 读取接口 MTU。面板范围 1280–65535，二进制此分支未证明 65535 上限。
- Flags: version-dependent
- Condition: 需有 RA；应与接口实际 MTU 及上游路径相容。
- Apply impact: 通告 MTU 与实际链路不符可能造成 IPv6 丢包或吞吐下降；二进制已证明下限处理，但不能从该分支推断 65535 上限。
- Trace result: 已取得 ra_mtu 的配置解析、0 值读取接口 MTU 及发送值下限处理；没有从该片段证明 65535 上限或字段缺省值。
- Evidence: `docs/field-help-dhcp-research.md:54` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 ra_mtu 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68296.
- Evidence: `etc/init.d/odhcpd:32–35` (Xiaomi RN02 1.0.43) — odhcpd init 直接启动服务二进制，不在脚本中配置 RA MTU。
- Evidence: `etc/init.d/odhcpd:8–24` (Xiaomi RN02 1.0.43) — odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。
- Evidence: `docs/field-help-queryport-review.md:174` (Xiaomi RN02 1.0.43) — ra_mtu 解析到接口字段；值为 0 时查询接口 MTU，最终发送值至少为 1280。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 24104.

#### dhcp/dhcp/dns

向 IPv6 客户端提供 DNS 地址列表；odhcpd 二进制中存在同名列表表项。

- Compact help: 接口 IPv6 DNS 列表；odhcpd 发送分支未取得可读实现。
- Flags: version-dependent
- Condition: 需有可用的 RA 或 DHCPv6 服务；不是 dnsmasq.server 的上游 DNS 设置。
- Apply impact: 会改变客户端 IPv6 DNS 选择；不可达地址可让 IPv6 网络看似连通但无法解析名称。
- Trace result: 检索 dnsmasq/odhcpd init、lib shell、厂商脚本与反编译 Lua，未证明接口段 dns 的 odhcpd 发送分支；dnsmasq dhcp_add 虽引用 dns 变量却未读取此字段。ELF 列表项仅证明名称/类型。
- Evidence: `docs/field-help-dhcp-research.md:29` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 dns 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68096.
- Evidence: `etc/init.d/dnsmasq:728–735` (Xiaomi RN02 1.0.43) — dnsmasq 后备 IPv6 分支有生成 option6:dns-server 的代码，但该函数内未见 config_get dns。
- Evidence: `etc/init.d/odhcpd:8–24` (Xiaomi RN02 1.0.43) — odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。

#### dhcp/dhcp/domain

按接口提供 IPv6 DNS 搜索域列表；与 dnsmasq 全局本地域名分开保存。

- Compact help: 接口 IPv6 搜索域列表；odhcpd 发送分支未取得可读实现。
- Flags: version-dependent
- Condition: 由 odhcpd 的 IPv6 配置处理；需配合可达 DNS 服务。
- Apply impact: 会影响客户端对短名称的补全；不匹配本地 DNS 记录时可能产生无效查询。
- Trace result: 检索接口 dhcp_add、odhcpd init、lib shell、厂商文本脚本和反编译 Lua，未找到 dhcp 接口段 domain 的文本读取；odhcpd 有 domain 列表表项，但搜索域发送实现未取得。
- Evidence: `docs/field-help-dhcp-research.md:31` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 domain 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68112.
- Evidence: `etc/init.d/dnsmasq:1086` (Xiaomi RN02 1.0.43) — dnsmasq 的 --domain 读取发生在 dnsmasq 全局段，而不是接口 dhcp_add。
- Evidence: `etc/init.d/odhcpd:8–24` (Xiaomi RN02 1.0.43) — odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。

#### dhcp/dhcp/dhcp_option

为此接口添加带网络标签的 DHCP 选项，可用编号和值或 option6: 等原生语法。

- Condition: 由 dnsmasq 实际服务此地址池；ignore 时不会进入选项生成。
- Condition: 与 dnsmasq 全局 dhcp_option 及自动生成的 DNS/网关选项一起检查。
- Apply impact: 只针对该池匹配客户端修改 DNS、网关等配置；不正确的选项可导致已获地址但不能联网。
- Evidence: `etc/init.d/dnsmasq:577–578` (Xiaomi RN02 1.0.43) — networkid 未设置时使用 interface 名称作为网络标签。
- Evidence: `etc/init.d/dnsmasq:738–740` (Xiaomi RN02 1.0.43) — 地址池分别调用普通与强制选项生成器。
- Evidence: `etc/init.d/dnsmasq:769–791` (Xiaomi RN02 1.0.43) — dhcp_option_add 优先保留原生列表，兼容旧式字符串并按标签生成选项。

#### dhcp/dhcp/dhcp_option_force

即使客户端没有请求，也发送此接口的 DHCP 选项。厂商还会自动附加型号、版本及初始化状态选项。

- Flags: version-dependent
- Condition: 仅影响 dnsmasq 处理的 DHCP；保留原生逗号与列表语法。
- Apply impact: 比普通选项覆盖面更强；错误的 DNS 或网关可持续发给客户端，且原厂附加选项仍可能存在。
- Evidence: `etc/init.d/dnsmasq:525–533` (Xiaomi RN02 1.0.43) — dhcp_option_force_add 读取该字段并生成 --dhcp-option-force。
- Evidence: `etc/init.d/dnsmasq:535–548` (Xiaomi RN02 1.0.43) — 厂商在同一函数继续加入由型号、ROM 版本、颜色及初始化状态生成的强制选项。
- Evidence: `etc/init.d/dnsmasq:738–740` (Xiaomi RN02 1.0.43) — 地址池调用 dhcp_option_add 强制路径及 dhcp_option_force_add。
- Evidence: `docs/field-help-dhcp-research.md:109` (Xiaomi RN02 1.0.43) — 内置帮助说明即使客户端没有请求，也发送此 DHCP 选项。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 209463.

### host

#### dhcp/host/name

设置静态客户端主机名；没有 MAC/DUID 时，dnsmasq 还可用此名称作为客户端匹配条件。

- Condition: dns=1 且有 ip 才额外写本地 hosts 记录；全局 domain 可追加后缀。
- Apply impact: 名字影响租约识别和本地解析；没有名称、IPv4 或 hostid 的空记录会被跳过。
- Evidence: `etc/init.d/dnsmasq:366–370` (Xiaomi RN02 1.0.43) — 读取 name/ip/hostid；三者全空时忽略记录。
- Evidence: `etc/init.d/dnsmasq:393–398` (Xiaomi RN02 1.0.43) — 没有 MAC 或 DUID 时使用 name 作为客户端标识，然后清空输出名称字段。
- Evidence: `etc/init.d/dnsmasq:413–420` (Xiaomi RN02 1.0.43) — 有独立名称时将其追加到 --dhcp-host。

#### dhcp/host/mac

用一个或多个 MAC 地址匹配同一静态租约；脚本按空白拆分后用逗号拼接。

- Format / range: 原生 MAC 地址格式；多个地址保持列表或空白分隔形式。
- Condition: 用于 IPv4 静态匹配；缺少 MAC/DUID 时可退回名称匹配。
- Apply impact: 匹配客户端会得到对应固定地址；重复或错误 MAC 会让目标客户端无法匹配，或导致地址冲突。
- Evidence: `etc/init.d/dnsmasq:377–385` (Xiaomi RN02 1.0.43) — 读取 mac；支持多个 MAC 并将它们以逗号拼接。
- Evidence: `etc/init.d/dnsmasq:418–420` (Xiaomi RN02 1.0.43) — 拼接后的 MAC 标识进入 --dhcp-host。

#### dhcp/host/ip

给匹配客户端指定固定 IPv4 地址，或使用 dnsmasq 原生 ignore 标记忽略该客户端。

- Condition: 需有效的 MAC、DUID 或名称匹配条件。
- Condition: dns=1 且 name 非空时该字符串也会进入 hosts 文件，应避免组合使用 ignore 与生成 DNS 记录。
- Apply impact: 固定地址应与对应 LAN 子网一致且不能与其他分配冲突；ignore 会让匹配客户端无法从此服务获址。
- Evidence: `etc/init.d/dnsmasq:366–370` (Xiaomi RN02 1.0.43) — ip 参与静态租约是否有效的判断。
- Evidence: `etc/init.d/dnsmasq:416–420` (Xiaomi RN02 1.0.43) — ip 原样进入 --dhcp-host，IPv4 路径不转换该值。

#### dhcp/host/duid

设置 DHCPv6 客户端标识；dnsmasq 只在其 DHCPv6 服务路径中生成 id: 标识。

- Format / range: DHCPv6 原生十六进制 DUID；保留格式。
- Condition: 当前负责 DHCPv6 的服务必须支持相应静态租约；IPv4 dnsmasq 路径忽略 DUID。
- Apply impact: 只改 DUID 不会改变普通 DHCPv4 的 MAC 匹配；标识不对应目标客户端时固定 IPv6 分配失效。
- Evidence: `etc/init.d/dnsmasq:377–390` (Xiaomi RN02 1.0.43) — duid 被读取，但仅 DNSMASQ_DHCP_VER=6 且非空时拼接 id: 标识。

#### dhcp/host/hostid

设置 IPv6 固定地址的十六进制主机部分；dnsmasq 验证十六进制格式后把低 32 位放入 [::主机部分]。

- Format / range: 十六进制主机标识，可带 0x；dnsmasq 转换器仅保留低 32 位。
- Condition: dnsmasq 接管 DHCPv6 时才在此代码中输出；odhcpd 需核对其同名静态租约支持。
- Apply impact: 不会设置完整 IPv6 前缀；实际地址还依赖接口可用前缀及 DHCPv6 服务。
- Evidence: `etc/init.d/dnsmasq:368–370` (Xiaomi RN02 1.0.43) — hostid 参与静态记录是否有效的判断。
- Evidence: `etc/init.d/dnsmasq:400–402` (Xiaomi RN02 1.0.43) — hostid 非空时调用 hex_to_hostid 转换。
- Evidence: `etc/init.d/dnsmasq:416–420` (Xiaomi RN02 1.0.43) — 仅 IPv6 dnsmasq 路径在 --dhcp-host 地址中追加 [::hostid]。
- Evidence: `etc/init.d/dnsmasq:93–107` (Xiaomi RN02 1.0.43) — hex_to_hostid 去除可选 0x 前缀、拒绝非十六进制字符，再将低 32 位分成两个 16 位 IPv6 分组。

#### dhcp/host/leasetime

为这个静态客户端单独设置租约时长；空值不会输出专用租期。

- Unit: dnsmasq 时间字符串，例如 12h 或 infinite
- Condition: 无专用租期时应检查接口地址池 leasetime，不把示例 12h 当作此字段默认。
- Apply impact: 可让固定设备使用不同于地址池的租期；短租期增加续租，长租期延后配置更新。
- Evidence: `etc/init.d/dnsmasq:408–414` (Xiaomi RN02 1.0.43) — 读取 host.leasetime，无显式回退；只在非空时加入 nametime。
- Evidence: `etc/init.d/dnsmasq:418–420` (Xiaomi RN02 1.0.43) — nametime 追加到此条 --dhcp-host。

#### dhcp/host/dns

为含 IPv4 地址与名称的静态租约额外生成本地 hosts 记录。

- Source fallback: 0（不额外生成）
- Condition: 需要 ip 和 name 非空；全局 domain 会追加后缀。
- Condition: 由 dnsmasq 的额外 hosts 文件加载，不等于发送 IPv6 DNS 地址。
- Apply impact: 使静态客户端名称可在取得租约前供本地 DNS 使用；缺少 ip 或 name 时即使启用也不生成。
- Evidence: `etc/init.d/dnsmasq:372–375` (Xiaomi RN02 1.0.43) — dns 缺省为 0；只有 dns=1 且 ip、name 均非空时写入 HOSTFILE_TMP，并可追加 DOMAIN。

#### dhcp/host/broadcast

给此客户端附加 needs-broadcast 标签，请求使用广播 DHCP 应答。

- Compact help: 原厂只追加 needs-broadcast 标签；显式广播开关的生成行已注释。
- Source fallback: 0（不加广播标签）
- Flags: version-dependent
- Apply impact: 可帮助部分收不到单播应答的客户端；但本固件直接启用该标签的 dhcp-broadcast 行已被注释，不能保证单改此项会触发广播。
- Trace result: 确认静态租约广播标签的生成，但检索 dnsmasq init、厂商脚本与反编译 Lua，未找到仍执行的 --dhcp-broadcast=tag:needs-broadcast 生成；其他配置文件是否定义该标签未验证。
- Evidence: `etc/init.d/dnsmasq:408–413` (Xiaomi RN02 1.0.43) — broadcast 缺省为 0；非零时加入 set:needs-broadcast 标签。
- Evidence: `etc/init.d/dnsmasq:1207` (Xiaomi RN02 1.0.43) — 生成 --dhcp-broadcast=tag:needs-broadcast 的行被注释。
- Evidence: `docs/field-help-dhcp-research.md:114` (Xiaomi RN02 1.0.43) — dnsmasq 内置帮助说明可按标签强制广播应答；原厂脚本是否生成相应全局选项需独立核对。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 208935.

#### dhcp/host/tag

为匹配客户端附加一个或多个 dnsmasq set: 标签，以关联选项或策略。

- Condition: 应与已有标签匹配规则配套；保留每个标签边界。
- Apply impact: 标签本身不分配 IP；关联的选项规则会改变该客户端的 DHCP 参数。
- Evidence: `etc/init.d/dnsmasq:379–405` (Xiaomi RN02 1.0.43) — 读取 tag；按空白拆分为逗号分隔标签。
- Evidence: `etc/init.d/dnsmasq:413–420` (Xiaomi RN02 1.0.43) — 标签带 set: 前缀进入 --dhcp-host。

### domain

#### dhcp/domain/name

给本地 DNS 记录指定一个或多个主机名，脚本按空白拆分后写入生成的 hosts 文件。

- Condition: 需要同一记录 ip 非空；由 dnsmasq 的额外 hosts 文件加载。
- Apply impact: 这些名称会由本机直接解析；错误名称可能覆盖客户端期望的公开域名。
- Evidence: `etc/init.d/dnsmasq:800–810` (Xiaomi RN02 1.0.43) — name 必须非空；循环组合名称后，与 ip 一起写入 HOSTFILE_TMP。

#### dhcp/domain/ip

指定本地 hosts 记录答复的 IP 地址；该路径直接写入 hosts，不进行 CIDR 展开。

- Compact help: 填写单个 IPv4 或 IPv6 地址；原厂直接写 hosts，不展开 CIDR。
- Format / range: hosts 地址字段：单个 IPv4 或 IPv6 地址，不应填写 CIDR 网段。
- Condition: 同一记录 name 必须非空。
- Apply impact: 影响该名称的本地解析地址；填入网段或不合法地址可能让记录无法解析。
- Evidence: `etc/init.d/dnsmasq:803–810` (Xiaomi RN02 1.0.43) — ip 必须非空，并与记录名称直接写入 HOSTFILE_TMP。

### odhcpd

#### dhcp/odhcpd/maindhcp

通知 dnsmasq 是否把全部 DHCP 地址服务交给 odhcpd。原厂安装的是 ipv6only 变体，不可把此开关视为已验证的 DHCPv4 接管能力。

- Compact help: 会让 dnsmasq 退出 DHCP；原厂 ipv6only 变体的 IPv4 接管能力未证明。
- Source fallback: 0（dnsmasq 判断的回退）
- Flags: version-dependent
- Condition: 受 odhcpd init 是否启用及 dnsmasq DHCPv6 编译能力影响。
- Apply impact: 启用后 dnsmasq 可停止提供地址池；若 odhcpd 没有 DHCPv4 能力，IPv4 客户端会失去地址分配。
- Evidence: `etc/init.d/dnsmasq:1005–1019` (Xiaomi RN02 1.0.43) — odhcpd 存在时，maindhcp 缺省为 0；若其大于 0 且未优先走 dnsmasq IPv6 后备分支，则 DNSMASQ_DHCP_VER=0。
- Evidence: `usr/lib/opkg/info/odhcpd-ipv6only.control:1–16` (Xiaomi RN02 1.0.43) — 安装包为 odhcpd-ipv6only，说明列出 RA、DHCPv6、前缀委派及 RA/DHCPv6/NDP 中继服务。
- Evidence: `docs/field-help-dhcp-research.md:63` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 maindhcp 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68408.

#### dhcp/odhcpd/leasefile

按 odhcpd 字段语义指定 IPv6 租约/主机记录文件；不是 dnsmasq 的 DHCPv4 租约路径。

- Compact help: odhcpd 的文件路径；原厂二进制有此项，格式与默认路径未取得证明。
- Flags: version-dependent
- Condition: 路径及更新用途由 odhcpd 二进制处理；leasetrigger 可关联后续更新。
- Apply impact: 错误路径可能使 IPv6 租约记录或下游 DNS 更新失效；可读脚本没有证明文件格式及缺省路径。
- Trace result: 检索 odhcpd init/update、lib shell、厂商文本脚本及反编译 Lua，未发现 odhcpd.leasefile 的文本读取；ELF 字符串表项仅证明该配置名及类型，不证明采样配置即默认。
- Evidence: `docs/field-help-dhcp-research.md:64` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 leasefile 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68416.
- Evidence: `usr/sbin/odhcpd-update:1–6` (Xiaomi RN02 1.0.43) — odhcpd-update 的注释说明租约更新通知让 dnsmasq 重读 hosts，并向 dnsmasq 发送信号。

#### dhcp/odhcpd/leasetrigger

按 odhcpd 字段语义指定租约变更后的通知路径；原厂 odhcpd-update 脚本会通知 dnsmasq 重读 hosts。

- Compact help: IPv6 租约通知路径；已找到 update 脚本，实际 trigger 调用分支未取得。
- Flags: version-dependent
- Condition: 通知文件需存在并能运行；不能从 update 脚本本身推断它一定是缺省 trigger。
- Apply impact: 路径失效可能使租约变化不能及时反映到本地 DNS；更改已有执行路径前应核对该通知链。
- Trace result: 检索 odhcpd init/update、lib shell、厂商文本脚本与反编译 Lua，未找到 leasetrigger 文本消费者；odhcpd ELF 有字符串类型表项，但本次未取得调用时机、参数或默认路径。
- Evidence: `docs/field-help-dhcp-research.md:65` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 leasetrigger 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68424.
- Evidence: `usr/sbin/odhcpd-update:1–6` (Xiaomi RN02 1.0.43) — odhcpd-update 导入 procd 函数并向 dnsmasq 发送重读 hosts 的信号。

#### dhcp/odhcpd/loglevel

按 odhcpd 字段语义设置 syslog 级别阈值；面板编号 0 最紧急，7 最详细。

- Compact help: syslog 级别 0–7；原厂二进制含此项，默认值与过滤分支未验证。
- Format / range: 0–7（syslog/schema 级别；内部校验未取得）
- Flags: version-dependent
- Apply impact: 较详细日志会增加资源占用并包含客户端信息；实际过滤及默认级别尚未在可读固件实现中证明。
- Trace result: 检索 odhcpd init、lib shell、厂商文本脚本及反编译 Lua，未找到 odhcpd.loglevel 文本读取；只确认 odhcpd 的同名整数表项。
- Evidence: `docs/field-help-dhcp-research.md:66` (Xiaomi RN02 1.0.43) — 原始 odhcpd ELF 表含 loglevel 名称及原始类型/数值；此表本身不证明缺省值或运行行为。
  - Stock binary: `usr/sbin/odhcpd`; SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`; offset 68432.
- Evidence: `etc/init.d/odhcpd:32–35` (Xiaomi RN02 1.0.43) — odhcpd init 未传入日志级别参数，直接启动二进制。

### cname

#### dhcp/cname/cname

创建本地 DNS 别名，脚本将其与 target 拼成 --cname。

- Condition: target 必须是 DHCP 或本地 hosts 已知名称。
- Apply impact: 匹配名称会转到本机可解析的目标；此功能不是把任意外部域名作为 CNAME 目标。
- Evidence: `etc/init.d/dnsmasq:854–860` (Xiaomi RN02 1.0.43) — cname 与 target 都必须非空，输出 --cname=cname,target。
- Evidence: `etc/dnsmasq.conf:33–37` (Xiaomi RN02 1.0.43) — 原厂 dnsmasq.conf 注释说明 CNAME 仅适用于 DHCP 或 /etc/hosts 中的本地目标。
- Evidence: `docs/field-help-dhcp-research.md:110` (Xiaomi RN02 1.0.43) — dnsmasq 内置帮助说明是 LOCAL DNS 名称的别名。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 213714.

#### dhcp/cname/target

设置别名指向的本地主机名；原厂注释限定目标来自 DHCP 或 /etc/hosts。

- Condition: 同一条记录 cname 非空，目标已由本机 DHCP/hosts 知道。
- Apply impact: 不存在的本地目标不会成为正常可用的别名；仅填外部 DNS 域名不符合此固件的说明。
- Evidence: `etc/init.d/dnsmasq:857–860` (Xiaomi RN02 1.0.43) — target 非空时与 cname 一起输出 --cname。
- Evidence: `etc/dnsmasq.conf:33–37` (Xiaomi RN02 1.0.43) — dnsmasq.conf 说明本地别名仅用于 DHCP 或 hosts 中的目标。
- Evidence: `docs/field-help-dhcp-research.md:110` (Xiaomi RN02 1.0.43) — dnsmasq 内置帮助说明是 LOCAL DNS 名称的别名。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 213714.

#### dhcp/cname/ttl

既有字段表示 DNS 别名缓存有效期；本固件 cname 生成器仅输出别名和目标，不读取 ttl。

- Compact help: 原厂 cname 生成器没有读取 ttl；改值不能确认改变缓存时间。
- Unit: 秒（既有 schema 语义；此路径未消费）
- Flags: version-dependent
- Apply impact: 只改此项不能据当前静态代码确认缓存时间变化；不能把界面数值当作已生效的 DNS TTL。
- Trace result: 已检索 1.0.43 完整 dhcp_cname_add、dnsmasq 启动选项、lib shell、厂商脚本与反编译 Lua，未找到 cname 段 ttl 读取；父级保存的 1.0.64 公共脚本同函数也未读取 ttl。全局 local_ttl 是不同字段。
- Evidence: `etc/init.d/dnsmasq:850–861` (Xiaomi RN02 1.0.43) — 完整 dhcp_cname_add 只读取 cname/target 并输出两段 --cname，未读取或追加 ttl。
- Evidence: `etc/init.d/dnsmasq:1119–1123` (Xiaomi RN02 1.0.43) — dnsmasq 全局 local_ttl/max_ttl 等另有读取，不能证明 cname.ttl 的作用。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/dnsmasq:850–861` (Xiaomi RN02 1.0.64) — 已保存的 1.0.64 公共脚本仍仅输出 cname 与 target，没有读取或追加 ttl。
- Evidence: `docs/field-help-dhcp-research.md:111` (Xiaomi RN02 1.0.43) — dnsmasq 二进制的原生 CNAME 语法支持可选 ttl；这不等于厂商 UCI 生成器会读取 cname.ttl。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 213689.

### boot

#### dhcp/boot/filename

设置 DHCP 网络启动的文件名；缺少文件名时整个 boot 记录被忽略。

- Condition: 客户端需要网络启动支持；此项不自动启用或部署 TFTP 文件。
- Condition: serveraddress 非空时还必须填写 servername。
- Apply impact: 会改变匹配客户端请求的启动文件；错误名称可使 PXE/网络启动失败。
- Evidence: `etc/init.d/dnsmasq:510–518` (Xiaomi RN02 1.0.43) — filename 必須非空；随后进入 --dhcp-boot 的文件名位置。

#### dhcp/boot/serveraddress

指定网络启动服务器 IPv4 地址；原厂生成器同时要求 servername 非空。

- Compact help: 填写启动服务器单个地址；有地址时原厂还要求 servername。
- Format / range: 单个启动服务器地址，不是网段。
- Condition: filename 与 servername 必须非空；启动文件服务另行提供。
- Apply impact: 错误地址让客户端无法获取启动文件；这条路径原样传入地址，不支持把 CIDR 当成服务器地址。
- Evidence: `etc/init.d/dnsmasq:513–518` (Xiaomi RN02 1.0.43) — serveraddress 直接放入 --dhcp-boot；若有地址却没有名称则跳过整条记录。

#### dhcp/boot/servername

设置网络启动服务器名称，位于 --dhcp-boot 文件名之后。原厂脚本要求有 serveraddress 时也有名称。

- Condition: filename 非空；配合 serveraddress 检查。
- Apply impact: 名称与地址组合不完整时整个启动记录会被跳过；不是普通 DNS 上游主机名。
- Evidence: `etc/init.d/dnsmasq:513–518` (Xiaomi RN02 1.0.43) — 读取 servername/serveraddress；地址非空但名称为空时提前返回，否则按位置追加。

#### dhcp/boot/networkid

用 dnsmasq 网络/客户端标签限定启动配置；非空时生成 net:标签 前缀。

- Condition: 标签应与接口或客户端标签规则一致。
- Condition: filename 缺失时不生成任何启动规则。
- Apply impact: 让不同客户端获得不同启动文件；没有对应匹配标签时规则可能不命中。
- Evidence: `etc/init.d/dnsmasq:508–518` (Xiaomi RN02 1.0.43) — 读取 networkid，并在 --dhcp-boot 前追加 net:networkid 条件。
- Evidence: `etc/init.d/dnsmasq:520–522` (Xiaomi RN02 1.0.43) — 同一 networkid 也传给该 boot 记录的 dhcp_option 生成器。

### relay

#### dhcp/relay/interface

可选的 DHCP 中继逻辑接口；填写后需能转换为实际设备，否则此中继条目不会生成。

- Condition: local_addr 与 server_addr 必须非空；dnsmasq boot 阶段跳过 relay 生成。
- Apply impact: 会把中继限定到指定设备；错误逻辑网络名使整条中继失效。
- Evidence: `etc/init.d/dnsmasq:894–899` (Xiaomi RN02 1.0.43) — interface 为空时不加接口；非空时需 network_get_device 成功，再追加实际设备。

#### dhcp/relay/local_addr

设置 DHCP 中继接收客户端请求的本地地址；此地址是必填项。

- Compact help: 填写中继本地单个地址；原厂直接传入 --dhcp-relay，不展开 CIDR。
- Format / range: 单个本地地址，保留 dnsmasq 中继地址格式；不是 CIDR 网段。
- Condition: 与 server_addr 和可选 interface 配套。
- Apply impact: 本地地址不在正确网络时请求可能无法进入中继；脚本不对 CIDR 进行转换。
- Evidence: `etc/init.d/dnsmasq:888–899` (Xiaomi RN02 1.0.43) — local_addr 必须非空，直接作为 --dhcp-relay 第一个参数。
- Evidence: `docs/field-help-dhcp-research.md:112` (Xiaomi RN02 1.0.43) — dnsmasq 内置中继语法为 local-addr,server[,iface]，未给出 CIDR 网段形式。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 213618.

#### dhcp/relay/server_addr

设置接收中继请求的 DHCP 服务器地址；此地址是必填项。

- Compact help: 填写 DHCP 服务器单个地址；路由和防火墙可达性需另行满足。
- Format / range: 单个 DHCP 服务器地址，不是 CIDR 网段。
- Condition: 必须从中继设备可达；local_addr 必须非空。
- Apply impact: 服务器不可达或地址错误会使客户端收不到租约；不会建立到服务器的路由。
- Evidence: `etc/init.d/dnsmasq:891–899` (Xiaomi RN02 1.0.43) — server_addr 必须非空，直接作为 --dhcp-relay 第二个参数。
- Evidence: `docs/field-help-dhcp-research.md:112` (Xiaomi RN02 1.0.43) — dnsmasq 内置中继语法为 local-addr,server[,iface]。
  - Stock binary: `usr/sbin/dnsmasq`; SHA-256 `f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`; offset 213618.

### srvhost

#### dhcp/srvhost/srv

指定 DNS SRV 的完整服务名，例如 _sip._tcp.example.test。

- Condition: target 与 port 必须非空，否则整条记录跳过。
- Apply impact: 决定客户端查询哪项服务时得到这条记录；错误前缀或域名会使服务发现失败。
- Evidence: `etc/init.d/dnsmasq:816–830` (Xiaomi RN02 1.0.43) — srv 必須非空，并成为 --srv-host 第一段。
- Evidence: `etc/dnsmasq.conf:1–4` (Xiaomi RN02 1.0.43) — dnsmasq.conf 说明 srv-host 顺序为 name,target,port,priority,weight。

#### dhcp/srvhost/target

指定 SRV 服务目标主机名，不直接填写其 IP 地址映射。

- Condition: 需有 srv 与 port；目标应可通过本地或上游 DNS 解析。
- Apply impact: 客户端会进一步解析该主机名；目标无法解析时服务发现仍不能连接。
- Evidence: `etc/init.d/dnsmasq:819–830` (Xiaomi RN02 1.0.43) — target 必須非空，并位于 --srv-host 的第二段。

#### dhcp/srvhost/port

指定 SRV 目标服务监听端口，不改变 dnsmasq 自身 DNS 端口。

- Unit: 端口号
- Format / range: 1–65535（面板范围；生成器直接传值）
- Condition: 与 srv 和 target 一起构成有效 SRV 条目。
- Apply impact: 客户端将连接此目标端口；端口错误会使服务发现结果无法使用。
- Evidence: `etc/init.d/dnsmasq:822–830` (Xiaomi RN02 1.0.43) — port 必須非空，并位于 --srv-host 的第三段。

#### dhcp/srvhost/class

此处字段名 class 实际被用作 SRV 优先级；较小值代表更优先目标，不是 DNS 记录类别。

- Format / range: 0–65535（SRV 优先级）
- Condition: weight 只在 class 非空时输出。
- Apply impact: 有多条同服务记录时影响目标选择顺序；空值时脚本也不输出后面的 weight。
- Evidence: `etc/init.d/dnsmasq:825–830` (Xiaomi RN02 1.0.43) — class 位于 --srv-host 第四段；仅 class 非空才追加 weight。
- Evidence: `etc/dnsmasq.conf:4` (Xiaomi RN02 1.0.43) — 原厂说明第四段是 priority。

#### dhcp/srvhost/weight

设置同 SRV 优先级目标的相对权重；原厂生成器仅在 class 非空时输出。

- Format / range: 0–65535（SRV 权重）
- Condition: 需同时填写 class；仅影响同优先级目标的选择。
- Apply impact: 有多个同优先级目标时可分配客户端选择比例；单条记录或不同优先级时不是固定流量百分比。
- Evidence: `etc/init.d/dnsmasq:825–830` (Xiaomi RN02 1.0.43) — weight 位于 --srv-host 第五段，受 class 非空条件约束。
- Evidence: `etc/dnsmasq.conf:4` (Xiaomi RN02 1.0.43) — 原厂说明第五段是 weight。

### mxhost

#### dhcp/mxhost/domain

指定本地 MX 记录所属邮件域名。

- Condition: 同一条记录 relay 必須非空。
- Apply impact: 对此域的 MX 查询会得到本机配置的邮件服务器；错误域名会影响邮件投递发现。
- Evidence: `etc/init.d/dnsmasq:837–847` (Xiaomi RN02 1.0.43) — domain 必須非空，并成为 --mx-host 第一段。

#### dhcp/mxhost/relay

指定 MX 记录指向的邮件服务器主机名；不是 DHCP relay 或 IP 中继地址。

- Condition: domain 必須非空；邮件服务器目标应可解析。
- Apply impact: 邮件客户端或服务器需能解析并连接这个主机；错误目标会影响邮件投递。
- Evidence: `etc/init.d/dnsmasq:840–847` (Xiaomi RN02 1.0.43) — relay 必須非空，并成为 --mx-host 第二段。

#### dhcp/mxhost/pref

设置 MX 邮件服务器优先级；较小值优先。

- Source fallback: 0
- Format / range: 0–65535（MX 优先级）
- Condition: 需 domain 与 relay 非空。
- Apply impact: 同一邮件域配置多个目标时决定首选服务器顺序，不等于发送权重。
- Evidence: `etc/init.d/dnsmasq:843–847` (Xiaomi RN02 1.0.43) — pref 明确回退为 0，并成为 --mx-host 第三段。

## firewall

### defaults

#### firewall/defaults/input

未被更具体规则处理、发往路由器本机的数据包策略；ACCEPT 允许，REJECT 回应拒绝，DROP 静默丢弃。

- Source fallback: DROP（fw3 缺失或无效策略回退）
- Format / range: ACCEPT / REJECT / DROP
- Apply impact: 重载 fw3 后改变本机服务的最后处理动作，可能中断管理页面、DNS 或 DHCP 访问。
- Evidence: `docs/field-help-firewall-research.md:25` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 defaults 选项表读取 input，使用动作枚举解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 105692.
- Evidence: `docs/field-help-firewall-research.md:204` (Xiaomi RN02 1.0.43) — fw3 缺失或无效策略回退 DROP；LuCI defaults.input 也明确返回 DROP。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/defaults/output

路由器本机发出的流量在没有更具体允许规则时使用的策略，不是 LAN 客户端的转发策略。

- Source fallback: DROP（fw3 缺失或无效策略回退）
- Format / range: ACCEPT / REJECT / DROP
- Apply impact: 重载后可能阻断路由器自己的 DNS 查询、时间同步和更新连接。
- Evidence: `docs/field-help-firewall-research.md:27` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 defaults 选项表读取 output，使用动作枚举解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 105724.
- Evidence: `docs/field-help-firewall-research.md:204` (Xiaomi RN02 1.0.43) — defaults.output 缺失时 DROP；动作作用于 OUTPUT 策略。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/defaults/forward

经过路由器、但既不发往本机也不由本机生成的流量的全局后备策略。

- Source fallback: DROP（fw3 缺失或无效策略回退）
- Format / range: ACCEPT / REJECT / DROP
- Apply impact: 重载后影响客户端跨网络通信；具体区域策略、forwarding 与 rule 仍参与处理。
- Evidence: `docs/field-help-firewall-research.md:26` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 defaults 选项表读取 forward，使用动作枚举解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 105708.
- Evidence: `docs/field-help-firewall-research.md:204` (Xiaomi RN02 1.0.43) — fw3 策略校验回退 DROP，LuCI defaults.forward 缺失时也返回 DROP。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/defaults/synflood_protect

启用 fw3 的 TCP SYN 洪泛限速链。旧名 syn_flood 与本字段写入同一保护开关，不是两层独立防护。

- Condition: 实际限速由同段 synflood_rate、synflood_burst 决定；本目录未列出这两个参数。
- Apply impact: 重载后改变新建 TCP 连接的 SYN 包处理；正常连接突发也可能触及保护速率。
- Evidence: `docs/field-help-firewall-research.md:32` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 defaults 选项表读取 synflood_protect，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 105804.
- Evidence: `docs/field-help-firewall-research.md:205` (Xiaomi RN02 1.0.43) — syn_flood/synflood_protect 共用偏移；生成器使用 --syn 与 syn_flood 链。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/defaults/drop_invalid

对连接跟踪判定为 INVALID 的包执行丢弃，不是丢弃所有未建立的新连接。

- Condition: 需要 conntrack 状态匹配；与 ACCEPT 已建立连接规则并存。
- Apply impact: 重载后过滤异常状态的流量；非对称路由或失配的连接状态可能被丢弃。
- Evidence: `docs/field-help-firewall-research.md:28` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 defaults 选项表读取 drop_invalid，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 105740.
- Evidence: `docs/field-help-firewall-research.md:206` (Xiaomi RN02 1.0.43) — 布尔 drop_invalid 对应 conntrack INVALID 生成分支。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/defaults/flow_offloading

让已建立的可卸载连接进入 fw3 FLOWOFFLOAD 软件加速分支；解析器支持该字段不等于所有协议都能卸载。

- Flags: hardware-dependent
- Condition: 需要固件内核 FLOWOFFLOAD 模块；适用分支匹配 RELATED,ESTABLISHED。
- Apply impact: 重载后可能降低转发 CPU 开销；被加速流量的逐包统计、限速或后续过滤可与普通路径不同。
- Evidence: `docs/field-help-firewall-research.md:59` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 defaults 选项表读取 flow_offloading，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 106236.
- Evidence: `docs/field-help-firewall-research.md:207` (Xiaomi RN02 1.0.43) — 原厂 fw3 生成 FLOWOFFLOAD，并检查卸载模块；此处仅证明静态支持。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/defaults/flow_offloading_hw

在流量卸载规则上追加硬件卸载请求 --hw；不是小米所有硬件加速功能的总开关。

- Flags: hardware-dependent
- Condition: 依赖 flow_offloading 的卸载路径及内核、硬件驱动支持。
- Apply impact: 重载后可请求硬件转发；没有驱动或模块支持时，不能仅凭此值保证加速生效。
- Evidence: `docs/field-help-firewall-research.md:60` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 defaults 选项表读取 flow_offloading_hw，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 106252.
- Evidence: `docs/field-help-firewall-research.md:207` (Xiaomi RN02 1.0.43) — fw3 硬件分支追加 --hw；未证明 RN02 当前硬件可执行此分支。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/defaults/disable_ipv6

禁止 fw3 安装 IPv6 规则；vendor IPv6 附加防火墙处理也会检查此值。它不关闭 IPv6 地址或网络服务。

- Condition: vendor ipv6_doit/ipv6_doit_v2 仅在值为 1 时提前返回。
- Apply impact: 重载后 IPv6 防护规则可能缺失，而 IPv6 连接仍能存在；不要当作关闭 IPv6 网络。
- Evidence: `docs/field-help-firewall-research.md:58` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 defaults 选项表读取 disable_ipv6，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 106220.
- Evidence: `usr/sbin/sysapi.firewall:499–500` (Xiaomi RN02 1.0.43) — 读取 firewall.@defaults[0].disable_ipv6；为 1 则停止该 IPv6 附加处理。
- Evidence: `usr/sbin/sysapi.firewall:528–529` (Xiaomi RN02 1.0.43) — 新版 IPv6 附加处理同样读取 disable_ipv6 并在 1 时返回。

### zone

#### firewall/zone/name

区域的引用名称，用于生成 zone_<名称> 链并供规则、转发和地址转换引用；它不是 UCI 段 ID。

- Format / range: 非空，fw3 最长 14 字节
- Apply impact: 改名后旧 src/dest 引用需同步更新，否则重载时引用的区域可能找不到。
- Evidence: `docs/field-help-firewall-research.md:176` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 zone 选项表读取 name，使用字符串解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108740.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/zone-details.lua:157–177` (Xiaomi RN02 1.0.43) — 区域 name 是必填 uciname；改名调用 rename_zone。
- Evidence: `docs/field-help-firewall-research.md:249` (Xiaomi RN02 1.0.43) — 原厂 fw3 对空区域名及超过 14 字节的名称跳过该区域。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/zone/network

把 /etc/config/network 的逻辑接口归入此区域，可为原生列表；不是直接填写物理网卡名。

- Condition: 逻辑接口须能解析为运行设备；与 device/subnet 共同限定区域。
- Apply impact: 重载与接口地址变更会重新解析区域设备；改变归属会同时改变过滤与 NAT 的适用范围。
- Evidence: `docs/field-help-firewall-research.md:178` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 zone 选项表读取 network，使用区域/设备引用解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108772.
- Evidence: `etc/hotplug.d/iface/20-firewall:9–12` (Xiaomi RN02 1.0.43) — 接口 ifup/ifupdate 用 fw3 network 判断相关性，再 reload firewall。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/zone-details.lua:304–310` (Xiaomi RN02 1.0.43) — network 被称为 Covered networks，并逐项 add_network。

#### firewall/zone/device

直接按物理/运行设备名称匹配区域；区别于 network 的逻辑接口解析。fw3 将其读为列表。

- Condition: 使用实际设备名称；可与 network、subnet 一起指定。
- Apply impact: 重载后改变按入/出设备选择的区域；设备名称变更会使相关流量不再进入原区域。
- Evidence: `docs/field-help-firewall-research.md:179` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 zone 选项表读取 device，使用区域/设备引用解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108788.
- Evidence: `docs/field-help-firewall-research.md:208` (Xiaomi RN02 1.0.43) — zone.device 是引用列表，规则生成使用 -i/-o 设备模板。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/zone/subnet

按地址或网络前缀补充区域匹配，fw3 逐项读取地址列表；不创建接口地址或静态路由。

- Condition: 地址族须与区域 family 一致；保留原生列表和否定语法。
- Apply impact: 重载后可缩小或扩大某区域策略覆盖的源/目标地址范围。
- Evidence: `docs/field-help-firewall-research.md:180` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 zone 选项表读取 subnet，使用地址/网络解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108804.
- Evidence: `docs/field-help-firewall-research.md:208` (Xiaomi RN02 1.0.43) — zone.subnet 用地址解析器列表，生成地址/掩码匹配。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/zone/input

来自本区域、发往路由器本机的后备策略。

- Source fallback: LuCI 模型缺失时继承 defaults 同名策略，仍缺失则 DROP；不表示当前区域策略
- Format / range: ACCEPT / REJECT / DROP
- Apply impact: 重载后影响区域内客户端访问路由器服务，可能中断管理访问。
- Evidence: `docs/field-help-firewall-research.md:181` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 zone 选项表读取 input，使用动作枚举解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108820.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/firewall.lua:943–955` (Xiaomi RN02 1.0.43) — zone.input 的 LuCI getter 缺失时取 defaults.input，随后回退 DROP。

#### firewall/zone/output

路由器本机发往本区域的后备策略，不是此区域客户端访问外网的策略。

- Source fallback: LuCI 模型缺失时继承 defaults 同名策略，仍缺失则 DROP；不表示当前区域策略
- Format / range: ACCEPT / REJECT / DROP
- Apply impact: 重载后影响路由器主动访问区域内主机及本机服务的出站流量。
- Evidence: `docs/field-help-firewall-research.md:183` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 zone 选项表读取 output，使用动作枚举解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108852.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/firewall.lua:987–999` (Xiaomi RN02 1.0.43) — zone.output 的 LuCI getter 缺失时取 defaults.output，随后回退 DROP。

#### firewall/zone/forward

本区域转发流量的后备策略；区域间 forwarding 许可仍是独立配置。

- Source fallback: LuCI 模型缺失时继承 defaults 同名策略，仍缺失则 DROP；不表示当前区域策略
- Format / range: ACCEPT / REJECT / DROP
- Apply impact: 重载后改变区域内及未被具体跨区域规则许可的转发流量处理。
- Evidence: `docs/field-help-firewall-research.md:182` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 zone 选项表读取 forward，使用动作枚举解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108836.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/firewall.lua:965–977` (Xiaomi RN02 1.0.43) — zone.forward 的 LuCI getter 缺失时取 defaults.forward，随后回退 DROP。

#### firewall/zone/family

限定区域适用的 IPv4、IPv6 或两者；不是给逻辑接口分配地址族。

- Format / range: any / ipv4 / ipv6
- Condition: IPv6 规则还受 defaults.disable_ipv6 控制。
- Apply impact: 重载后其他地址族的流量不使用此区域规则，双栈接口的保护范围可能改变。
- Evidence: `docs/field-help-firewall-research.md:177` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 zone 选项表读取 family，使用地址族解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108756.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/zone-details.lua:373–398` (Xiaomi RN02 1.0.43) — 区域 family 的选项分别是双栈、ipv4 与 ipv6。

#### firewall/zone/masq

对离开此区域的 IPv4 包执行 MASQUERADE，以出接口地址作为来源地址。

- Condition: IPv4 区域；masq_src/masq_dest 进一步限制范围，无法解析会关闭伪装。
- Apply impact: 重载后改变客户端外连的可见来源地址；已有连接的 NAT 状态不一定随字段立即更新。
- Evidence: `docs/field-help-firewall-research.md:184` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 zone 选项表读取 masq，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108868.
- Evidence: `docs/field-help-firewall-research.md:209` (Xiaomi RN02 1.0.43) — 原厂 fw3 包含区域 MASQUERADE 分支；来源/目的地址限制无法解析会 disabling masq。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/zone/masq6

通用 schema 中表示 IPv6 地址伪装；1.0.43 的 fw3 区域选项表没有 masq6，不能确认保存后生效。

- Compact help: 通用 IPv6 伪装字段；1.0.43 未找到 zone.masq6 消费。
- Flags: version-dependent
- Apply impact: 仅凭本字段不能保证 IPv6 NAT；vendor 脚本另按 ipv6 配置生成 NAT6，不等于读取 masq6。
- Trace result: 已搜索 fw3 完整 zone 表与全部字符串、firewall init/hotplug/lib/vendor shell 及既有 Lua 反编译；未找到精确字段 masq6。
- Evidence: `docs/field-help-firewall-research.md:242` (Xiaomi RN02 1.0.43) — 限定静态搜索范围未发现 masq6；其他 IPv6 NAT 分支不能充当此字段的证据。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.
- Evidence: `usr/sbin/sysapi.firewall:562–567` (Xiaomi RN02 1.0.43) — vendor 按 IPv6 prefix 创建 NAT6 MASQUERADE；此段没有读取 zone.masq6。

#### firewall/zone/masq_src

把区域 IPv4 伪装限定到指定来源网络；列表项可保留否定语法，以排除部分来源。

- Format / range: IPv4 地址/网络、可解析引用；支持原生列表与否定
- Condition: 仅在 masq 开启时有意义；适用于 IPv4。
- Apply impact: 重载后只改变哪些来源会被 NAT，不改变路由或是否允许转发；无法解析的限制会使伪装关闭。
- Evidence: `docs/field-help-firewall-research.md:186` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 zone 选项表读取 masq_src，使用地址/网络解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108900.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/zone-details.lua:405–422` (Xiaomi RN02 1.0.43) — masq_src 明确是来源子网限制，datatype 支持列表/否定。
- Evidence: `docs/field-help-firewall-research.md:209` (Xiaomi RN02 1.0.43) — 未解析的 masq_src 会 disabling masq。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/zone/masq_dest

把区域 IPv4 伪装限定到指定目标网络；可用否定项排除不应转换的目的地。

- Format / range: IPv4 地址/网络、可解析引用；支持原生列表与否定
- Condition: 仅在 masq 开启时有意义；适用于 IPv4。
- Apply impact: 重载后改变哪些目的地使用 NAT；不是只允许这些目的地联网的访问控制名单。
- Evidence: `docs/field-help-firewall-research.md:187` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 zone 选项表读取 masq_dest，使用地址/网络解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108916.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/zone-details.lua:428–445` (Xiaomi RN02 1.0.43) — masq_dest 明确是目的子网限制，datatype 支持列表/否定。
- Evidence: `docs/field-help-firewall-research.md:209` (Xiaomi RN02 1.0.43) — 未解析的 masq_dest 会 disabling masq。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/zone/mtu_fix

通过 TCPMSS --clamp-mss-to-pmtu 修正 TCP SYN 报文的 MSS，不直接改接口 MTU。

- Unit: 开关（不是 MTU 字节数）
- Condition: 需要 TCPMSS 支持；作用于 TCP 握手而非所有数据包。
- Apply impact: 重载后可缓解 PPPoE、隧道等路径的 TCP 大包问题；不修复 UDP 或所有路径 MTU 故障。
- Evidence: `docs/field-help-firewall-research.md:191` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 zone 选项表读取 mtu_fix，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108980.
- Evidence: `docs/field-help-firewall-research.md:210` (Xiaomi RN02 1.0.43) — 原厂生成 TCPMSS 和 --clamp-mss-to-pmtu；LuCI 标注 MSS clamping。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/zone/log

启用区域日志生成。LuCI 将它当作 0/1 开关，但 fw3 实际按数值读取，不是独立布尔选项。

- Compact help: 区域日志开关；原厂 fw3 按数值解析，配合 log_limit 限速。
- Condition: log_limit 限制日志消息，不是流量吞吐限速。
- Apply impact: 重载后被选中分支的包会写内核日志；噪声多时增加日志与 CPU 开销。
- Evidence: `docs/field-help-firewall-research.md:193` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 zone 选项表读取 log，使用32 位数值解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 109012.
- Evidence: `docs/field-help-firewall-research.md:211` (Xiaomi RN02 1.0.43) — zone.log 使用数值 parser；LuCI 开关写 1，生成分支使用 --log-prefix。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/zone-details.lua:460–467` (Xiaomi RN02 1.0.43) — 区域日志开关 enabled=1。

#### firewall/zone/log_limit

限制该区域日志匹配的平均速率，填写带时间单位的值；不是网速限制。

- Unit: 包/second、minute、hour 或 day
- Condition: 区域 log 开启时有意义。
- Apply impact: 重载后可减少日志洪泛；改变日志数量不自动改变 ACCEPT、REJECT 或 DROP 策略。
- Evidence: `docs/field-help-firewall-research.md:194` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 zone 选项表读取 log_limit，使用速率解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 109028.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/zone-details.lua:472–483` (Xiaomi RN02 1.0.43) — log_limit 是 Limit log messages，依赖 log=1；10/minute 只是 placeholder。
- Evidence: `docs/field-help-firewall-research.md:220` (Xiaomi RN02 1.0.43) — 速率解析和 --limit/--limit-burst 生成存在；此处不把占位提示当默认。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/zone/enabled

让 fw3 装载或跳过整个区域，而不是删除其网络接口。

- Condition: src/dest 引用此区域的其他段需要重新检查。
- Apply impact: 停用后依赖此区域的规则、转发和 NAT 可能失去有效区域；全局默认策略仍存在。
- Evidence: `docs/field-help-firewall-research.md:175` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 zone 选项表读取 enabled，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108724.
- Evidence: `docs/field-help-firewall-research.md:249` (Xiaomi RN02 1.0.43) — zone 加载器同段 0x150f4–0x1510c 在 enabled=0 时释放并跳过区域。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

### forwarding

#### firewall/forwarding/src

允许转发的入站区域名；与 dest 构成单向许可，不是逻辑接口名。

- Condition: src 与 dest 都须是有效防火墙区域；enabled 不能为 0。
- Apply impact: 重载后放行来自此区域、发往 dest 的流量；反方向不会因为本段自动放行。
- Evidence: `docs/field-help-firewall-research.md:66` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 forwarding 选项表读取 src，使用区域/设备引用解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 106364.
- Evidence: `docs/field-help-firewall-research.md:212` (Xiaomi RN02 1.0.43) — 原厂生成 src->dest forwarding 及 zone_dest_ACCEPT 跳转。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/forwarding/dest

此单向转发许可的出站区域名；不是转发后的 IP 地址。

- Condition: 与 src 配对；区域过滤、路由与 masq 另行配置。
- Apply impact: 重载后改变被放行的出站区域；允许转发不等于自动开启该区域的 NAT。
- Evidence: `docs/field-help-firewall-research.md:67` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 forwarding 选项表读取 dest，使用区域/设备引用解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 106380.
- Evidence: `docs/field-help-firewall-research.md:212` (Xiaomi RN02 1.0.43) — forwarding 有独立 dest 引用，跳转对应区域 ACCEPT 链。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/forwarding/enabled

让 fw3 装载或跳过这一条区域间转发许可；不删除 src/dest 区域。

- Condition: 仅控制此 forwarding 段，不是全局 forward 策略。
- Apply impact: 停用后此许可不再放行跨区域转发，其他规则或区域默认 ACCEPT 仍可能允许。
- Evidence: `docs/field-help-firewall-research.md:63` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 forwarding 选项表读取 enabled，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 106316.

### rule

#### firewall/rule/name

此流量规则的识别名称，用于规则显示与注释；不参与地址、协议或端口匹配。

- Apply impact: 重载后只改变规则的识别信息，便于定位日志与规则顺序。
- Evidence: `docs/field-help-firewall-research.md:117` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 name，使用字符串解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107764.
- Evidence: `docs/field-help-firewall-research.md:213` (Xiaomi RN02 1.0.43) — 原厂 name 由字符串解析器读取，规则显示使用 Rule/NAT/Redirect 名称格式；不是包匹配条件。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/rule/src

匹配进入路由器的来源区域；无 dest 时可针对路由器本机的入站流量。

- Condition: src/dest 决定入站、出站或转发路径；两者缺失按本机 OUTPUT 规则处理。
- Apply impact: 重载后改变规则所在的入站/转发路径，区域填错会使规则匹配范围改变。
- Evidence: `docs/field-help-firewall-research.md:119` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 src，使用区域/设备引用解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107796.
- Evidence: `docs/field-help-firewall-research.md:214` (Xiaomi RN02 1.0.43) — 原厂按 src/dest 区域生成 input/output/forward 规则；两者缺失时按 OUTPUT 处理。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/rule/dest

匹配离开路由器的目标区域；与 src 同时指定时形成转发规则。

- Condition: src/dest 决定入站、出站或转发路径；两者缺失按本机 OUTPUT 规则处理。
- Apply impact: 重载后改变规则处理的出站区域；仅 dest 通常针对本机出站流量。
- Evidence: `docs/field-help-firewall-research.md:120` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 dest，使用区域/设备引用解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107812.
- Evidence: `docs/field-help-firewall-research.md:214` (Xiaomi RN02 1.0.43) — 原厂按 src/dest 区域生成 input/output/forward 规则；两者缺失时按 OUTPUT 处理。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/rule/src_ip

匹配数据包来源地址或网络前缀，可保留原生列表与否定语法。

- Condition: 地址族与 family 和区域一致；按原生地址、CIDR、列表/否定语法填写。
- Apply impact: 重载后缩小或扩大适用来源；只匹配地址，不自动绑定 MAC 或 DHCP 租约。
- Evidence: `docs/field-help-firewall-research.md:127` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 src_ip，使用地址/网络解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107924.

#### firewall/rule/dest_ip

匹配数据包目的地址或网络前缀；这里是过滤条件，不是重定向后的主机地址。

- Condition: 地址族与 family 和区域一致；按原生地址、CIDR、列表/否定语法填写。
- Apply impact: 重载后改变哪些目的地受本规则动作处理，不产生地址转换。
- Evidence: `docs/field-help-firewall-research.md:130` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 dest_ip，使用地址/网络解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107972.

#### firewall/rule/src_mac

匹配收到帧的来源 MAC，可保留原生列表；不能识别经过其他路由器后的远端设备 MAC。

- Condition: 需可见的二层来源 MAC；不是远端设备身份认证。
- Apply impact: 重载后按二层来源选择规则；跨三层转发时可见的 MAC 与客户端本身可能不同。
- Evidence: `docs/field-help-firewall-research.md:128` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 src_mac，使用MAC 解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107940.
- Evidence: `docs/field-help-firewall-research.md:217` (Xiaomi RN02 1.0.43) — 原厂 MAC 解析和 --mac-source 匹配存在；redirect 的 SNAT 禁止 src_mac。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/rule/src_port

匹配 TCP/UDP 等带端口协议的来源端口，可填写端口、范围及原生列表/否定项。

- Unit: 端口号
- Format / range: 协议端口 0–65535；可用原生范围/否定语法
- Condition: 需 proto 含带端口的协议，常见为 TCP/UDP。
- Apply impact: 重载后改变客户端源端口的匹配范围；客户端临时端口通常不是服务端监听端口。
- Evidence: `docs/field-help-firewall-research.md:129` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 src_port，使用端口/范围解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107956.
- Evidence: `docs/field-help-firewall-research.md:216` (Xiaomi RN02 1.0.43) — 原厂端口解析识别范围与否定，并以 uint16 保存；生成 --sport/--dport。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/rule/dest_port

匹配目的服务端口，范围与原生列表/否定语法按 fw3 端口解析器处理。

- Unit: 端口号
- Format / range: 协议端口 0–65535；可用原生范围/否定语法
- Condition: 需 proto 含带端口的协议，常见为 TCP/UDP。
- Apply impact: 重载后改变命中的服务；不配置服务本身的监听端口。
- Evidence: `docs/field-help-firewall-research.md:131` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 dest_port，使用端口/范围解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107988.
- Evidence: `docs/field-help-firewall-research.md:216` (Xiaomi RN02 1.0.43) — 原厂端口解析识别范围与否定，并以 uint16 保存；生成 --sport/--dport。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/rule/proto

匹配 IP 协议名称/编号；空值回退 TCP+UDP，不等于所有协议。

- Source fallback: tcp udp（fw3 缺协议回退）
- Apply impact: 重载后改变哪些协议受动作处理；ICMP 与 TCP/UDP 端口条件不能互换。
- Evidence: `docs/field-help-firewall-research.md:126` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 proto，使用协议解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107908.
- Evidence: `docs/field-help-firewall-research.md:215` (Xiaomi RN02 1.0.43) — 原厂 rule/redirect 缺协议回退 TCP+UDP；nat 缺协议回退 all。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/rule/family

限定此过滤规则生成 IPv4、IPv6 或两族规则；地址条件必须兼容所选族。

- Format / range: any / ipv4 / ipv6
- Condition: IPv6 生成仍受 defaults.disable_ipv6 与内核模块支持约束。
- Apply impact: 重载后可只改变一个地址族的访问策略，另一族可能仍被允许。
- Evidence: `docs/field-help-firewall-research.md:118` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 family，使用地址族解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107780.

#### firewall/rule/enabled

让 fw3 装载或跳过这一条流量规则，字段内容仍保留。

- Apply impact: 停用后不再执行此规则动作；后续规则和区域/全局默认策略继续决定结果。
- Evidence: `docs/field-help-firewall-research.md:116` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 enabled，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107748.

#### firewall/rule/ipset

引用已存在的 IP 集合及原生匹配方向，对集合成员流量应用本规则动作。

- Condition: 须存在有效 ipset，集合地址族与规则一致。
- Apply impact: 重载后按集合筛选；集合未知或 ipset 支持关闭时，fw3 会跳过该规则。
- Evidence: `docs/field-help-firewall-research.md:123` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 ipset，使用IP 集合引用解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107860.
- Evidence: `docs/field-help-firewall-research.md:218` (Xiaomi RN02 1.0.43) — 原厂用 --match-set 匹配集合；ipset 支持关闭或集合未知会跳过相关规则。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/rule/mark

按当前包标记及可选掩码匹配；不是给包写入新标记。

- Format / range: 32 位标记[/掩码]；省略掩码为 0xffffffff
- Apply impact: 重载后改变按已有标记选择的流量；可能与策略路由、QoS 标记相互作用。
- Evidence: `docs/field-help-firewall-research.md:143` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 mark，使用标记/掩码解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108180.
- Evidence: `docs/field-help-firewall-research.md:219` (Xiaomi RN02 1.0.43) — 原厂解析标记/掩码并生成 --mark；省略掩码用 0xffffffff。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/rule/limit

限制此规则匹配的平均包速率，须包含时间单位；不是带宽上限。

- Unit: 包/second、minute、hour 或 day
- Condition: 与 limit_burst 配合；超过匹配速率不表示自动丢弃。
- Apply impact: 重载后只有符合限速匹配的包执行 target，超限包继续经过后续规则。
- Evidence: `docs/field-help-firewall-research.md:134` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 limit，使用速率解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108036.
- Evidence: `docs/field-help-firewall-research.md:220` (Xiaomi RN02 1.0.43) — 原厂速率单位表含 second/minute/hour/day，生成 --limit/--limit-burst。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/rule/limit_burst

设置 limit 匹配允许的初始/突发包数量，不是每秒的持续速率。

- Unit: 包
- Condition: 仅配合本段 limit；不是秒数或字节数。
- Apply impact: 重载后改变短时突发命中的包量；无 limit 时不能独立作为吞吐限制。
- Evidence: `docs/field-help-firewall-research.md:135` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 limit_burst，使用32 位数值解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108052.
- Evidence: `docs/field-help-firewall-research.md:220` (Xiaomi RN02 1.0.43) — 原厂速率单位表含 second/minute/hour/day，生成 --limit/--limit-burst。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/rule/target

决定命中规则后的动作。MARK 还需要 set_mark/set_xmark；HELPER 和 DSCP 分别还需要 set_helper、set_dscp，不能只选择动作。

- Source fallback: REJECT（fw3 缺失或无效动作回退）
- Format / range: 目录列出 ACCEPT / REJECT / DROP / MARK / NOTRACK / HELPER / DSCP
- Condition: MARK 配合本段 set_mark/set_xmark；HELPER/DSCP 所需写入字段未列在当前目录。
- Apply impact: 重载后会放行、拒绝、丢弃或改变流量处理；动作缺参数时 fw3 可能跳过规则而非执行。
- Evidence: `docs/field-help-firewall-research.md:148` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 target，使用动作枚举解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108260.
- Evidence: `docs/field-help-firewall-research.md:221` (Xiaomi RN02 1.0.43) — fw3 缺失/无效 target 回退 REJECT，MARK 要求写标记参数。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.
- Evidence: `docs/field-help-firewall-research.md:223` (Xiaomi RN02 1.0.43) — helper 与 HELPER 所需 set_helper 是独立字段。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/rule/icmp_type

限定 ICMP/ICMPv6 类型或类型/代码，不是 TCP/UDP 端口。原厂类型表包含 echo-request 等命名。

- Format / range: ICMP 类型 0–255；可使用原厂命名或列表
- Condition: proto 必须为相应 ICMP 协议，family 与类型编号/名称一致。
- Apply impact: 重载后只对选定 ICMP 类型执行动作；过窄配置可能影响 IPv6 邻居发现或路径 MTU 通知。
- Evidence: `docs/field-help-firewall-research.md:132` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 icmp_type，使用ICMP 类型解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108004.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/rule-details.lua:630–645` (Xiaomi RN02 1.0.43) — ICMP 类型下拉包含 protocol-unreachable 等类型。
- Evidence: `usr/sbin/ipv6.sh:406–415` (Xiaomi RN02 1.0.43) — vendor 创建 ICMP 规则，并添加 ICMPv6 类型名单、速率和 IPv6 family。

#### firewall/rule/set_mark

target=MARK 时写入数据包标记，可带位掩码；与只做匹配的 mark 不同。

- Format / range: 32 位值[/掩码]；省略掩码为 0xffffffff
- Condition: target=MARK；不能用否定值；省略掩码会覆盖完整 32 位标记。
- Apply impact: 重载后新标记可影响后续 QoS 或策略路由；掩码不当会覆盖其他功能占用的标记位。
- Evidence: `docs/field-help-firewall-research.md:144` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 set_mark，使用标记/掩码解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108196.
- Evidence: `docs/field-help-firewall-research.md:222` (Xiaomi RN02 1.0.43) — 原厂生成 --set-mark；校验不允许否定的 set_mark。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.
- Evidence: `docs/field-help-firewall-research.md:219` (Xiaomi RN02 1.0.43) — mark/mask 解析器省略掩码时写 0xffffffff。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/rule/set_xmark

target=MARK 时生成 --set-xmark：先按掩码清位，再异或指定值；不是简单地匹配已有标记。

- Format / range: 32 位值[/掩码]；省略掩码为 0xffffffff
- Condition: target=MARK；不能用否定值；与 set_mark 是不同写入方式。
- Apply impact: 重载后可只修改选定位；会影响使用这些位的 QoS、加速与策略路由规则。
- Evidence: `docs/field-help-firewall-research.md:145` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 set_xmark，使用标记/掩码解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108212.
- Evidence: `docs/field-help-firewall-research.md:222` (Xiaomi RN02 1.0.43) — 原厂 set_xmark 使用 mark/mask 解析器并生成 --set-xmark。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/rule/helper

匹配已有连接跟踪助手名称。它与 HELPER 动作所需的 set_helper 是不同字段；填 helper 不会替代 set_helper。

- Compact help: 匹配连接助手；HELPER 动作还需要独立 set_helper。
- Condition: 助手应在原厂 fw3_helper 定义中存在，并支持所选协议/地址族；相关模块须加载。
- Apply impact: 重载后按助手筛选已有连接；助手名称、协议或内核模块不匹配时规则可能跳过。
- Evidence: `docs/field-help-firewall-research.md:124` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 helper，使用连接助手引用解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107876.
- Evidence: `docs/field-help-firewall-research.md:223` (Xiaomi RN02 1.0.43) — rule.helper 与 set_helper 是独立表项；生成 --helper 做匹配，HELPER 无 set_helper 会报警。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/rule/start_time

规定每日开始匹配的时间，接受 HH:MM 或 HH:MM:SS；仅对到达此规则的包做时间条件匹配。

- Unit: 时:分[:秒]
- Format / range: 小时 0–23；分钟、秒 0–59
- Condition: utc_time 决定 UTC 或内核本地时区；weekdays 可再限制日期。
- Apply impact: 重载后开始时刻前的包不命中此规则；需要其他规则承担该时段的允许/拒绝策略。
- Evidence: `docs/field-help-firewall-research.md:139` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 start_time，使用时间解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108116.
- Evidence: `docs/field-help-firewall-research.md:224` (Xiaomi RN02 1.0.43) — 原厂时间解析器检查小时/分钟/秒范围，并生成 --timestart。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/rule/stop_time

规定每日结束匹配的时间，接受 HH:MM 或 HH:MM:SS；不是连接空闲超时。

- Unit: 时:分[:秒]
- Format / range: 小时 0–23；分钟、秒 0–59
- Condition: 配合 start_time；utc_time 决定所用时区。
- Apply impact: 重载后结束时刻外的包不命中此规则；跨午夜及已建立连接的处理需要结合整个规则集。
- Evidence: `docs/field-help-firewall-research.md:140` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 stop_time，使用时间解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108132.
- Evidence: `docs/field-help-firewall-research.md:224` (Xiaomi RN02 1.0.43) — 同一时间解析器读取 stop_time，并生成 --timestop。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/rule/weekdays

按星期限制此规则匹配的日期，原厂枚举 monday 到 sunday，可保留原生列表表达。

- Unit: 星期
- Format / range: monday…sunday（原生列表/缩写兼容性依 fw3 解析）
- Condition: 与 start_time/stop_time、utc_time 合并作为条件。
- Apply impact: 重载后其他星期不命中本规则；不是自动启停整个防火墙服务。
- Evidence: `docs/field-help-firewall-research.md:141` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 weekdays，使用星期解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108148.
- Evidence: `docs/field-help-firewall-research.md:225` (Xiaomi RN02 1.0.43) — 原厂星期枚举表与 --weekdays 生成引用存在。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/rule/utc_time

按 UTC 解释规则的时间条件；关闭时 fw3 添加 --kerneltz，使用内核时区，不是浏览器时区。

- Condition: 需要时间或日期匹配字段；系统时间与内核时区正确才有预期结果。
- Apply impact: 重载后可能使规则的每日生效窗口相对本地时钟移动；不修改系统时钟。
- Evidence: `docs/field-help-firewall-research.md:136` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 rule 选项表读取 utc_time，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108068.
- Evidence: `docs/field-help-firewall-research.md:224` (Xiaomi RN02 1.0.43) — utc_time 为假时生成 --kerneltz，时间条件另外生成 --timestart/--timestop。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

### redirect

#### firewall/redirect/name

此地址转换规则的识别名称，不决定外部端口或内部主机。

- Apply impact: 重载后改变 NAT 规则的显示/注释，地址端口匹配仍由其他字段决定。
- Evidence: `docs/field-help-firewall-research.md:88` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 name，使用字符串解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107284.
- Evidence: `docs/field-help-firewall-research.md:213` (Xiaomi RN02 1.0.43) — 原厂 name 由字符串解析器读取，规则显示使用 Rule/NAT/Redirect 名称格式；不是包匹配条件。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/redirect/src

DNAT 时表示流量进入的来源区域；不是要转换到的内部区域。

- Condition: 引用防火墙 zone.name，不是 network 逻辑接口。
- Apply impact: 重载后改变接受外部流量的入口；来源区域不匹配时不会执行此转发。
- Evidence: `docs/field-help-firewall-research.md:90` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 src，使用区域/设备引用解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107316.
- Evidence: `docs/field-help-firewall-research.md:226` (Xiaomi RN02 1.0.43) — 原厂 redirect.target 缺失回退 DNAT；DNAT/SNAT 的匹配和改写地址字段含义不同。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/redirect/dest

DNAT 时表示转换后的内部目标区域；SNAT 时是出站目标区域。

- Condition: 引用防火墙 zone.name，不是 network 逻辑接口。
- Apply impact: 重载后改变 NAT 规则使用的区域和允许转发链；不能只填区域而忽略真实路由。
- Evidence: `docs/field-help-firewall-research.md:91` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 dest，使用区域/设备引用解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107332.
- Evidence: `docs/field-help-firewall-research.md:226` (Xiaomi RN02 1.0.43) — 原厂 redirect.target 缺失回退 DNAT；DNAT/SNAT 的匹配和改写地址字段含义不同。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/redirect/src_ip

匹配转换前的数据包来源地址/网络，不是转换后的来源地址。

- Condition: 地址族与 family 和区域一致；原厂此表项为单值，地址/网络前缀与匹配条件的否定按具体转换动作处理。
- Apply impact: 重载后限制哪些来源可使用此转发，不能替代转换后的 dest_ip。
- Evidence: `docs/field-help-firewall-research.md:95` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 src_ip，使用地址/网络解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107396.

#### firewall/redirect/dest_ip

DNAT 时填写转换后的内部地址；SNAT 时作为原始目的地址匹配。

- Compact help: DNAT 时为内部转换目标；SNAT 时为原始目的匹配。
- Condition: 地址族与 family 和区域一致；原厂此表项为单值，地址/网络前缀与匹配条件的否定按具体转换动作处理。
- Apply impact: 重载后会改变流量实际发送的内部主机（DNAT），或缩小 SNAT 匹配目的地。
- Evidence: `docs/field-help-firewall-research.md:100` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 dest_ip，使用地址/网络解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107476.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/forward-details.lua:362–373` (Xiaomi RN02 1.0.43) — DNAT 表单将该字段说明为内部目标地址/端口。
- Evidence: `docs/field-help-firewall-research.md:226` (Xiaomi RN02 1.0.43) — 原厂 redirect.target 缺失回退 DNAT；DNAT/SNAT 的匹配和改写地址字段含义不同。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/redirect/src_mac

限制 DNAT 的二层来源 MAC；原厂 fw3 明确不允许 SNAT 使用此条件。

- Condition: 需可见的二层来源 MAC；不是远端设备身份认证。
- Apply impact: 重载后改变可使用 DNAT 的来源；若用于 SNAT，规则校验可能跳过而不是转换。
- Evidence: `docs/field-help-firewall-research.md:96` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 src_mac，使用MAC 解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107412.
- Evidence: `docs/field-help-firewall-research.md:217` (Xiaomi RN02 1.0.43) — 原厂 MAC 解析和 --mac-source 匹配存在；redirect 的 SNAT 禁止 src_mac。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/redirect/src_port

匹配转换前的来源端口；不是路由器上暴露的外部服务端口。

- Compact help: 原厂此段按单值端口/范围解析，列表支持未证明。
- Unit: 端口号
- Format / range: 协议端口 0–65535；可用原生范围/否定语法
- Condition: 需 proto 含带端口的协议，常见为 TCP/UDP。
- Condition: 原厂此段端口表项为单值；可解析端口范围，未证明原生列表支持。
- Apply impact: 重载后限制客户端来源端口；外部目的端口由 src_dport 指定。
- Evidence: `docs/field-help-firewall-research.md:97` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 src_port，使用端口/范围解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107428.
- Evidence: `docs/field-help-firewall-research.md:216` (Xiaomi RN02 1.0.43) — 原厂端口解析识别范围与否定，并以 uint16 保存；生成 --sport/--dport。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/redirect/dest_port

DNAT 时是内部目标服务端口；SNAT 时是原始目的端口匹配。

- Compact help: DNAT 时为内部转换目标；SNAT 时为原始目的匹配。
- Unit: 端口号
- Format / range: 协议端口 0–65535；可用原生范围/否定语法
- Condition: 需 proto 含带端口的协议，常见为 TCP/UDP。
- Condition: 原厂此段端口表项为单值；可解析端口范围，未证明原生列表支持。
- Apply impact: 重载后改变 DNAT 到内部主机的端口，或限制 SNAT 的目的服务。
- Evidence: `docs/field-help-firewall-research.md:101` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 dest_port，使用端口/范围解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107492.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/forward-details.lua:398–413` (Xiaomi RN02 1.0.43) — DNAT 表单将该字段说明为内部目标地址/端口。
- Evidence: `docs/field-help-firewall-research.md:226` (Xiaomi RN02 1.0.43) — 原厂 redirect.target 缺失回退 DNAT；DNAT/SNAT 的匹配和改写地址字段含义不同。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/redirect/proto

选择参与 NAT 的 IP 协议；缺失时 fw3 回退 TCP+UDP。

- Source fallback: tcp udp（fw3 缺协议回退）
- Apply impact: 重载后分别改变 TCP/UDP 转发；仅设置外部端口不能匹配未包含的协议。
- Evidence: `docs/field-help-firewall-research.md:94` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 proto，使用协议解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107380.
- Evidence: `docs/field-help-firewall-research.md:215` (Xiaomi RN02 1.0.43) — 原厂 rule/redirect 缺协议回退 TCP+UDP；nat 缺协议回退 all。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/redirect/family

限制此转换规则的地址族，转换前后地址必须使用兼容地址族。

- Format / range: any / ipv4 / ipv6
- Condition: IPv6 生成仍受 defaults.disable_ipv6 与内核模块支持约束。
- Apply impact: 重载后决定生成哪一族规则；并不保证 IPv6 NAT 或回环在设备上可用。
- Evidence: `docs/field-help-firewall-research.md:89` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 family，使用地址族解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107300.

#### firewall/redirect/enabled

让 fw3 装载或跳过这一条端口转发/地址转换，不删除内部服务器。

- Apply impact: 停用后新流量不再由此规则转换；旧连接的 conntrack NAT 状态可能尚存。
- Evidence: `docs/field-help-firewall-research.md:87` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 enabled，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107268.

#### firewall/redirect/ipset

用已存在的 IP 集合筛选参与此地址转换的流量；保留原生集合方向语法。

- Condition: 须存在有效 ipset，集合地址族与规则一致。
- Apply impact: 重载后集合外流量不走此转换；集合未知或支持关闭时该 redirect 会被跳过。
- Evidence: `docs/field-help-firewall-research.md:92` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 ipset，使用IP 集合引用解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107348.
- Evidence: `docs/field-help-firewall-research.md:218` (Xiaomi RN02 1.0.43) — 原厂用 --match-set 匹配集合；ipset 支持关闭或集合未知会跳过相关规则。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/redirect/mark

按转换前已有的数据包标记及掩码筛选此 NAT 规则，不写新标记。

- Format / range: 32 位标记[/掩码]；省略掩码为 0xffffffff
- Apply impact: 重载后只让指定标记流量使用转发；与 QoS/策略路由的标记分配需一致。
- Evidence: `docs/field-help-firewall-research.md:112` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 mark，使用标记/掩码解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107668.
- Evidence: `docs/field-help-firewall-research.md:219` (Xiaomi RN02 1.0.43) — 原厂解析标记/掩码并生成 --mark；省略掩码用 0xffffffff。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/redirect/limit

限制能够匹配此地址转换的平均包速率，不是转发服务的带宽限制。

- Unit: 包/second、minute、hour 或 day
- Condition: 与 limit_burst 配合；超过匹配速率不表示自动丢弃。
- Apply impact: 重载后超限包不匹配这条转换，可能转由其他 NAT/过滤规则处理。
- Evidence: `docs/field-help-firewall-research.md:103` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 limit，使用速率解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107524.
- Evidence: `docs/field-help-firewall-research.md:220` (Xiaomi RN02 1.0.43) — 原厂速率单位表含 second/minute/hour/day，生成 --limit/--limit-burst。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/redirect/limit_burst

指定此转换的 limit 匹配允许的突发包数。

- Unit: 包
- Condition: 仅配合本段 limit；不是秒数或字节数。
- Apply impact: 重载后影响转发首次或短时突发匹配；持续平均速率仍由 limit 决定。
- Evidence: `docs/field-help-firewall-research.md:104` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 limit_burst，使用32 位数值解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107540.
- Evidence: `docs/field-help-firewall-research.md:220` (Xiaomi RN02 1.0.43) — 原厂速率单位表含 second/minute/hour/day，生成 --limit/--limit-burst。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/redirect/target

DNAT 改写目的地址/端口；SNAT 改写来源地址/端口。同名地址字段的意义随此动作变化。

- Source fallback: DNAT（fw3 缺失或无效动作回退）
- Format / range: DNAT / SNAT
- Condition: DNAT 的转换目标为 dest_ip/dest_port；SNAT 的改写来源为 src_dip/src_dport。
- Apply impact: 重载后改变流量的实际地址转换方向，错误动作可能转发到错误主机或以错误来源发送。
- Evidence: `docs/field-help-firewall-research.md:115` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 target，使用动作枚举解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107716.
- Evidence: `docs/field-help-firewall-research.md:226` (Xiaomi RN02 1.0.43) — 原厂 target 回退 DNAT，init validator 只列 DNAT/SNAT。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.
- Evidence: `etc/init.d/firewall:31` (Xiaomi RN02 1.0.43) — redirect 的 target 校验接受 SNAT 或 DNAT。

#### firewall/redirect/src_dip

DNAT 时匹配外部目的地址；SNAT 时填写改写后的来源地址。它不是两个动作都通用的外部匹配地址。

- Compact help: DNAT 的外部目的匹配；SNAT 的改写来源地址。
- Condition: 意义由 target 决定；地址族应与 family 一致。
- Apply impact: 重载后可限制 DNAT 暴露的外部地址，或改变 SNAT 对外显示的来源地址。
- Evidence: `docs/field-help-firewall-research.md:98` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 src_dip，使用地址/网络解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107444.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/forward-details.lua:296–301` (Xiaomi RN02 1.0.43) — DNAT 表单定义 External IP address，匹配原始目的 IP。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/rule-details.lua:389–397` (Xiaomi RN02 1.0.43) — SNAT 旧式 redirect 表单将 src_dip 定义为 SNAT IP address。
- Evidence: `docs/field-help-firewall-research.md:226` (Xiaomi RN02 1.0.43) — redirect 两种转换方向的字段含义不同。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/redirect/src_dport

DNAT 时匹配路由器收到的外部目的端口；SNAT 时指定改写后的来源端口。

- Compact help: DNAT 的外部目的端口；SNAT 的改写来源端口。
- Unit: 端口号
- Format / range: 协议端口 0–65535；支持原生范围
- Condition: target 决定含义；proto 应包含 TCP/UDP 等带端口协议。
- Condition: 原厂此表项为单值，不是规则段的端口列表。
- Apply impact: 重载后改变公开服务的入口端口（DNAT），或改变出站来源端口（SNAT）。
- Evidence: `docs/field-help-firewall-research.md:99` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 src_dport，使用端口/范围解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107460.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/forward-details.lua:335–346` (Xiaomi RN02 1.0.43) — DNAT 表单将 src_dport 描述为外部目的端口/范围。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/rule-details.lua:441–453` (Xiaomi RN02 1.0.43) — SNAT 表单将 src_dport 作为改写后来源端口，空值不改端口。

#### firewall/redirect/reflection

为适用的 DNAT 生成 NAT 回环，让内网客户端可经外部地址访问内部转发服务。

- Condition: 依赖有效 DNAT、来源区域地址与内部目标；reflection_src 选择回环来源地址。
- Apply impact: 重载后可能增加内部回环的 DNAT/SNAT 规则；关闭后经外部地址访问服务可能失败，直接内网访问不受此开关控制。
- Evidence: `docs/field-help-firewall-research.md:113` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 reflection，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107684.
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/forward-details.lua:417–441` (Xiaomi RN02 1.0.43) — reflection 标为 Enable NAT Loopback；cfgvalue 缺失时返回 1，仅这是 LuCI 显示回退。
- Evidence: `docs/field-help-firewall-research.md:227` (Xiaomi RN02 1.0.43) — 原厂反射开关与 internal/external 来源枚举存在。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/redirect/reflection_src

选择回环 SNAT 使用内部区域地址或外部区域地址；不是客户端原始来源地址过滤。

- Format / range: internal / external
- Condition: reflection 启用且 DNAT 回环能生成；相关区域必须有可用地址。
- Apply impact: 重载后改变内部服务看到的回环来源地址，可能影响服务器基于来源地址的访问控制。
- Evidence: `docs/field-help-firewall-research.md:114` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 reflection_src，使用回环来源枚举解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107700.
- Evidence: `docs/field-help-firewall-research.md:227` (Xiaomi RN02 1.0.43) — 原厂 reflection_src enum 只有 internal 与 external。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/redirect/reflection_zone

通用 schema 中用于选择 NAT 回环区域的列表；1.0.43 的 redirect 选项表没有此名称。

- Compact help: 通用回环区域字段；1.0.43 未找到 reflection_zone 消费。
- Flags: version-dependent
- Condition: 原厂已解析 reflection/reflection_src；本字段支持未证实。
- Apply impact: 不能保证按此列表选择回环区域；原厂已确认的开关是 reflection 和 reflection_src。
- Trace result: 已搜索完整 fw3 redirect 表、全部 fw3 NUL 字符串、firewall init/hotplug/lib/vendor shell 及全部既有 Lua 反编译；未找到精确字段 reflection_zone。
- Evidence: `docs/field-help-firewall-research.md:243` (Xiaomi RN02 1.0.43) — 原厂 redirect 完整选项表与限定搜索中未找到 reflection_zone。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.
- Evidence: `docs/field-help-firewall-research.md:114` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 redirect 选项表读取 reflection_src，使用回环来源枚举解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107700.

### nat

#### firewall/nat/name

此源 NAT 规则的识别名称，用于显示与注释，不是来源地址。

- Apply impact: 重载后改变规则的识别信息，不直接改变 SNAT/MASQUERADE 匹配。
- Evidence: `docs/field-help-firewall-research.md:150` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 nat 选项表读取 name，使用字符串解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108308.
- Evidence: `docs/field-help-firewall-research.md:213` (Xiaomi RN02 1.0.43) — 原厂 name 由字符串解析器读取，规则显示使用 Rule/NAT/Redirect 名称格式；不是包匹配条件。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/nat/src

指定源 NAT 挂载的出站区域；原厂 nat 生成 zone_<区域>_postrouting 链，与 rule.src 的入站含义不同。

- Compact help: 源 NAT 的出站区域；与流量规则 src 的入站含义不同。
- Condition: 引用防火墙 zone.name，不是 network 逻辑接口。
- Apply impact: 重载后改变哪些出接口流量使用此源 NAT；填错区域会转换错误出口或不命中。
- Evidence: `docs/field-help-firewall-research.md:152` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 nat 选项表读取 src，使用区域/设备引用解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108340.
- Evidence: `docs/field-help-firewall-research.md:228` (Xiaomi RN02 1.0.43) — 原厂 nat.src 对应出站区域的 zone_<区域>_postrouting 链；不是 rule.src 的入站区域。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/nat/dest

通用 schema 的目标区域字段；1.0.43 fw3 的 nat 表没有 dest，只找到 dest_ip/dest_port 匹配。

- Compact help: 通用字段；1.0.43 未找到 nat.dest 消费。
- Flags: version-dependent
- Condition: 引用防火墙 zone.name，不是 network 逻辑接口。
- Apply impact: 不能确认 nat.dest 会限制出口；原厂源 NAT 出站区域由 src 指定。
- Trace result: 搜索 fw3 完整 nat 选项表、firewall init/hotplug/lib/vendor shell 及全部既有 Lua 反编译，未发现 nat.dest 读取；其他段同名字段不等于本段支持。
- Evidence: `docs/field-help-firewall-research.md:246` (Xiaomi RN02 1.0.43) — 完整 nat 表中没有 dest，范围内 shell/Lua 消费搜索也未找到该段字段。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.
- Evidence: `docs/field-help-firewall-research.md:152` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 nat 选项表读取 src，使用区域/设备引用解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108340.

#### firewall/nat/src_ip

匹配源 NAT 转换前的来源地址或网络；转换后的地址另由 snat_ip 指定。

- Condition: 地址族与 family 和区域一致；原厂此表项为单值，地址/网络前缀与匹配条件的否定按具体转换动作处理。
- Apply impact: 重载后改变哪些内部来源被 SNAT/伪装，不分配新的接口地址。
- Evidence: `docs/field-help-firewall-research.md:156` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 nat 选项表读取 src_ip，使用地址/网络解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108404.

#### firewall/nat/dest_ip

匹配源 NAT 流量的目的地址/网络；不是改写后的地址。

- Condition: 地址族与 family 和区域一致；原厂此表项为单值，地址/网络前缀与匹配条件的否定按具体转换动作处理。
- Apply impact: 重载后可按目的网络筛选源 NAT，不会把流量重定向到该地址。
- Evidence: `docs/field-help-firewall-research.md:160` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 nat 选项表读取 dest_ip，使用地址/网络解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108468.

#### firewall/nat/src_mac

通用 schema 的来源 MAC 条件；原厂 1.0.43 nat 选项表没有 src_mac。

- Compact help: 通用字段；1.0.43 未找到 nat.src_mac 消费。
- Flags: version-dependent
- Condition: 需可见的二层来源 MAC；不是远端设备身份认证。
- Apply impact: 无法保证源 NAT 按 MAC 筛选；不要把其他 rule/redirect 的 MAC 支持视为 nat 支持。
- Trace result: 搜索 fw3 完整 nat 选项表、firewall init/hotplug/lib/vendor shell 及全部既有 Lua 反编译，未发现 nat.src_mac 读取；其他段同名字段不等于本段支持。
- Evidence: `docs/field-help-firewall-research.md:247` (Xiaomi RN02 1.0.43) — 完整 nat 表中没有 src_mac，范围内 shell/Lua 消费搜索也未找到该段字段。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.
- Evidence: `docs/field-help-firewall-research.md:156` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 nat 选项表读取 src_ip，使用地址/网络解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108404.

#### firewall/nat/src_port

匹配源 NAT 转换前的来源端口；转换后的端口由 snat_port 指定。

- Compact help: 原厂此段按单值端口/范围解析，列表支持未证明。
- Unit: 端口号
- Format / range: 协议端口 0–65535；可用原生范围/否定语法
- Condition: 需 proto 含带端口的协议，常见为 TCP/UDP。
- Condition: 原厂此段端口表项为单值；可解析端口范围，未证明原生列表支持。
- Apply impact: 重载后缩小原始客户端端口的转换范围，不改变服务器目的端口。
- Evidence: `docs/field-help-firewall-research.md:157` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 nat 选项表读取 src_port，使用端口/范围解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108420.
- Evidence: `docs/field-help-firewall-research.md:216` (Xiaomi RN02 1.0.43) — 原厂端口解析识别范围与否定，并以 uint16 保存；生成 --sport/--dport。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/nat/dest_port

按原始目的服务端口筛选源 NAT；它不表示转换后的来源端口。

- Compact help: 原厂此段按单值端口/范围解析，列表支持未证明。
- Unit: 端口号
- Format / range: 协议端口 0–65535；可用原生范围/否定语法
- Condition: 需 proto 含带端口的协议，常见为 TCP/UDP。
- Condition: 原厂此段端口表项为单值；可解析端口范围，未证明原生列表支持。
- Apply impact: 重载后只改变哪些目的服务流量使用本规则；服务监听端口不变。
- Evidence: `docs/field-help-firewall-research.md:161` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 nat 选项表读取 dest_port，使用端口/范围解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108484.
- Evidence: `docs/field-help-firewall-research.md:216` (Xiaomi RN02 1.0.43) — 原厂端口解析识别范围与否定，并以 uint16 保存；生成 --sport/--dport。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/nat/proto

选择参与源 NAT 的 IP 协议；原厂 nat 缺失时回退 all，区别于 rule/redirect 的 TCP+UDP。

- Source fallback: all（fw3 缺协议回退）
- Apply impact: 重载后改变源 NAT 协议范围；端口条件还需要相应带端口协议。
- Evidence: `docs/field-help-firewall-research.md:155` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 nat 选项表读取 proto，使用协议解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108388.
- Evidence: `docs/field-help-firewall-research.md:215` (Xiaomi RN02 1.0.43) — 原厂 rule/redirect 缺协议回退 TCP+UDP；nat 缺协议回退 all。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/nat/family

限定此源 NAT 的 IPv4/IPv6 地址族；来源和 snat_ip 必须与所选族兼容。

- Format / range: any / ipv4 / ipv6
- Condition: IPv6 生成仍受 defaults.disable_ipv6 与内核模块支持约束。
- Apply impact: 重载后按地址族生成源 NAT；二进制解析支持不等于每族内核 NAT 可用。
- Evidence: `docs/field-help-firewall-research.md:151` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 nat 选项表读取 family，使用地址族解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108324.

#### firewall/nat/enabled

让 fw3 装载或跳过此源 NAT 段，不停用整个区域的 masq。

- Apply impact: 停用后流量可能回到区域伪装或其他源 NAT；旧连接 NAT 状态可能继续存在。
- Evidence: `docs/field-help-firewall-research.md:149` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 nat 选项表读取 enabled，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108292.

#### firewall/nat/ipset

引用已有集合，按集合成员及匹配方向筛选源 NAT。

- Condition: 须存在有效 ipset，集合地址族与规则一致。
- Apply impact: 重载后集合未知、族不一致或 ipset 支持关闭会使本规则被跳过。
- Evidence: `docs/field-help-firewall-research.md:154` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 nat 选项表读取 ipset，使用IP 集合引用解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108372.
- Evidence: `docs/field-help-firewall-research.md:218` (Xiaomi RN02 1.0.43) — 原厂用 --match-set 匹配集合；ipset 支持关闭或集合未知会跳过相关规则。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/nat/mark

按已有的包标记和掩码筛选源 NAT；不会设置来源 IP 或写新标记。

- Format / range: 32 位标记[/掩码]；省略掩码为 0xffffffff
- Apply impact: 重载后可把不同标记流量分配到不同源 NAT；需与上游标记规则配合。
- Evidence: `docs/field-help-firewall-research.md:173` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 nat 选项表读取 mark，使用标记/掩码解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108676.
- Evidence: `docs/field-help-firewall-research.md:219` (Xiaomi RN02 1.0.43) — 原厂解析标记/掩码并生成 --mark；省略掩码用 0xffffffff。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/nat/limit

限制本源 NAT 规则的平均匹配包速率，不是出口带宽。

- Unit: 包/second、minute、hour 或 day
- Condition: 与 limit_burst 配合；超过匹配速率不表示自动丢弃。
- Apply impact: 重载后超限包不匹配此源 NAT，可能由其他规则继续处理。
- Evidence: `docs/field-help-firewall-research.md:163` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 nat 选项表读取 limit，使用速率解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108516.
- Evidence: `docs/field-help-firewall-research.md:220` (Xiaomi RN02 1.0.43) — 原厂速率单位表含 second/minute/hour/day，生成 --limit/--limit-burst。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/nat/limit_burst

设置源 NAT 的 limit 匹配允许的突发包数。

- Unit: 包
- Condition: 仅配合本段 limit；不是秒数或字节数。
- Apply impact: 重载后改变短时突发命中的流量，不单独控制平均速度。
- Evidence: `docs/field-help-firewall-research.md:164` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 nat 选项表读取 limit_burst，使用32 位数值解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108532.
- Evidence: `docs/field-help-firewall-research.md:220` (Xiaomi RN02 1.0.43) — 原厂速率单位表含 second/minute/hour/day，生成 --limit/--limit-burst。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/nat/target

SNAT 用指定 snat_ip/snat_port 改写来源；MASQUERADE 使用动态出口地址。它不是允许/拒绝动作。

- Source fallback: MASQUERADE（fw3 缺失或无效动作回退）
- Format / range: SNAT / MASQUERADE
- Condition: SNAT 至少提供 snat_ip 或 snat_port；MASQUERADE 不应带这两个改写值。
- Apply impact: 重载后改变出站流量对外显示的来源；错误来源地址或端口可使回复无法返回。
- Evidence: `docs/field-help-firewall-research.md:174` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 nat 选项表读取 target，使用动作枚举解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108692.
- Evidence: `docs/field-help-firewall-research.md:229` (Xiaomi RN02 1.0.43) — 原厂缺/无效 target 回退 MASQUERADE；SNAT 必须有来源 IP 或端口。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/nat/snat_ip

target=SNAT 时指定改写后的来源 IP；原始来源匹配使用 src_ip。它不把此地址配置到接口上。

- Condition: target=SNAT；地址族须匹配 family；非 SNAT 使用 snat_ip 会被原厂校验拒绝。
- Apply impact: 重载后服务端将看到此来源地址；需要真实可路由的回程，否则连接无法成功。
- Evidence: `docs/field-help-firewall-research.md:158` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 nat 选项表读取 snat_ip，使用地址/网络解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108436.
- Evidence: `docs/field-help-firewall-research.md:229` (Xiaomi RN02 1.0.43) — SNAT 可提供 snat_ip；非 SNAT 的 snat_ip 触发 must not use 报错。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/nat/snat_port

target=SNAT 时改写来源端口或范围；不是原始来源/目的端口匹配。

- Unit: 端口号
- Format / range: 协议端口 0–65535；支持原生范围
- Condition: target=SNAT；proto 应含带端口协议；非 SNAT 使用该值会被校验拒绝。
- Condition: 原厂此表项为单值，不是规则段的端口列表。
- Apply impact: 重载后改变服务端看到的客户端端口，可能影响固定端口协议及并发映射。
- Evidence: `docs/field-help-firewall-research.md:159` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 nat 选项表读取 snat_port，使用端口/范围解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 108452.
- Evidence: `docs/field-help-firewall-research.md:229` (Xiaomi RN02 1.0.43) — SNAT 可提供 snat_port；非 SNAT 的 snat_port 触发 must not use 报错。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.
- Evidence: `docs/field-help-firewall-research.md:216` (Xiaomi RN02 1.0.43) — snat_port 使用原厂端口/范围解析器，端点保存为 uint16。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

### include

#### firewall/include/path

指定已有附加文件的路径。fw3 可装载规则文件，或把脚本作为 shell 文件执行；vendor loader 可继续插入小米规则。

- Condition: 配合 type；目标文件需要存在。原厂脚本包装不允许通过 config() 读取 UCI。
- Apply impact: 防火墙启动/重载后附加文件可改动实际规则与相关服务；改错路径会跳过，改错脚本可中断附加处理。
- Evidence: `docs/field-help-firewall-research.md:69` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 include 选项表读取 path，使用字符串解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 106948.
- Evidence: `docs/field-help-firewall-research.md:231` (Xiaomi RN02 1.0.43) — 原厂脚本路径检查及 shell 包装存在，vendor loader 另调用 sysapi.firewall。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.
- Evidence: `lib/firewall.sysapi.loader:7–9` (Xiaomi RN02 1.0.43) — 附加 loader 在 sysapi.firewall 可执行时调用它。

#### firewall/include/type

选择已有附加文件格式。1.0.43 fw3 的实际枚举是 script/restore；目录中 nftables 属于通用 fw4 语义，未找到原厂支持。

- Compact help: 1.0.43 原厂类型为 script/restore；nftables 未验证。
- Format / range: 原厂 fw3：script / restore；目录 nftables 为版本相关项
- Flags: version-dependent
- Apply impact: 重载时决定脚本执行或 iptables-restore 装载路径；nftables 不能当成此基线已支持的格式。
- Trace result: fw3 include.type 解析枚举表只含 script/restore；搜索同二进制及 firewall init/lib/vendor/Lua 消费者未找到 nftables。
- Evidence: `docs/field-help-firewall-research.md:70` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 include 选项表读取 type，使用include 类型枚举解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 106964.
- Evidence: `docs/field-help-firewall-research.md:230` (Xiaomi RN02 1.0.43) — include 类型表只有 script/restore，无 nftables。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.
- Evidence: `docs/field-help-firewall-research.md:245` (Xiaomi RN02 1.0.43) — 限定搜索未找到 nftables 消费；不否认其他固件可能支持。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/include/enabled

让 fw3 装载或跳过这一段附加文件，不能等同于附加脚本的内部服务开关。

- Condition: 只作用于本 include 段；不自动删除其他服务持有的规则。
- Apply impact: 停用后该段不再插入自定义规则；其他 include 或 vendor 服务仍可能插入相同功能规则。
- Evidence: `docs/field-help-firewall-research.md:68` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 include 选项表读取 enabled，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 106932.

#### firewall/include/reload

指定防火墙 reload 时重新装载此附加配置；原厂静态装载分支会检查该开关。

- Condition: enabled 启用且 path/type 有效；不同附加类型的执行分支不同。
- Apply impact: 开启可在重载后重新应用附加规则；脚本或规则文件需要能安全重复执行，字段本身不保证幂等。
- Evidence: `docs/field-help-firewall-research.md:72` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 include 选项表读取 reload，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 106996.
- Evidence: `docs/field-help-firewall-research.md:232` (Xiaomi RN02 1.0.43) — include.reload 保存于偏移 0x1c；reload 装载 restore 分支值为 0 时跳过。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/include/fw4_compatible

通用 fw4 兼容声明；1.0.43 使用 fw3，原厂 include 选项表没有此字段。

- Compact help: 通用 fw4 声明；1.0.43 fw3 未找到字段消费。
- Flags: version-dependent
- Apply impact: 此声明不会把 fw3 变为 fw4，也不能使 nftables 附加文件自动生效。
- Trace result: 搜索完整 fw3 include 表与全部 NUL 字符串、firewall init/hotplug/lib/vendor shell 及全部既有 Lua 反编译，未找到精确字段 fw4_compatible。
- Evidence: `docs/field-help-firewall-research.md:244` (Xiaomi RN02 1.0.43) — 原厂 include 完整表与限定消费者搜索未发现 fw4_compatible。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.
- Evidence: `etc/init.d/firewall:75–77` (Xiaomi RN02 1.0.43) — 原厂 reload_service 调用 fw3 reload，而不是 fw4。

### ipset

#### firewall/ipset/name

集合的引用名称，由规则 ipset 字段指向；不是规则名称或 DNS 域名。

- Condition: 必须非空，且 ipset 工具/内核类型支持可用。
- Apply impact: 重载后以此名创建集合；改名后规则的引用需同步，否则相关规则可能跳过。
- Evidence: `docs/field-help-firewall-research.md:74` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 ipset 选项表读取 name，使用字符串解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107044.
- Evidence: `docs/field-help-firewall-research.md:236` (Xiaomi RN02 1.0.43) — 原厂按名称创建 ipset，并以 add 逐项添加成员。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.
- Evidence: `docs/field-help-firewall-research.md:233` (Xiaomi RN02 1.0.43) — 原厂检查 IP 集合必须使用单一地址族，不能为 any。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/ipset/family

指定该集合的 IPv4 或 IPv6 类型；原厂明确不接受 family=any，不是双栈集合。

- Compact help: 集合仅可选 IPv4 或 IPv6；原厂不接受 any 双栈。
- Format / range: ipv4 / ipv6；原厂不接受 any
- Condition: 集合成员与引用规则须使用同一地址族。
- Apply impact: 重载后创建单族集合；引用此集合的规则地址族不同会被跳过。
- Evidence: `docs/field-help-firewall-research.md:75` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 ipset 选项表读取 family，使用地址族解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107060.
- Evidence: `docs/field-help-firewall-research.md:233` (Xiaomi RN02 1.0.43) — 原厂校验报错 must not have family any。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/ipset/match

定义集合键的类型和方向，例如 src_net；由 src_/dst_/dest_ 加 ip、net、port、mac 或 set 等类型组合。

- Format / range: 1–3 个匹配类型；组合须受 storage 与 ipset 支持
- Condition: 配合 storage 与 entry；不能只有集合名而没有 match 类型。
- Apply impact: 重载后决定成员如何解释与规则如何匹配；类型改变后旧 entry 可能不再合法。
- Evidence: `docs/field-help-firewall-research.md:77` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 ipset 选项表读取 match，使用集合匹配类型解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107092.
- Evidence: `docs/field-help-firewall-research.md:234` (Xiaomi RN02 1.0.43) — 原厂识别方向前缀与类型表，并校验最多三个数据类型。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/ipset/storage

选择 bitmap、hash 或 list 集合存储方法；必须与 match 类型组合兼容。

- Format / range: bitmap / hash / list；实际支持组合依 match
- Condition: bitmap 可能还需 iprange/portrange，当前目录未列这些范围参数；缺值时 fw3 按 match 推定方法而非固定默认。
- Apply impact: 重载后改变集合创建方式与内存/容量特征；无效组合会使集合无法建立。
- Evidence: `docs/field-help-firewall-research.md:76` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 ipset 选项表读取 storage，使用集合存储枚举解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107076.
- Evidence: `docs/field-help-firewall-research.md:235` (Xiaomi RN02 1.0.43) — 原厂存储枚举为 bitmap/hash/list；缺失方法按 matches 推定。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/ipset/entry

集合的静态成员列表，内容必须符合 match 定义的地址、网段、端口或组合；每个原生列表项保持独立。

- Condition: 成员格式与 family、match、storage 一致；规则须另引用本集合。
- Apply impact: 重载后成员逐项添加到集合，直接改变引用该集合的流量匹配；不创建防火墙允许动作。
- Evidence: `docs/field-help-firewall-research.md:85` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 ipset 选项表读取 entry，使用集合成员解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107220.
- Evidence: `docs/field-help-firewall-research.md:236` (Xiaomi RN02 1.0.43) — 原厂 entry 列表使用 add %s %s 生成成员添加。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/ipset/maxelem

控制支持此选项的集合最多容纳的成员数；原厂会为不适用的存储/类型组合忽略它。

- Unit: 成员
- Condition: 需 storage/type 组合支持 maxelem；目录输入下限 1，不代表已证明原厂固定最大值。
- Apply impact: 重载后影响集合容量和内存；容量不足时后续成员可能添加失败，不是吞吐限速。
- Evidence: `docs/field-help-firewall-research.md:81` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 ipset 选项表读取 maxelem，使用32 位数值解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107156.
- Evidence: `docs/field-help-firewall-research.md:237` (Xiaomi RN02 1.0.43) — 原厂创建 maxelem %u，部分组合明确 maxelem ignored。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/ipset/timeout

指定支持超时的集合成员生存期；按 ipset 协议为秒，0 表示不自动超时。不是 TCP 连接超时。

- Unit: 秒
- Format / range: 非负秒数；0 不自动超时（ipset 协议语义）
- Condition: 所选 ipset 类型须支持 timeout；成员被重新加入时超时可重新计算。
- Apply impact: 重载后集合成员可按超时消失，使引用规则不再命中；不会自动中断所有现有连接。
- Evidence: `docs/field-help-firewall-research.md:83` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 ipset 选项表读取 timeout，使用32 位数值解析器；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107188.
- Evidence: `docs/field-help-firewall-research.md:238` (Xiaomi RN02 1.0.43) — 原厂向 ipset create 输出 timeout %u；秒/0 的解释是 ipset 协议语义，不是固件设置默认。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

#### firewall/ipset/enabled

让 fw3 创建/装载或跳过此集合，成员配置仍保留。

- Condition: 引用它的规则需与集合启用状态配合。
- Apply impact: 停用后引用此集合的 rule/redirect/nat 可能因集合缺失而跳过；不是单独停用那些规则段。
- Evidence: `docs/field-help-firewall-research.md:73` (Xiaomi RN02 1.0.43) — 原厂 fw3 的 ipset 选项表读取 enabled，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`; offset 107028.
- Evidence: `docs/field-help-firewall-research.md:218` (Xiaomi RN02 1.0.43) — 原厂 rule/redirect/nat 遇未知集合会跳过。
  - Stock binary: `sbin/fw3`; SHA-256 `7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`.

## system

### system

#### system/system/hostname

设置路由器的系统主机名。原厂写入内核主机名，但不会改无线 SSID，也不决定 WAN DHCP 客户端发送的名称。

- Source fallback: OpenWrt（system 启动校验缺省）
- Condition: 本机 DNS 名称还取决于 dnsmasq.add_local_hostname、LAN 地址和运行模式。
- Apply impact: system 重载会写入内核主机名。dnsmasq 在路由模式且启用 add_local_hostname 时还用它生成本机 DNS 记录，需其重新加载才更新。
- Evidence: `etc/init.d/system:9–10` (Xiaomi RN02 1.0.43) — system 校验器将 hostname 缺省设为 OpenWrt。
- Evidence: `etc/init.d/system:37` (Xiaomi RN02 1.0.43) — hostname 写入 /proc/sys/kernel/hostname。
- Evidence: `etc/init.d/dnsmasq:1289–1294` (Xiaomi RN02 1.0.43) — 非 AP/中继模式且 add_local_hostname 生效时，以 system 主机名生成本机记录。
- Evidence: `lib/netifd/proto/dhcp.sh:47–52` (Xiaomi RN02 1.0.43) — WAN DHCP 主机名另取 misc.hardware.dhcp_hostname，缺省由硬件型号生成。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/system:37–37` (Xiaomi RN02 1.0.64) — 1.0.64 system 脚本同样将 hostname 写入内核主机名文件。

#### system/system/timezone

设置系统时间显示和本地定时任务使用的 POSIX 时区。原厂写入 /tmp/TZ；这是时区规则，不是 NTP 服务器地址。

- Source fallback: UTC（system 校验与 timezone 脚本回退；官方界面另有显示回退）
- Flags: version-dependent
- Condition: 有效 zonename 对应文件会替代 /tmp/TZ。
- Condition: 厂商 timezoneindex、webtimezone、初始化状态及地区映射可能覆盖 timezone。
- Apply impact: system 重载会设置用户空间和内核时区。厂商 timezone 服务也会根据地区或官方界面选择重写此字段；不要只把它当作显示标签。
- Evidence: `etc/init.d/system:13` (Xiaomi RN02 1.0.43) — timezone 校验缺省为 UTC。
- Evidence: `etc/init.d/system:39–46` (Xiaomi RN02 1.0.43) — timezone 写入 /tmp/TZ；有效 zonename 时改用 /tmp/localtime。
- Evidence: `etc/init.d/timezone:24–29` (Xiaomi RN02 1.0.43) — 按地区获得的 tz 写入 /tmp/TZ，并提交到 system.timezone。
- Evidence: `etc/init.d/timezone:48–73` (Xiaomi RN02 1.0.43) — timezoneindex、初始化状态与 webtimezone 决定使用地区时区还是已保存 timezone。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQSysUtil.lua:6072–6112` (Xiaomi RN02 1.0.43) — 厂商设置流程同时写 timezone、webtimezone、timezoneindex，随后重启 timezone 服务。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/system:39–46` (Xiaomi RN02 1.0.64) — 1.0.64 system 脚本同样写 /tmp/TZ、在有效 zonename 时建 localtime 链接并应用内核时区。

#### system/system/zonename

时区数据库文件名，例如 IANA 区域名称。仅当 /usr/share/zoneinfo 下的对应文件存在时使用；不是自动转换 timezone 的输入。

- Flags: version-dependent
- Condition: /usr/share/zoneinfo/<zonename> 必须存在。
- Condition: 与 timezone 一起保存；二者不是同一个字段。
- Apply impact: 有效文件会链接为 /tmp/localtime 并移除 /tmp/TZ；缺少文件时继续使用 POSIX timezone。实际可用区域取决于固件中安装的 zoneinfo。
- Evidence: `etc/init.d/system:14` (Xiaomi RN02 1.0.43) — zonename 校验为字符串，没有指定回退值。
- Evidence: `etc/init.d/system:39–43` (Xiaomi RN02 1.0.43) — 先写 /tmp/TZ；非空 zonename 且 zoneinfo 文件存在时建立 /tmp/localtime 链接并删除 /tmp/TZ。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/system:40–41` (Xiaomi RN02 1.0.64) — 1.0.64 脚本同样仅在指定 zoneinfo 文件存在时建立 /tmp/localtime 链接。

#### system/system/description

系统描述文本，与 hostname 的内核名称作用不同；1.0.43 未证实原厂服务使用此字段。

- Compact help: 系统描述文本；1.0.43 未找到原厂 system.description 消费。
- Flags: version-dependent
- Apply impact: 可保存设备说明；不能据此断言会改变官方设备名称、发现广播或主机名。已检查的 system 启动路径不展示描述。
- Trace result: 已检索 etc/init.d、lib、原厂 Lua 和反编译 XQSysUtil 的 system 读写，未找到 system 类型章节的 description 读取；XQSysUtil 同名项是备份状态描述，不是此字段。
- Evidence: `etc/init.d/system:9–14` (Xiaomi RN02 1.0.43) — system 校验声明 hostname、conloglevel、buffersize、timezone 和 zonename。
- Evidence: `etc/init.d/system:37–43` (Xiaomi RN02 1.0.43) — 此启动路径写内核主机名、dmesg 参数和时区文件。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQSysUtil.lua:5430–5435` (Xiaomi RN02 1.0.43) — XQSysUtil 的 description 命中属于备份状态输出，状态来自 /tmp/backup_files_status。

#### system/system/notes

设备备注文本；1.0.43 未证实原厂运行服务读取此备注。

- Compact help: 设备备注文本；1.0.43 未找到原厂 system.notes 消费。
- Flags: version-dependent
- Apply impact: 编辑可保留管理记录，但未确认官方界面会显示，也未确认它会改变服务启动或系统标识。
- Trace result: 已检索 system init、lib 脚本、原厂 Lua 与反编译 XQSysUtil，未找到精确 notes 字段或 system.notes 消费；备注语义来自现有字段定义，不作为原厂支持结论。
- Evidence: `etc/init.d/system:9–14` (Xiaomi RN02 1.0.43) — system 校验器列出启动所需的五个配置字段。
- Evidence: `etc/init.d/system:51–54` (Xiaomi RN02 1.0.43) — reload_service 加载 system 并只遍历 system 类型章节执行 system_config。

#### system/system/log_size

通用系统日志缓冲区大小，界面以 KiB 表示。1.0.43 未证实用此 UCI 字段配置原厂 syslog-ng。

- Compact help: 通用日志缓冲大小（KiB）；1.0.43 未证实 log_size 接入 syslog-ng。
- Unit: KiB（现有字段约定）
- Flags: version-dependent
- Apply impact: 不能保证扩大内存日志容量。原厂 syslog-ng 从独立配置读取队列和消息大小，不等同于此字段；实际内存影响未验证。
- Trace result: 已检索 system、syslog-ng、miwifi-logd init、lib 脚本及反编译 Lua，未找到精确 log_size 的读取或 UCI 到 syslog-ng 的转换。不能从 syslog-ng 的队列参数推导本字段缺省。
- Evidence: `etc/init.d/syslog-ng:10–18` (Xiaomi RN02 1.0.43) — syslog-ng 服务使用 /etc/syslog-ng.conf，启动前检查该文件与语法。
- Evidence: `etc/syslog-ng.conf:9–10` (Xiaomi RN02 1.0.43) — syslog-ng 配置直接指定 log_fifo_size 和 log_msg_size。
- Evidence: `docs/field-help-system-dropbear-research.md:87` (Xiaomi RN02 1.0.43) — 原厂 syslog-ng 动态解析库含 log_fifo_size token；它与配置中的 log_msg_size 都不等于 UCI log_size。
  - Stock binary: `usr/lib/libsyslog-ng-3.5.6.so`; SHA-256 `2c514cedebfc5032e75ab49a02c2866b46a37a91808d07583d6c9774f33b9765`; offset 320977.

#### system/system/log_ip

远程 syslog 目标地址或主机名；是通用字段语义，1.0.43 未找到原厂发送路径读取它。

- Compact help: 通用远程 syslog 目标；1.0.43 未找到 log_ip 的原厂发送路径。
- Flags: version-dependent
- Condition: 若其他日志实现支持它，通常还需 log_remote、log_proto 和 log_port；原厂联动未证实。
- Apply impact: 更改此地址不保证日志外发。原厂配置中的 127.0.0.1 UDP 入口是接收源，不是此远程目标。
- Trace result: 已检索 syslog-ng init、etc/syslog-ng.conf 与其包含目录、miwifi-logd init、lib 和反编译 Lua，未找到 system.log_ip 读取或远程目标生成。
- Evidence: `etc/init.d/syslog-ng:11` (Xiaomi RN02 1.0.43) — 原厂日志服务配置文件为 /etc/syslog-ng.conf。
- Evidence: `etc/syslog-ng.conf:24–26` (Xiaomi RN02 1.0.43) — net 日志源在回环地址监听 UDP。
- Evidence: `etc/syslog-ng.conf:45–51` (Xiaomi RN02 1.0.43) — 日志流汇集 src、net、kernel 源，输出到 d_messages。
- Evidence: `docs/field-help-system-dropbear-research.md:86` (Xiaomi RN02 1.0.43) — 原厂 syslog-ng 解析库的 cfgfile 帮助指向 /etc/syslog-ng.conf；地址配置仍属于该独立文件。
  - Stock binary: `usr/lib/libsyslog-ng-3.5.6.so`; SHA-256 `2c514cedebfc5032e75ab49a02c2866b46a37a91808d07583d6c9774f33b9765`; offset 331073.

#### system/system/log_port

远程 syslog 目标端口；不要与原厂回环 UDP 日志接收端口混为一谈。

- Compact help: 通用远程日志端口；原厂回环 UDP 514 接收源不是本项缺省。
- Format / range: 1–65535（传输层端口约束）
- Flags: version-dependent
- Condition: 远程目标需与 log_ip、log_proto、log_remote 配合；原厂是否读取未证实。
- Apply impact: 1.0.43 未证实此字段改变日志目的端口。即使保存合法端口，也不代表远程日志发送服务已启用。
- Trace result: 已检索 syslog-ng init/配置/包含目录、lib 和反编译 Lua，未找到精确 log_port 读取。配置中的监听 514 不能用作此 UCI 字段的缺省。
- Evidence: `etc/syslog-ng.conf:24–26` (Xiaomi RN02 1.0.43) — 配置中的 UDP 514 属于 net 日志接收源。
- Evidence: `etc/init.d/syslog-ng:21–25` (Xiaomi RN02 1.0.43) — syslog-ng 由 procd 启动，命令未携带 UCI 远程端口参数。
- Evidence: `docs/field-help-system-dropbear-research.md:86` (Xiaomi RN02 1.0.43) — 原厂 syslog-ng 解析库默认使用 /etc/syslog-ng.conf；本证据不指定 UCI 远程端口缺省。
  - Stock binary: `usr/lib/libsyslog-ng-3.5.6.so`; SHA-256 `2c514cedebfc5032e75ab49a02c2866b46a37a91808d07583d6c9774f33b9765`; offset 331073.

#### system/system/log_proto

远程日志传输协议，字段定义支持 udp 或 tcp；1.0.43 未证实此项会切换原厂日志输出。

- Compact help: 通用 UDP/TCP 发送选择；1.0.43 未证实原厂读取 log_proto。
- Format / range: udp / tcp（现有字段约定）
- Flags: version-dependent
- Condition: 需要支持此项的发送服务和可达 log_ip；原厂支持未确认。
- Apply impact: 不保证建立 TCP 连接或改用 UDP 发送。原厂配置里的 UDP 是本地接收源，不能据此推断远程发送协议。
- Trace result: 已检索 system/syslog-ng/miwifi-logd init、syslog-ng 配置目录、lib 和反编译 Lua，未找到精确 log_proto 读取或根据它生成 tcp/udp 目的地。
- Evidence: `etc/syslog-ng.conf:24–26` (Xiaomi RN02 1.0.43) — net 源使用 udp(ip(127.0.0.1) port(514)) 接收日志。
- Evidence: `etc/syslog-ng.conf:32–34` (Xiaomi RN02 1.0.43) — d_messages 是 /tmp/messages 文件目的地。
- Evidence: `docs/field-help-system-dropbear-research.md:89` (Xiaomi RN02 1.0.43) — syslog-ng 解析库含 tcp6 token，证明解析器能力而非 system.log_proto 消费或运行目标。
  - Stock binary: `usr/lib/libsyslog-ng-3.5.6.so`; SHA-256 `2c514cedebfc5032e75ab49a02c2866b46a37a91808d07583d6c9774f33b9765`; offset 335753.

#### system/system/log_remote

通用远程日志开关；不同于原厂 miwifi-logd 的启用条件。

- Compact help: 通用远程日志开关；原厂 miwifi-logd 的启动条件不是本项。
- Flags: version-dependent
- Condition: log_ip、log_port、log_proto 只有在消费者支持时才决定远程发送。
- Apply impact: 不能保证开启后外发日志，也不能保证关闭后停止厂商日志服务。miwifi-logd 的已知启动条件读取 NETMODE，配置触发器监听 xiaoqiang 与 milog。
- Trace result: 已检索 system、syslog-ng、miwifi-logd init、lib 与反编译 Lua，未找到精确 log_remote 的读取；不能将厂商日志服务的运行条件当作此开关实现。
- Evidence: `etc/init.d/miwifi-logd:9–16` (Xiaomi RN02 1.0.43) — miwifi-logd 仅在 NETMODE 等于 whc_cap 时启动，命令不携带 system 日志参数。
- Evidence: `etc/init.d/miwifi-logd:19–22` (Xiaomi RN02 1.0.43) — 该服务的重载触发器是 xiaoqiang 和 milog。
- Evidence: `docs/field-help-system-dropbear-research.md:90` (Xiaomi RN02 1.0.43) — syslog-ng 解析库含 udp6 token，不证明 UCI log_remote 驱动远程发送。
  - Stock binary: `usr/lib/libsyslog-ng-3.5.6.so`; SHA-256 `2c514cedebfc5032e75ab49a02c2866b46a37a91808d07583d6c9774f33b9765`; offset 335758.

#### system/system/log_file

通用本地日志文件路径。1.0.43 活跃 syslog-ng 配置直接指定文件，未证实使用此 UCI 路径。

- Compact help: 通用日志路径；原厂相关读取仅见注释，syslog-ng 直接配置文件目的地。
- Flags: legacy, version-dependent
- Apply impact: 编辑不能保证改变 /tmp/messages 的输出位置。脚本库中读取 log_file 的旧代码已注释，不会创建此路径或为它配置轮转。
- Trace result: 已检索 syslog-ng init/配置、lib/lib.scripthelper.sh 和反编译 Lua，system.log_file 读取只见于注释；qcawificfg80211 的同名 shell 变量是独立调试文件，不是本字段。
- Evidence: `etc/syslog-ng.conf:32–34` (Xiaomi RN02 1.0.43) — 原厂 d_messages 直接输出 /tmp/messages。
- Evidence: `lib/lib.scripthelper.sh:38–48` (Xiaomi RN02 1.0.43) — DAEMONSYSLOGFILE 回退为 syslog；从 system 取 log_file 的旧代码及创建目录代码均已注释。
- Evidence: `docs/field-help-system-dropbear-research.md:86` (Xiaomi RN02 1.0.43) — syslog-ng 解析库默认配置文件为 /etc/syslog-ng.conf；运行文件目的地需看该配置，不证明 UCI log_file 读取。
  - Stock binary: `usr/lib/libsyslog-ng-3.5.6.so`; SHA-256 `2c514cedebfc5032e75ab49a02c2866b46a37a91808d07583d6c9774f33b9765`; offset 331073.

#### system/system/conloglevel

内核控制台打印级别，加载时传给 dmesg -n；不设置用户空间 syslog 的过滤级别。

- Format / range: 0–8（内核控制台级别约定；init 仅校验非负整数）
- Condition: system 重载时生效；与 buffersize 共同决定是否执行 dmesg。
- Apply impact: 改变控制台上出现的内核消息数量。级别较高通常更详细；不会清空已有日志，也不等于更改远程日志策略。
- Evidence: `etc/init.d/system:11` (Xiaomi RN02 1.0.43) — conloglevel 校验为非负整数，无指定缺省。
- Evidence: `etc/init.d/system:38` (Xiaomi RN02 1.0.43) — 存在 conloglevel 时将其传入 dmesg -n；两项均为空时跳过 dmesg。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/system:38–38` (Xiaomi RN02 1.0.64) — 1.0.64 system 脚本同样通过 dmesg -n 应用 conloglevel。
- Evidence: `docs/field-help-system-dropbear-research.md:80` (Xiaomi RN02 1.0.43) — BusyBox 解压帮助将 dmesg -n LEVEL 解释为设置控制台日志级别，区别于 ring buffer 大小。
  - Stock binary: `bin/busybox`; SHA-256 `d9fa606a3c706a56bfd4f2e7d1c538a2f0ae132b016cd5392004b93123d46234`; offset 443357.

#### system/system/cronloglevel

crond 的日志详细程度，传给 -l；与内核控制台日志级别分开。

- Source fallback: 5（crond 启动参数回退）
- Format / range: 非负整数；界面提供 0–8，init 未规定该上限
- Condition: /etc/crontabs 中必须有任务；由 cron 启动读取。
- Apply impact: 下次 cron 启动时改变定时任务日志量，不改变任务周期。数值较小通常更详细；cron 没有注册 system 配置重载触发器，不能保证单独重载 system 即更新。
- Evidence: `etc/init.d/cron:15–25` (Xiaomi RN02 1.0.43) — 没有 crontab 时不启动；读取第一个 system 章节的 cronloglevel 并校验非负整数。
- Evidence: `etc/init.d/cron:30–35` (Xiaomi RN02 1.0.43) — crond 使用 -l 参数，空值回退为 5。
- Evidence: `etc/init.d/cron:39–41` (Xiaomi RN02 1.0.43) — service_triggers 仅注册校验函数。
- Evidence: `docs/field-help-system-dropbear-research.md:78` (Xiaomi RN02 1.0.43) — BusyBox crond 帮助说明 -l 设置日志级别，0 最详细，帮助中的编译默认是 8。
  - Stock binary: `bin/busybox`; SHA-256 `d9fa606a3c706a56bfd4f2e7d1c538a2f0ae132b016cd5392004b93123d46234`; offset 443357.

### timeserver

#### system/timeserver/enabled

控制厂商自动校时脚本；该脚本只读取名为 ntp 的章节，值为 0 时直接退出。

- Compact help: 原厂只读取命名 ntp 章节；enabled=0 跳过自动校时。
- Source fallback: 未设置时继续尝试（脚本仅对 0 退出）
- Flags: version-dependent
- Condition: 原厂读取 system.ntp.enabled；其他命名 timeserver 章节未证实被遍历。
- Condition: 需要已初始化且网络可用；厂商 timemode 可能重写此项。
- Apply impact: 停用会跳过以后的 ntpsetclock 调用，不会回退已经设置的时间。脚本还受初始化、联网配置和连通性检查限制；官方自动/手动时间模式可重写此项。
- Evidence: `usr/sbin/ntpsetclock:95–96` (Xiaomi RN02 1.0.43) — 读取 system.ntp.enabled，仅值为 0 时退出。
- Evidence: `usr/sbin/ntpsetclock:105–122` (Xiaomi RN02 1.0.43) — 校时前检查初始化、联网配置与网络就绪；失败则退出。
- Evidence: `etc/crontabs/root:3` (Xiaomi RN02 1.0.43) — 定时任务每 15 分钟调用 ntpsetclock。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQSysUtil.lua:7406–7436` (Xiaomi RN02 1.0.43) — 自动模式将 system.ntp.enabled 设为 1 并触发校时，其他模式将其设为 0。

#### system/timeserver/enable_server

通用 NTP 对外服务开关；与本机自动校时 enabled 不同。原厂 ntpd 支持 -l 服务模式，但未证实此 UCI 字段生成该参数。

- Compact help: 通用 NTP 对外服务开关；原厂仅证实单次客户端校时，未证实读取本项。
- Flags: version-dependent
- Condition: 若服务实现支持，对外校时还需监听接口和防火墙允许 UDP 123；原厂实现未确认。
- Apply impact: 不能保证开启后让局域网设备从本机同步。已确认的厂商流程以单次客户端模式运行 ntpd，不是根据此字段建立常驻 NTP 服务器。
- Trace result: 已检索 ntpsetclock、etc/init.d（该静态固件无 sysntpd 文件）、lib/netifd、原厂及反编译 XQSysUtil，未找到精确 system.ntp.enable_server 读取。客户端命令不能证明本字段支持。
- Evidence: `usr/sbin/ntpsetclock:38–43` (Xiaomi RN02 1.0.43) — ntp_sync 构建 /usr/sbin/ntpd -N -q -n -4 客户端命令。
- Evidence: `usr/sbin/ntpsetclock:51–55` (Xiaomi RN02 1.0.43) — 每个保存服务器附加 -p，再执行命令；该构建路径没有 enable_server 配置。
- Evidence: `docs/field-help-system-dropbear-research.md:79` (Xiaomi RN02 1.0.43) — BusyBox ntpd 帮助说明 -q 校时后退出、-l 同时作为 123 端口服务器；这是命令行能力。
  - Stock binary: `bin/busybox`; SHA-256 `d9fa606a3c706a56bfd4f2e7d1c538a2f0ae132b016cd5392004b93123d46234`; offset 443357.

#### system/timeserver/server

本机校时使用的 NTP 服务器列表。原厂读取 system.ntp.server，每项生成 ntpd -p；通过 -q 单次校时，不是长期对外服务。

- Compact help: 原厂读取 system.ntp.server 列表；其他命名章节未证实参与校时。
- Source fallback: 0.pool.ntp.org、1.pool.ntp.org、2.pool.ntp.org、3.pool.ntp.org、0.cn.pool.ntp.org（空列表时脚本回退）
- Flags: version-dependent
- Condition: 必须位于名为 ntp 的章节；enabled 不能为 0，且网络需通过检查。
- Condition: 主机名需要可用 DNS；保持原生列表格式，每项一个服务器。
- Apply impact: 下次校时使用新服务器；不能保证立即改时。ntpd 失败后厂商脚本尝试 HTTP 时间源；官方管理接口也能替换此列表并主动调用校时。
- Evidence: `usr/sbin/ntpsetclock:38–49` (Xiaomi RN02 1.0.43) — 空 system.ntp.server 回退到脚本定义的五个 pool 主机，并保存该回退列表。
- Evidence: `usr/sbin/ntpsetclock:51–55` (Xiaomi RN02 1.0.43) — 遍历服务器，为 ntpd 加 -p 参数并执行。
- Evidence: `usr/sbin/ntpsetclock:154–160` (Xiaomi RN02 1.0.43) — NTP 失败时尝试 htp_sync。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQSysUtil.lua:7266–7275` (Xiaomi RN02 1.0.43) — getNTPServerList 使用 get_list 读取 system.ntp.server。
- Evidence: `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQSysUtil.lua:7333–7357` (Xiaomi RN02 1.0.43) — 设置流程删除旧 server、写入新列表并提交，再调用 ntpsetclock now。
- Evidence: `docs/field-help-system-dropbear-research.md:79` (Xiaomi RN02 1.0.43) — BusyBox ntpd 帮助说明可重复的 -p PEER 从指定服务器获取时间，-q 在设时后退出。
  - Stock binary: `bin/busybox`; SHA-256 `d9fa606a3c706a56bfd4f2e7d1c538a2f0ae132b016cd5392004b93123d46234`; offset 443357.

#### system/timeserver/use_dhcp

通用“采用 DHCP 下发时间服务器”开关。DHCP 客户端确实上报 ntpserver/timeserver，但未证实此开关连接到厂商校时。

- Compact help: DHCP 会上报时间服务器，但未证实本项会将其用于原厂校时。
- Flags: version-dependent
- Condition: 上游 DHCP 必须提供时间服务器；还需有将接口数据送入校时服务的消费者。
- Apply impact: 不能保证开启后自动采用上游 NTP 地址。已确认的 ntpsetclock 仍取 system.ntp.server，未在该路径读取 DHCP 元数据。
- Trace result: 已检索 ntpsetclock、init、lib/netifd/dhcp.script、原厂与反编译 Lua，未找到 timeserver.use_dhcp 或 system.ntp.use_dhcp 读取；XQLanWanUtil 的同名 use_dhcp 属于 WAN IPv6 配置，不是此开关。
- Evidence: `lib/netifd/dhcp.script:117–120` (Xiaomi RN02 1.0.43) — DHCP 结果将 ntpsrv 与 timesvr 写入接口数据的 ntpserver/timeserver 字段。
- Evidence: `usr/sbin/ntpsetclock:41–49` (Xiaomi RN02 1.0.43) — 厂商脚本从 system.ntp.server 读取校时服务器，空值才用其内置列表。

#### system/timeserver/interface

通用 NTP 逻辑接口选择。原厂 ntpd 有 -I 的服务器接口绑定能力，但未证实 ntpsetclock 把此字段转为 -I。

- Compact help: 通用 NTP 接口选择；未证实原厂 ntpsetclock 使用本项绑定接口。
- Flags: version-dependent
- Condition: CLI 的 -I 用于服务器接口并隐含 -l；UCI 逻辑接口是否转成运行设备名未找到消费者。
- Apply impact: 不能保证修改后限制 NTP 的监听或发送接口。已确认的客户端命令只携带运行模式和服务器参数，实际出站路径仍取决于网络路由。
- Trace result: 已检索 ntpsetclock 全文、init、lib/netifd 和反编译 XQSysUtil 的 ntp 读写，未找到 system.ntp.interface 读取或转换；ELF -I 能力不证明 UCI 接线。
- Evidence: `usr/sbin/ntpsetclock:38–43` (Xiaomi RN02 1.0.43) — ntp_sync 初始化客户端命令并从 system.ntp.server 取服务器。
- Evidence: `usr/sbin/ntpsetclock:51–55` (Xiaomi RN02 1.0.43) — 该命令只追加服务器 -p 参数后执行，未生成接口绑定参数。
- Evidence: `docs/field-help-system-dropbear-research.md:79` (Xiaomi RN02 1.0.43) — BusyBox ntpd 支持 -I IFACE 绑定服务器接口且隐含 -l；这是命令行能力。
  - Stock binary: `bin/busybox`; SHA-256 `d9fa606a3c706a56bfd4f2e7d1c538a2f0ae132b016cd5392004b93123d46234`; offset 443357.

### led

#### system/led/name

这条 LED 配置的识别名称；不是硬件 sysfs 设备名。通用板级 helper 有此项，原厂灯服务未证实读取 system.led.name。

- Compact help: 通用 LED 配置名称；未证实原厂 xqled 读取 system.led.name。
- Flags: version-dependent
- Apply impact: 改名不等于切换物理灯。原厂 xqled 服务从自己的配置及状态 profile 启动，未确认会在官方界面显示此名称。
- Trace result: 已检索 etc/init.d、lib/functions/uci-defaults.sh、lib/xqled、led_ctl 和反编译 XQSysUtil，未找到遍历 system 的 led 章节并读取 name；板级 helper 写 JSON 不等于运行时消费此 UCI 项。
- Evidence: `lib/functions/uci-defaults.sh:367–376` (Xiaomi RN02 1.0.43) — 通用板级 LED helper 将名称和 sysfs 名加入 JSON led 对象。
- Evidence: `etc/init.d/xqled:8–16` (Xiaomi RN02 1.0.43) — 原厂启动 led_srv，使用 /lib/xqled/xqled.json 与 xqled.driver.profile。
- Evidence: `docs/field-help-system-dropbear-research.md:38` (Xiaomi RN02 1.0.43) — 原厂 led_srv 的 -c 选择配置文件；这是 init 使用的厂商 JSON 输入，不证明 system.led 字段读取。
  - Stock binary: `usr/sbin/led_srv`; SHA-256 `4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733`; offset 18932.

#### system/led/sysfs

通用 LED 类设备名称，对应 /sys/class/leds 下的目录；必须与实际硬件导出的名称匹配。1.0.43 未证实 system.led.sysfs 接入厂商灯服务。

- Compact help: 通用 LED 类设备名；原厂 xqled 用独立 led 字段，未证实此项接入。
- Flags: hardware-dependent, version-dependent
- Condition: 实际 /sys/class/leds 设备和相应灯控制后端必须存在。
- Apply impact: 不能保证更改后控制指定 LED。厂商 sysfs 后端取 xqled 动作中的 led 字段，并检查 brightness 文件，不取本字段。
- Trace result: 已检索 system/xqled init、lib/xqled 的 sysfs/gpio 后端、uci-defaults 与厂商 Lua，未找到 system.led.sysfs 读取；xqled 的 led 字段不是该字段别名。
- Evidence: `lib/functions/uci-defaults.sh:367–376` (Xiaomi RN02 1.0.43) — 通用 LED helper 将第三个参数以 sysfs 名写入 JSON。
- Evidence: `lib/xqled/xqled_common.sh:3–5` (Xiaomi RN02 1.0.43) — 厂商灯脚本的 THIS_MODULE 是 xqled。
- Evidence: `lib/xqled/xqled_sysfs.sh:23–38` (Xiaomi RN02 1.0.43) — 厂商动作从 led 字段取设备，要求 /sys/class/leds/<led>/brightness 存在。
- Evidence: `docs/field-help-system-dropbear-research.md:38` (Xiaomi RN02 1.0.43) — 原厂 led_srv 的 -c 选择配置文件；这是 init 使用的厂商 JSON 输入，不证明 system.led 字段读取。
  - Stock binary: `usr/sbin/led_srv`; SHA-256 `4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733`; offset 18932.

#### system/led/default

通用 LED 触发器接管前的默认亮灭状态；与触发器名称及厂商状态灯总开关不同。

- Compact help: 通用默认亮灭状态；原厂状态灯另由 xqled/BLUE_LED 控制。
- Flags: hardware-dependent, version-dependent
- Condition: 需要支持通用 system.led 的运行时加载器；触发器可能随后改变亮灭。
- Apply impact: 不能保证保存后立即亮灯，或让启动阶段持续保持此状态。1.0.43 启动灯服务使用厂商 profile，尚未发现将此项写入 brightness 的路径。
- Trace result: 已检索 init、lib/functions/uci-defaults.sh、lib/xqled、led_ctl 与反编译 Lua，未找到 system.led.default 消费；helper 只写板级 JSON，BLUE_LED 是另一模块的开关。
- Evidence: `lib/functions/uci-defaults.sh:379–384` (Xiaomi RN02 1.0.43) — 通用 ucidef_set_led_default 将第四个参数写入 JSON default。
- Evidence: `etc/init.d/xqled:10–16` (Xiaomi RN02 1.0.43) — 原厂 led_srv 读取 xqled JSON 和驱动 profile。
- Evidence: `usr/sbin/led_ctl:63–69` (Xiaomi RN02 1.0.43) — 原厂状态灯开关保存为 xiaoqiang.common.BLUE_LED，并调用 xqled 状态动作。
- Evidence: `docs/field-help-system-dropbear-research.md:38` (Xiaomi RN02 1.0.43) — 原厂 led_srv 的 -c 选择配置文件；这是 init 使用的厂商 JSON 输入，不证明 system.led 字段读取。
  - Stock binary: `usr/sbin/led_srv`; SHA-256 `4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733`; offset 18932.

#### system/led/trigger

通用内核 LED 触发器名；netdev、timer 等依赖已安装驱动。厂商 xqled 的 trigger 是另一套 on/off/blink 状态。

- Compact help: 通用内核 LED 触发器；不能与厂商 xqled 的 on/off/blink 混用。
- Flags: hardware-dependent, version-dependent
- Condition: 所选内核触发器须在对应 LED 的 trigger 属性中可用；原厂对本字段的支持未确认。
- Apply impact: 不能保证此项驱动原厂状态灯。不要把 xqled 的 blink 动作或 profile 状态名当作 system.led 已支持的内核触发器。
- Trace result: 已检索 system/xqled init、lib/xqled、led_ctl、通用 LED helper 和反编译 Lua，未找到 system.led.trigger 读取；同名 xqled trigger 属于独立模块，不能证明通用字段生效。
- Evidence: `lib/functions/uci-defaults.sh:496–501` (Xiaomi RN02 1.0.43) — 通用 helper 把 trigger_name 写入板级 LED JSON 的 trigger。
- Evidence: `lib/xqled/xqled_common.sh:5–10` (Xiaomi RN02 1.0.43) — xqled 定义的触发状态为 blink、on、off。
- Evidence: `lib/xqled/xqled_gpio.sh:62–64` (Xiaomi RN02 1.0.43) — GPIO 后端读取 xqled 功能的 trigger，随后按 blink/on 分支控制 GPIO。
- Evidence: `docs/field-help-system-dropbear-research.md:38` (Xiaomi RN02 1.0.43) — 原厂 led_srv 的 -c 选择配置文件；这是 init 使用的厂商 JSON 输入，不证明 system.led 字段读取。
  - Stock binary: `usr/sbin/led_srv`; SHA-256 `4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733`; offset 18932.

#### system/led/dev

通用 netdev LED 触发器监控的底层网络设备名，例如 eth0；不是 network 中的逻辑接口名称。

- Compact help: 通用 netdev LED 监控设备；未证实原厂会从此字段绑定网卡。
- Flags: hardware-dependent, version-dependent
- Condition: 通常需 trigger=netdev、有效 sysfs LED 以及存在的底层网络设备。
- Apply impact: 仅在 netdev 加载器支持时才会改变观察的网卡，不能修改网卡本身。1.0.43 未找到把此 UCI 字段绑定到 LED 设备的路径。
- Trace result: 已检索 init、lib/functions/uci-defaults.sh、lib/xqled 和反编译 Lua，未找到 system.led.dev 读取；helper 输出键是 device，也未找到将该 JSON 转成此 UCI 字段的消费者。
- Evidence: `lib/functions/uci-defaults.sh:409–417` (Xiaomi RN02 1.0.43) — 通用 netdev helper 取设备与模式，输出 type=netdev、device 和 mode。
- Evidence: `etc/init.d/xqled:14–16` (Xiaomi RN02 1.0.43) — 实际原厂灯服务以 xqled JSON/profile 启动。
- Evidence: `docs/field-help-system-dropbear-research.md:38` (Xiaomi RN02 1.0.43) — 原厂 led_srv 的 -c 选择配置文件；这是 init 使用的厂商 JSON 输入，不证明 system.led 字段读取。
  - Stock binary: `usr/sbin/led_srv`; SHA-256 `4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733`; offset 18932.

#### system/led/mode

通用 netdev 触发条件组合：link 表示链路状态，tx/rx 表示发送/接收活动。此组合不是网络接口工作模式。

- Compact help: 通用 link/tx/rx 条件组合；1.0.43 未证实原厂读取此 UCI 项。
- Format / range: link、tx、rx 的原生组合（通用 netdev 约定）
- Flags: hardware-dependent, version-dependent
- Condition: 与 trigger=netdev、dev 和 sysfs 一起解释；依赖内核触发器支持。
- Apply impact: 若加载器支持，可选择常亮链路提示或流量闪烁；1.0.43 未证实原厂把此项写入 netdev LED 触发属性。
- Trace result: 已检索 init、lib/xqled、通用 netdev helper 和反编译 Lua，未找到 system.led.mode 的运行时消费。helper 的 link tx rx 回退只对板级函数参数有效，不作为本 UCI 项缺省。
- Evidence: `lib/functions/uci-defaults.sh:409–417` (Xiaomi RN02 1.0.43) — netdev 板级 helper 的第五参数为空时采用 link tx rx，并写入 JSON mode。
- Evidence: `etc/init.d/xqled:10–16` (Xiaomi RN02 1.0.43) — 原厂 led_srv 使用独立 xqled JSON 与 profile。
- Evidence: `docs/field-help-system-dropbear-research.md:38` (Xiaomi RN02 1.0.43) — 原厂 led_srv 的 -c 选择配置文件；这是 init 使用的厂商 JSON 输入，不证明 system.led 字段读取。
  - Stock binary: `usr/sbin/led_srv`; SHA-256 `4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733`; offset 18932.

#### system/led/delayon

通用 timer/oneshot 触发器的点亮持续时间，以毫秒表示；不等同于厂商 xqled 的 msec_on。

- Compact help: 通用 timer 点亮时长（毫秒）；原厂另读 msec_on，不是本项。
- Unit: 毫秒（通用 timer 约定）
- Format / range: 非负整数；实际范围由 LED 触发器决定
- Flags: hardware-dependent, version-dependent
- Condition: 通常需 trigger=timer 或 oneshot，并与 delayoff 配合。
- Apply impact: 仅在对应通用触发器加载器支持时改变亮灯阶段。厂商 GPIO 闪烁另外读 msec_on，不能把其回退 800 当成本字段缺省。
- Trace result: 已检索 init、lib/xqled、led_ctl、通用 timer helper 和反编译 Lua，未找到 system.led.delayon 读取；板级 JSON 同名键和 xqled.msec_on 都不证明此 UCI 项已接入。
- Evidence: `lib/functions/uci-defaults.sh:477–486` (Xiaomi RN02 1.0.43) — 通用 timer helper 用第五参数写 JSON delayon、第六参数写 delayoff。
- Evidence: `lib/functions/uci-defaults.sh:492–493` (Xiaomi RN02 1.0.43) — timer helper 将触发器设为 timer。
- Evidence: `lib/xqled/xqled_gpio.sh:87–92` (Xiaomi RN02 1.0.43) — 厂商 GPIO 闪烁读 msec_on/msec_off，分别回退 800，并通过 MS2UNIT 转换。
- Evidence: `docs/field-help-system-dropbear-research.md:38` (Xiaomi RN02 1.0.43) — 原厂 led_srv 的 -c 选择配置文件；这是 init 使用的厂商 JSON 输入，不证明 system.led 字段读取。
  - Stock binary: `usr/sbin/led_srv`; SHA-256 `4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733`; offset 18932.

#### system/led/delayoff

通用 timer/oneshot 触发器的熄灭持续时间，以毫秒表示；与 delayon 分别控制一个闪烁周期的两段。

- Compact help: 通用 timer 熄灭时长（毫秒）；原厂另读 msec_off，不是本项。
- Unit: 毫秒（通用 timer 约定）
- Format / range: 非负整数；实际范围由 LED 触发器决定
- Flags: hardware-dependent, version-dependent
- Condition: 通常需 trigger=timer 或 oneshot，并与 delayon 配合。
- Apply impact: 仅在对应通用触发器加载器支持时改变灭灯阶段。原厂 xqled 读取自己的 msec_off，本字段不保证覆盖厂商灯效。
- Trace result: 已检索 init、lib/xqled、led_ctl、通用 timer helper 和反编译 Lua，未找到 system.led.delayoff 读取；xqled.msec_off 是另一字段，不能据此指定本字段回退。
- Evidence: `lib/functions/uci-defaults.sh:477–486` (Xiaomi RN02 1.0.43) — 通用 timer helper 以第六参数写 JSON delayoff。
- Evidence: `lib/xqled/xqled_gpio.sh:87–92` (Xiaomi RN02 1.0.43) — 厂商 blink 分支从功能配置读取 msec_off 并转换 GPIO 时间单位。
- Evidence: `docs/field-help-system-dropbear-research.md:38` (Xiaomi RN02 1.0.43) — 原厂 led_srv 的 -c 选择配置文件；这是 init 使用的厂商 JSON 输入，不证明 system.led 字段读取。
  - Stock binary: `usr/sbin/led_srv`; SHA-256 `4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733`; offset 18932.

#### system/led/interval

通用 LED 触发器的检查间隔，界面以毫秒表示；不是 timer 的亮灭持续时间。1.0.43 未确认其消费者与适用触发器。

- Compact help: 通用触发器检查间隔；原厂消费者、适用触发器与单位未确认。
- Unit: 毫秒（现有字段约定；原厂未确认）
- Flags: hardware-dependent, version-dependent
- Condition: 需要明确支持 interval 的触发器和对应加载器；不能假定所有触发器都使用它。
- Apply impact: 不能保证改变后调整网络活动检测频率或厂商灯效。已检查的 netdev helper 仅输出设备与模式，原厂 xqled 服务也未建立此 UCI 项的关联。
- Trace result: 已检索 init、lib/functions/uci-defaults.sh、lib/xqled、led_ctl 及反编译 Lua，未找到 system.led.interval 读取，也未找到通用 netdev helper 输出 interval；没有可证实的缺省或范围。
- Evidence: `lib/functions/uci-defaults.sh:409–417` (Xiaomi RN02 1.0.43) — 通用 netdev helper 写 type、device、mode。
- Evidence: `etc/init.d/xqled:14–18` (Xiaomi RN02 1.0.43) — 原厂启动命令只使用 xqled JSON/profile 与日志级别参数。
- Evidence: `docs/field-help-system-dropbear-research.md:38` (Xiaomi RN02 1.0.43) — 原厂 led_srv 的 -c 选择配置文件；这是 init 使用的厂商 JSON 输入，不证明 system.led 字段读取。
  - Stock binary: `usr/sbin/led_srv`; SHA-256 `4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733`; offset 18932.

## dropbear

### dropbear

#### dropbear/dropbear/Port

设置原厂 Dropbear SSH 的 TCP 监听端口，源码回退为 22。原厂支持端口列表，并为绑定接口的每个地址生成 -p 参数；独立救援通道使用自己的启动配置。

- Source fallback: 22（原厂 init 校验回退）
- Unit: TCP 端口
- Format / range: 1–65535（port 类型）；原厂校验允许列表
- Flags: version-dependent
- Condition: Interface 非空时仅绑定其已获取的 IPv4/IPv6 地址。
- Condition: 原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。
- Condition: be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。
- Apply impact: 重建原厂实例后，新连接改用相应端口；防火墙和客户端需匹配。此原厂实例配置不证明独立救援入口跟随改端口，应分别核对。
- Evidence: `etc/init.d/dropbear:41` (Xiaomi RN02 1.0.43) — Port 以 list(port) 校验，回退 22。
- Evidence: `etc/init.d/dropbear:15–27` (Xiaomi RN02 1.0.43) — 无绑定地址时生成 -p port，有地址时逐项生成 -p addr:port。
- Evidence: `etc/init.d/dropbear:82` (Xiaomi RN02 1.0.43) — 实例把解析出的地址和 Port 传给 append_ports。
- Evidence: `etc/init.d/dropbear:132–137` (Xiaomi RN02 1.0.43) — 原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/dropbear:82–82` (Xiaomi RN02 1.0.64) — 1.0.64 原厂实例同样把 Port 和接口地址传给 append_ports。
- Evidence: `docs/field-help-system-dropbear-research.md:26` (Xiaomi RN02 1.0.43) — Dropbear 服务端帮助给出 -p [address:]port。
  - Stock binary: `usr/sbin/dropbear`; SHA-256 `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2`; offset 153239.
- Evidence: `docs/field-help-system-dropbear-research.md:27` (Xiaomi RN02 1.0.43) — Dropbear 服务端帮助说明 -p 监听指定 TCP 端口及可选地址。
  - Stock binary: `usr/sbin/dropbear`; SHA-256 `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2`; offset 153257.

#### dropbear/dropbear/Interface

监听的 network 逻辑接口名。原厂解析该接口的全部 IPv4/IPv6 地址；空值则不传监听地址。

- Flags: version-dependent
- Condition: 填写逻辑接口而不是 eth0 等底层设备名；接口需要已有地址。
- Condition: 原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。
- Condition: be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。
- Apply impact: 修改会改变原厂实例的监听范围。指定接口但无可用地址时实例不启动；开机阶段先跳过有接口绑定的实例，随后由接口事件触发加载。独立救援绑定由其自己的启动配置决定。
- Evidence: `etc/init.d/dropbear:35` (Xiaomi RN02 1.0.43) — Interface 校验为字符串，没有配置缺省接口。
- Evidence: `etc/init.d/dropbear:61–68` (Xiaomi RN02 1.0.43) — Interface 非空时 BOOT 阶段跳过，否则查询接口所有地址，查询失败则返回。
- Evidence: `lib/functions/network.sh:131–143` (Xiaomi RN02 1.0.43) — network_get_ipaddrs_all 合并 IPv4 与 IPv6 地址，均无地址时失败。
- Evidence: `etc/init.d/dropbear:152–160` (Xiaomi RN02 1.0.43) — 配置变化触发 reload；已启用实例的 Interface 添加 interface.* 重载触发器。
- Evidence: `etc/init.d/dropbear:132–137` (Xiaomi RN02 1.0.43) — 原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/dropbear:61–68` (Xiaomi RN02 1.0.64) — 1.0.64 原厂实例同样解析 Interface 地址，启动阶段跳过绑定实例，无地址时返回失败。
- Evidence: `docs/field-help-system-dropbear-research.md:27` (Xiaomi RN02 1.0.43) — Dropbear 的 -p 参数支持指定监听地址，与 init 的 Interface 地址解析配合。
  - Stock binary: `usr/sbin/dropbear`; SHA-256 `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2`; offset 153257.

#### dropbear/dropbear/PasswordAuth

允许整个实例的密码认证；为 0 时原厂给 Dropbear 加 -s。它不保存或修改用户密码。

- Source fallback: 1（原厂 init 校验回退）
- Flags: version-dependent
- Condition: root 密码登录还取决于 RootPasswordAuth、RootLogin 及有效账户凭据。
- Condition: 原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。
- Condition: be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。
- Apply impact: 关闭后新登录不能使用密码认证，公钥认证不因此关闭。root 密码还需 RootPasswordAuth 和 RootLogin 允许；独立救援的认证策略不能由原厂 gate 推断。
- Evidence: `etc/init.d/dropbear:33` (Xiaomi RN02 1.0.43) — PasswordAuth 布尔校验回退 1。
- Evidence: `etc/init.d/dropbear:76` (Xiaomi RN02 1.0.43) — PasswordAuth=0 时向命令追加 -s。
- Evidence: `etc/init.d/dropbear:78–79` (Xiaomi RN02 1.0.43) — root 密码和 root 全部登录限制分别映射 -g/-w。
- Evidence: `etc/init.d/dropbear:132–137` (Xiaomi RN02 1.0.43) — 原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/dropbear:76–76` (Xiaomi RN02 1.0.64) — 1.0.64 原厂实例同样在 PasswordAuth=0 时追加 -s。
- Evidence: `docs/field-help-system-dropbear-research.md:28` (Xiaomi RN02 1.0.43) — 原厂 ELF 的 -s 帮助明确禁用密码登录。
  - Stock binary: `usr/sbin/dropbear`; SHA-256 `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2`; offset 152962.

#### dropbear/dropbear/RootPasswordAuth

允许或禁止 root 使用密码登录 SSH。原厂在值为 0 时追加 -g；它不自行禁用 root 公钥认证，root 登录还受其他认证开关控制。

- Source fallback: 1（原厂 init 校验回退）
- Flags: version-dependent
- Condition: root 密码需要 PasswordAuth=1、RootLogin=1 和有效 root 密码。
- Condition: 原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。
- Condition: be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。
- Apply impact: 关闭后 root 密码登录被拒绝。普通用户是否可用密码仍由 PasswordAuth 决定；RootLogin=0 会进一步拒绝 root 的全部认证方式。
- Evidence: `etc/init.d/dropbear:37` (Xiaomi RN02 1.0.43) — RootPasswordAuth 布尔校验回退 1。
- Evidence: `etc/init.d/dropbear:76–79` (Xiaomi RN02 1.0.43) — PasswordAuth=0 加 -s，RootPasswordAuth=0 加 -g，RootLogin=0 加 -w。
- Evidence: `etc/init.d/dropbear:132–137` (Xiaomi RN02 1.0.43) — 原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/dropbear:78–78` (Xiaomi RN02 1.0.64) — 1.0.64 原厂实例同样在 RootPasswordAuth=0 时追加 -g。
- Evidence: `docs/field-help-system-dropbear-research.md:29` (Xiaomi RN02 1.0.43) — 原厂 ELF 的 -g 帮助明确禁用 root 的密码登录。
  - Stock binary: `usr/sbin/dropbear`; SHA-256 `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2`; offset 152990.

#### dropbear/dropbear/RootLogin

允许 root 通过此实例登录；为 0 时追加 -w，限制不只是密码认证。

- Source fallback: 1（原厂 init 校验回退）
- Flags: version-dependent
- Condition: 若允许 root，认证方式还由 PasswordAuth、RootPasswordAuth 和账户公钥配置决定。
- Condition: 原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。
- Condition: be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。
- Apply impact: 关闭后原厂实例拒绝 root 登录，包括公钥。它不删除 root 账户或已有密钥，也不代表独立救援实例按相同策略运行。
- Evidence: `etc/init.d/dropbear:38` (Xiaomi RN02 1.0.43) — RootLogin 布尔校验回退 1。
- Evidence: `etc/init.d/dropbear:78–79` (Xiaomi RN02 1.0.43) — RootPasswordAuth=0 对应 -g；RootLogin=0 对应 -w，两者是独立参数。
- Evidence: `etc/init.d/dropbear:132–137` (Xiaomi RN02 1.0.43) — 原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/dropbear:79–79` (Xiaomi RN02 1.0.64) — 1.0.64 原厂实例同样在 RootLogin=0 时追加 -w。
- Evidence: `docs/field-help-system-dropbear-research.md:30` (Xiaomi RN02 1.0.43) — 原厂 ELF 的 -w 帮助明确禁止 root 登录。
  - Stock binary: `usr/sbin/dropbear`; SHA-256 `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2`; offset 152937.

#### dropbear/dropbear/GatewayPorts

允许 SSH 远程转发的监听接受非本机来源连接；原厂值为 1 时加 -a。这不是路由转发或网关的总开关。

- Source fallback: 0（原厂 init 校验回退）
- Flags: version-dependent
- Condition: 需要 SSH 客户端发起远程端口转发；防火墙仍决定可达性。
- Condition: 原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。
- Condition: be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。
- Apply impact: 可把远程转发端口暴露到其他可达设备，仍受客户端请求和防火墙约束。不改变 Dropbear 自身 Port/Interface，也不等于启用 WAN SSH。
- Evidence: `etc/init.d/dropbear:36` (Xiaomi RN02 1.0.43) — GatewayPorts 布尔校验回退 0。
- Evidence: `etc/init.d/dropbear:77` (Xiaomi RN02 1.0.43) — GatewayPorts=1 时给 Dropbear 追加 -a。
- Evidence: `etc/init.d/dropbear:82` (Xiaomi RN02 1.0.43) — SSH 服务自身监听由 append_ports 单独设置。
- Evidence: `etc/init.d/dropbear:132–137` (Xiaomi RN02 1.0.43) — 原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/dropbear:77–77` (Xiaomi RN02 1.0.64) — 1.0.64 原厂实例同样在 GatewayPorts=1 时追加 -a。
- Evidence: `docs/field-help-system-dropbear-research.md:31` (Xiaomi RN02 1.0.43) — 原厂 ELF 的 -a 帮助说明转发端口允许来自任意主机的连接。
  - Stock binary: `usr/sbin/dropbear`; SHA-256 `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2`; offset 153184.

#### dropbear/dropbear/IdleTimeout

SSH 连接无传输活动的超时，单位秒；非零时原厂传 -I，0 时不传此参数。

- Compact help: 空闲超时（秒）；非零传 -I，0 不传参数，最终取 Dropbear 自身缺省。
- Source fallback: 0（原厂 init 回退；不传 -I）
- Unit: 秒
- Format / range: 非负整数；0 不传超时参数
- Flags: version-dependent
- Condition: 与 SSHKeepAlive 分别控制空闲时长和保活发送。
- Condition: 原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。
- Condition: be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。
- Apply impact: 超时可断开无传输活动的连接；与定期保活的 SSHKeepAlive 不同。ELF 声明显式 -I 0 不超时，但原厂 init 遇 0 时省略 -I，最终取其编译缺省，不能仅靠 UCI 的 0 推断。
- Evidence: `etc/init.d/dropbear:43` (Xiaomi RN02 1.0.43) — IdleTimeout 以非负整数校验，回退 0。
- Evidence: `etc/init.d/dropbear:83` (Xiaomi RN02 1.0.43) — 仅 IdleTimeout 非零时追加 -I。
- Evidence: `etc/init.d/dropbear:132–137` (Xiaomi RN02 1.0.43) — 原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/dropbear:83–83` (Xiaomi RN02 1.0.64) — 1.0.64 原厂实例同样只在 IdleTimeout 非零时追加 -I。
- Evidence: `docs/field-help-system-dropbear-research.md:32` (Xiaomi RN02 1.0.43) — 原厂 ELF 的 -I 帮助声明空闲超时单位为秒、0 表示永不超时；编译缺省仍为格式占位符。
  - Stock binary: `usr/sbin/dropbear`; SHA-256 `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2`; offset 153557.

#### dropbear/dropbear/SSHKeepAlive

发送 SSH 保活消息的间隔，单位秒；非零时原厂传 -K，0 时省略该参数。

- Compact help: SSH 保活间隔（秒）；非零传 -K，0 不传参数，最终取 Dropbear 自身缺省。
- Source fallback: 300（原厂 init 校验回退）
- Unit: 秒
- Format / range: 非负整数；0 不传保活参数
- Flags: version-dependent
- Condition: SSHKeepAlive 不是 IdleTimeout；保活不会替代监听地址与认证配置。
- Condition: 原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。
- Condition: be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。
- Apply impact: 可用于发现失去响应的连接，与 TCP 系统级 keepalive 不同。ELF 声明显式 -K 0 不发送保活，但原厂 init 遇 0 时省略 -K，最终取其编译缺省；缩短间隔会增加保活消息。
- Evidence: `etc/init.d/dropbear:42` (Xiaomi RN02 1.0.43) — SSHKeepAlive 以非负整数校验，回退 300。
- Evidence: `etc/init.d/dropbear:84` (Xiaomi RN02 1.0.43) — 仅 SSHKeepAlive 非零时追加 -K。
- Evidence: `etc/init.d/dropbear:132–137` (Xiaomi RN02 1.0.43) — 原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/dropbear:84–84` (Xiaomi RN02 1.0.64) — 1.0.64 原厂实例同样只在 SSHKeepAlive 非零时追加 -K。
- Evidence: `docs/field-help-system-dropbear-research.md:33` (Xiaomi RN02 1.0.43) — 原厂 ELF 的 -K 帮助声明保活间隔单位为秒、0 表示不发送；编译缺省仍为格式占位符。
  - Stock binary: `usr/sbin/dropbear`; SHA-256 `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2`; offset 153504.

#### dropbear/dropbear/MaxAuthTries

每个 SSH 连接的认证尝试上限；非零时传 -T，0 时留给 Dropbear 自身缺省。

- Source fallback: 3（原厂 init 校验回退）
- Unit: 次/连接
- Format / range: init 校验非负整数；0 不传 -T，实际非零上限由 Dropbear 构建决定
- Flags: version-dependent
- Condition: 客户端发送的多把公钥也可能消耗认证尝试；不是账号密码修改。
- Condition: 原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。
- Condition: be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。
- Apply impact: 过小可能让带多个身份密钥的客户端在尝试正确凭据前就被断开。它不是全局失败次数或基于 IP 的封禁规则。
- Evidence: `etc/init.d/dropbear:44` (Xiaomi RN02 1.0.43) — MaxAuthTries 以非负整数校验，回退 3。
- Evidence: `etc/init.d/dropbear:85` (Xiaomi RN02 1.0.43) — 仅 MaxAuthTries 非零时追加 -T。
- Evidence: `etc/init.d/dropbear:132–137` (Xiaomi RN02 1.0.43) — 原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/dropbear:85–85` (Xiaomi RN02 1.0.64) — 1.0.64 原厂实例同样只在 MaxAuthTries 非零时追加 -T。
- Evidence: `docs/field-help-system-dropbear-research.md:34` (Xiaomi RN02 1.0.43) — 原厂 ELF 的 -T 帮助声明每次连接的最大认证尝试；非零值起点为 1，上限/缺省在此为占位符。
  - Stock binary: `usr/sbin/dropbear`; SHA-256 `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2`; offset 153059.

#### dropbear/dropbear/BannerFile

认证前发送给 SSH 客户端的提示文件路径；原厂校验 file，非空时传 -b。不是交互登录后的 shell 提示。

- Flags: version-dependent
- Condition: 路径指向路由器已有可读取的提示文件。
- Condition: 原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。
- Condition: be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。
- Apply impact: 加载有效文件后，新登录能看到提示；文件需在实例启动时可读取。它不会改变认证方式，也不能在此字段填写文件内容。
- Evidence: `etc/init.d/dropbear:40` (Xiaomi RN02 1.0.43) — BannerFile 校验为 file。
- Evidence: `etc/init.d/dropbear:81` (Xiaomi RN02 1.0.43) — 非空 BannerFile 通过 -b 传给 Dropbear。
- Evidence: `etc/init.d/dropbear:132–137` (Xiaomi RN02 1.0.43) — 原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/dropbear:81–81` (Xiaomi RN02 1.0.64) — 1.0.64 原厂实例同样以 -b 传入非空 BannerFile。
- Evidence: `docs/field-help-system-dropbear-research.md:36` (Xiaomi RN02 1.0.43) — 原厂 ELF 帮助说明提示文件内容在用户登录前展示。
  - Stock binary: `usr/sbin/dropbear`; SHA-256 `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2`; offset 152688.

#### dropbear/dropbear/keyfile

SSH 服务器主机密钥文件路径，不是客户端登录公钥内容。1.0.43 原厂 init 读取的是 rsakeyfile，未读取目录中列出的 keyfile。

- Compact help: 主机密钥文件路径；原厂 init 读取 rsakeyfile，不证明 keyfile 生效；独立救援另行管理。
- Flags: credential, version-dependent
- Condition: 文件需是有效主机私钥，勿在本字段粘贴私钥内容。
- Condition: keyfile 与 rsakeyfile 不是可证实的原厂别名；独立救援的使用需看其管理实现。
- Condition: be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。
- Apply impact: 保存 keyfile 不保证原厂切换主机身份；独立救援可按自己的管理实现使用密钥路径。真正更换主机密钥时客户端可能报告指纹变化，此项仍可编辑。
- Trace result: 已检索 etc/init.d/dropbear、lib、原厂与反编译 Lua，未找到 dropbear.keyfile 的读取；原厂校验/实例变量/命令生成均用 rsakeyfile。ELF 支持 -r keyfile 仅证明命令行能力，不证明 UCI keyfile 接线。
- Evidence: `etc/init.d/dropbear:39` (Xiaomi RN02 1.0.43) — 原厂声明的主机密钥字段是 rsakeyfile:file。
- Evidence: `etc/init.d/dropbear:80–81` (Xiaomi RN02 1.0.43) — 原厂把非空 rsakeyfile 传入 -r；紧邻的 BannerFile 则传入 -b。
- Evidence: `etc/init.d/dropbear:93–113` (Xiaomi RN02 1.0.43) — 原厂 keygen 检查默认 RSA 主机密钥，缺失时在 /tmp 生成并移到 /etc/dropbear。
- Evidence: `etc/init.d/dropbear:132–137` (Xiaomi RN02 1.0.43) — 原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/dropbear:80–80` (Xiaomi RN02 1.0.64) — 1.0.64 原厂实例仍从 rsakeyfile 而不是 keyfile 生成 -r 参数。
- Evidence: `docs/field-help-system-dropbear-research.md:37` (Xiaomi RN02 1.0.43) — 原厂 ELF 支持可重复的 -r 主机密钥文件参数；这不证明 UCI keyfile 被 init 读取。
  - Stock binary: `usr/sbin/dropbear`; SHA-256 `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2`; offset 152773.

#### dropbear/dropbear/enable

此 Dropbear 章节的启用开关；原厂值为 0 时不创建该实例。与 rc.common 的服务自启动 enable 命令不同。

- Source fallback: 1（原厂 init 校验回退）
- Flags: version-dependent
- Condition: 原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。
- Condition: be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。
- Apply impact: 停用此项会使原厂加载时跳过本章节，其他章节不因此关闭。不能用它推断独立救援 SSH 的状态，也不是强制删除已有会话的 killclients 命令。
- Evidence: `etc/init.d/dropbear:34` (Xiaomi RN02 1.0.43) — enable 布尔校验回退 1。
- Evidence: `etc/init.d/dropbear:70–75` (Xiaomi RN02 1.0.43) — enable=0 时从 dropbear_instance 返回，不创建实例。
- Evidence: `etc/init.d/dropbear:118–121` (Xiaomi RN02 1.0.43) — 接口触发器仅收集 enable=1 的章节接口。
- Evidence: `etc/init.d/dropbear:132–137` (Xiaomi RN02 1.0.43) — 原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/dropbear:70–70` (Xiaomi RN02 1.0.64) — 1.0.64 原厂实例同样在 enable=0 时返回。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/dropbear:132–137` (Xiaomi RN02 1.0.64) — 1.0.64 原厂脚本也在 ssh_en 不为 1 或 CHANNEL=release 时返回；只证明此 init 的门槛。

#### dropbear/dropbear/mdns

通过 procd 的 mDNS 集成公布 SSH TCP 服务。非零时注册 ssh/tcp，使用该实例的 Port。

- Source fallback: 1（原厂 init 校验回退）
- Flags: version-dependent
- Condition: 需要已启动的 SSH 实例和可用 mDNS responder；公布端口取 Port。
- Condition: 原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。
- Condition: be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。
- Apply impact: 有 mDNS responder 集成时，局域网客户端可发现 SSH 服务。此项不打开防火墙、不创建监听端口；缺少 mDNS responder 时公布行为取决于运行环境。
- Evidence: `etc/init.d/dropbear:46` (Xiaomi RN02 1.0.43) — mdns 布尔校验回退 1。
- Evidence: `etc/init.d/dropbear:88` (Xiaomi RN02 1.0.43) — mdns 非零时调用 procd_add_mdns ssh tcp，传入 Port 与 daemon=dropbear。
- Evidence: `etc/init.d/dropbear:132–137` (Xiaomi RN02 1.0.43) — 原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。
- Evidence: `live-inspection/field-help-live-1.0.64/etc/init.d/dropbear:88–88` (Xiaomi RN02 1.0.64) — 1.0.64 原厂实例同样通过 procd_add_mdns 公布 ssh/tcp 服务。
