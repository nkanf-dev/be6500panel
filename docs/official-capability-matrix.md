# be6500panel：RN02 官方功能 / OpenWrt 专业管理 parity 矩阵

## 0. Current document scope（先读）

- **当前任务现场基线**：父任务已确认设备为 Xiaomi BE6500 / RN02、当前 ROM `1.0.43`、已取得 LAN 上的 root 管理入口。获取 root 已完成，不再作为功能实现的前置待办。
- **本文件证据范围**：只读四份 `static/{api-review.md,boot-services-review.md,control-options.md,platform-review.md}`，以及已挂载的官方 `1.0.43` 解包目录 `/Volumes/RN02_STATIC/rootfs` 中的公开 UI、默认配置、服务脚本。使用现成 `static/api-routes.json` 与少量派生 Lua 定位名称/调用链。没有连接路由器，没有执行固件、安装软件或改项目代码。没有读取/复制 private snapshot。
- 旧报告中的“设备仍是 1.0.42 / 尚未 root”是**当时研究范围**，不覆盖当前现场基线。相同 ROM 号也不把静态默认配置变成运行状态；运行配置、热升级覆盖、NETMODE、硬件 offload、radio/port 映射以当前设备读回为准。
- **现有 corecontrol 的实现边界**：`network/wireless/dhcp/firewall/system/dropbear` 六包原始 UCI 事务。它们是控制基础，不是官方功能完整支持。`raw expert` 是诊断/高级编辑兜底；没有专用 adapter 的功能应列为“待实现”，不能用 raw 编辑入口算作 parity 完成。
- 文内 `/…` 是固件里的公开路径；`api/…` 是公开路由名，完整前缀为 `/cgi-bin/luci/`。不记录任何会话值、密钥、密码或账号。厂商 API 默认沿用其鉴权会话；be6500panel 使用自身 LAN 认证，厂商会话只保留在后端，不下发给浏览器或写入日志。

### 0.1 Parity 的交付含义

每个官方功能必须有：**发现能力 → 读状态/读配置 → 有类型的编辑 → 预览变更与影响 → 受控应用 → 运行状态核验 → 撤销或明确的生命周期任务**。读到 UCI、HTTP 返回 `code=0`、创建一条规则均不是完成条件。

- **官方 parity**：覆盖当前 RN02 官方 Web 与 App 管理功能，包括被简陋网页藏起来的既有管理能力。
- **专业管理**：在原厂 QSDK 底座上补齐路由/策略/服务/遥测的有类型管理。能利用已具备的能力，就提供专门页面与可核验后端。
- **底座归属**：保留原厂内核、QCA 无线固件、交换/端口与 PPE/SFE/ECM 驱动组合；管理层调用其控制面，不以通用 OpenWrt 模块替换这些硬件路径。

### 0.2 矩阵记法

| 标记 | 含义 |
|---|---|
| UI | 原厂 Web 模板直接有控件/调用 |
| API | 已有官方注册 handler；App/接口能力也纳入 parity |
| F | RN02 静态 feature 声明启用，仍须当前能力读回 |
| C | 按当前 NETMODE、已接外设、区域或 feature 条件出现 |
| D | 需要新增 be6500panel 专用 adapter；不是现有六包事务已实现 |
| B0 | 只读；不提交、不 reload |
| B1 | 小范围配置；先存恢复点，提交后 reload、读回；不改变管理路径 |
| B2 | 网络/无线/策略变更；连接可能中断，单一任务、超时回退、显式确认 |
| B3 | 重启/模式切换/全量恢复；独立异步任务，先准备重连与恢复路径 |
| B4 | OTA/恢复出厂；独立维护任务，不混入普通 UCI 保存 |

> 本矩阵的 reload/commit 与验证栏是待实现 adapter 的执行合同；列出的静态调用链不是本次设备运行结果。默认所有写操作在后台持有设备级写锁。厂商 setter 若内部已经 commit/restart，adapter 不得再假装它是“待提交 UCI 草稿”。必须先保存其涉及的整个配置域，再作为一个实际应用任务执行和核验。

## 1. Read matrix：当前 root 的发现与状态面

以下是下一 root phase 的读取合同，不是本次已访问设备的结果。读取只返回业务需要的字段；凭据字段返回“已设置”而非原值。

| 读域 | 当前读取接口 / 公开路径 | 产出与消费者 |
|---|---|---|
| 身份/构建/模式 | `/usr/share/xiaoqiang/xiaoqiang_version`、`/etc/openwrt_release`；`api/misystem/{status,newstatus,device_info}`；`xiaoqiang.common.NETMODE`；当前 `misc` feature 与厂商 feature helpers | 型号、ROM、kernel/ABI、router/AP/RE/CAP，当前能力与运行状态分离；用于所有 adapter 的前置检查 |
| 管理与服务 | 动态发现 `ubus` 对象/schema；`service list`、当前 init/procd、监听/绑定；读取与管理入口相关的 firewall zone | 当前管理链、代理服务状态、是否 LAN-only；标出每次操作对自身连接的影响 |
| 网络/端口 | `network.interface.* status`、`network.device status`（以实际 schema 为准）；地址/路由/规则/bridge、link counters；`api/xqnetwork/wan_info`；`api/misystem/{get_ps_map,get_ps_service}` | 逻辑 WAN1/WAN2、实际 ifname/物理口/速率、IP/DNS、桥/VLAN、拓扑与端口占用图 |
| DHCP/DNS/IPv6 | `api/xqnetwork/{lan_info,lan_dhcp,macbind_info,get_wan6_v2,get_wan6_info_v2,get_lan6_v2,get_ipv6_firewall}`；租约；当前 resolver、odhcpd/dnsmasq 状态 | 地址池/静态分配、DNS 来源、PD/RA/邻居状态；配置与下发结果并列 |
| 无线与终端 | `api/xqnetwork/{wifi_detail_all,get_hostap_mlo,get_twt,get_miotrelay_switch}`；`api/misystem/{devicelist,topo_graph}`；实际 driver/hostapd 查询 | radio/BSS/guest/miOT/MLO 映射、国家码、信道带宽、协商协议/链路、信号、速率、Mesh 拓扑；不能只显示 UCI enabled |
| 规则与快路径 | 当前 `iptables-save` / `ip6tables-save`、`ipset`、`tc` 统计；`ecm`、相关 debugfs 可用项；`mwan3/miqos/firewall_cpp` 状态 | 原厂 include 生成后的最终规则、mark 使用、队列、ECM 实际前端/连接缓存；用于策略一致性核验 |
| 系统/持久化 | uptime/load/memory/process；`/proc/mtd`、mounts、UBI 卷、空间/inode；`/data` 与配置 bind；公开服务/日志路径 | 资源预算、配置真实持久位置、重启维护方案、日志容量；只读展示分区与卷，常规 UI 不开放裸 flash 写入 |
| OTA/备份/恢复 | `api/xqsystem/{check_rom_update,ota,upgrade_status}`；厂商备份可选 key 清单；当前升级/恢复标志的业务投影 | 可恢复配置集合、维护状态机、版本兼容和签名校验结果；不公开包含秘密的原始包内容 |

能力记录至少有 `source`、`observedAt`、`config`、`runtime`、`writeAdapter`、`reloadPlan`、`impact`。无数据是 `unknown`，feature 关闭是 `disabled`，不适用是 `not-applicable`，尚未实现 adapter 是 `planned`；四者不能混成一个“不支持”。

## 2. 官方网络功能矩阵

| 官方功能与 UI 分类 | Read / Write 官方路由 | 配置归属与 root 接口 | reload / commit 安全边界 | 验证与 adapter 状态 |
|---|---|---|---|---|
| **WAN DHCP / PPPoE / 静态 IPv4**，DNS 自动/手动、MTU、特殊拨号、服务名、连接/断开、MAC 克隆、协商速率（UI） | R `api/xqnetwork/{wan_info,pppoe_status,get_wan_status,check_wan_type}`；W `{set_wan,pppoe_start,pppoe_stop,mac_clone,set_wan_speed}` | `network`、`pppoe` 与当前 `port_service` WAN 映射；setter 使用 `XQLanWanUtil`，`wan_name` 映射 WAN1/WAN2，不硬编码 eth 口 | **B2**；WAN 配置、上游 DNS、物理口速率分开预览；官方 helper 管理其 commit 与网络事件。root 实现时用实际 netifd/port_service 生命周期，不仅 `uci commit network` | WAN interface up、地址/默认路由、DNS、PPP 状态/失败原因；新连接出口、既有会话、断开/重拨；**D wan adapter** |
| **双 WAN**：第二 WAN、负载分担/故障切换、权重、按设备出口（UI/F） | R `api/xqnetwork/{get_multiwan_basic_info,get_multiwan_dev_list,get_multiwan_dev_policies}` + `api/misystem/get_ps_service`；W `api/xqnetwork/{set_multiwan_enable,set_multiwan_policy,set_multiwan_weight,set_multiwan_dev_policy}` + `api/misystem/set_ps_service` | `port_service`、`network`、`mwan3`；当前 physical service 映射与 member/policy/device、IPv4/IPv6 两族 | **B2**；端口分配先做冲突检测；配置域整体锁定，使用官方 multiwan helper，协调 `mwan3`、network reload 与加速连接失效；禁止与 QoS/PBR 并行改 mark | 两出口各自地址/DNS/连通；单链路故障/恢复、权重、多设备固定出口、HTTPS sticky、IPv6 和新旧连接；**D multiwan adapter** |
| **LAN IP/掩码/地址冲突处理**（UI） | R `api/xqnetwork/lan_info`；W `api/xqnetwork/set_lan_ip` | `network.lan`；`XQLanWanUtil` + `XQIPConflict`，伴随 LAN IP change event | **B3**；官方 setter 有成功后 reboot / Mesh 网关同步路径。单独任务，明确新管理地址、客户端重取地址与重连；不与 QoS/VPN 等提交混用 | 新地址可达、客户端同网段、DHCP 池/静态租约/DMZ/转发目标更新一致；Mesh 网关同步；**D lan adapter** |
| **LAN DHCP**：开关、范围/完整起止 IP、租期、网关/DNS（UI/F） | R `api/xqnetwork/lan_dhcp`；W `api/xqnetwork/set_lan_dhcp` | `dhcp`，`dnsmasq`；官方 helper 处理 `start/limit` 与 `startip/endip`、`router/dns1/dns2` | **B2**；静态链不只是 dnsmasq reload：helper restart dnsmasq，handler 还安排 network restart、`phyhelper restart lan`、`port_service restart`，并处理 IP/MAC binding | 新租约正确、范围不含路由器/保留地址、租期/DNS/网关正确、禁用后不应答；维护入口持续可达；**D dhcp adapter** |
| **DHCP 静态分配 / MAC 绑定 / IP-MAC 安全绑定**（UI/F） | R `api/xqnetwork/macbind_info`；W `{mac_bind,mac_unbind}`；更强 IP/MAC 检查能力按当前 feature 发现 | `macbind` 与 `dhcp` host 成对管理；`XQMacBind`，更强过滤另涉及 `XQIPMacBind` 与 firewall include `ipmacBind` | **B1**；唯一 MAC/IP、LAN containment、地址冲突预检；官方 reload 通知 `noflushd` 并 restart dnsmasq。不可只增 `dhcp.host` 而漏掉原厂关联表 | 设备重新取租约、静态地址/名称读回；冲突提示；安全绑定需同时验证 ARP 与访问规则；**D lease/binding adapter** |
| **IPv6**：总开关、WAN native/DHCPv6/PPPoEv6/static/中继、LAN RA/DHCPv6、PD、IPv6 防火墙（UI/F） | R `api/xqnetwork/{get_wan6_switch_v2,get_wan6_v2,get_wan6_info_v2,get_lan6_v2,get_ipv6_firewall}`；W `{set_wan6_switch_v2,set_wan6_v2,set_lan6_v2,set_ipv6_firewall}`；旧 `{set_wan6,ipv6_status}` 仅作兼容发现 | **`ipv6` + `network` + `dhcp` + `firewall`**；`/usr/sbin/ipv6.sh`、odhcp6c/odhcpd；当前 support_modes 作为可编辑枚举 | **B2**；用官方 v2 helper 整体管理。`setWan6Cfg` 涉及 network/firewall 与 `ipv6.sh reload/autocheck/macvlan`；`setLan6Cfg` 涉及 odhcpd、network、LAN PHY；firewall 模式经 `ipv6.sh set_firewall` | PD/RA/DHCPv6、LAN 全局地址、默认路由、AAAA/DNS、外网连通、入站 IPv6 规则、双 WAN；**D ipv6 adapter** |
| **DNS 管理**：WAN 上游、LAN 下发；专业域名转发/主机记录/上游策略 | 官方 WAN/LAN 路由如上；root typed DNS adapter 读写 `dhcp` DNS 相关字段、`network` peerdns/DNS；发现现有 dnsmasq ubus | `dnsmasq`、resolver auto 文件、厂商 DNS 重定向/sysapi include；专业功能使用已有 dnsmasq 能力，不另抢 53 端口 | **B1/B2**；区别“路由器使用 DNS”“给客户端下发 DNS”“过滤/重定向 DNS”。只改自己负责的条目；配置语法预检，再 reload dnsmasq，必要时网络域刷新 | 路由器查询/客户端查询、内部域、上游故障、IPv4/IPv6 DNS、重绑定保护、Mesh 解析；**D dns adapter** |
| **工作模式**：路由器、有线 AP、无线中继、Mesh CAP/RE（UI/F/C） | R 当前 NETMODE、`api/xqnetwork/{wifi_detail_all,wifi_list,wifiap_signal,wan_link}`；W `{set_lan_ap,disable_lan_ap,set_wifi_ap,disable_ap}`；App `{misystem/set_router_normal,set_router_wifiap,set_router_lanap}` | `xiaoqiang`、`network/wireless/dhcp/firewall`、Mesh 状态，厂商 mode helper | **B3**；转换完整拓扑，不是改一个字符串。AP/RE 的 firewall/odhcpd/云同步启动策略不同；显示转换后的管理地址与恢复任务 | 各模式地址来源、上联、DHCP 角色、桥、管理口、guest 隔离、Mesh 角色；**D mode adapter** |
| **IPTV / Internet VLAN / LAN 口选择 / 802.1p**（UI/C） | R `api/misystem/{get_ps_service,get_ps_map,get_vlan_iptv}`；W `{set_ps_service,set_vlan_iptv}` | **`port_service`** 服务/attr + 网络/交换运行配置；`XQPortServiceUtil.ps`，service 包含 `iptv/wantag`；当前口映射为准 | **B2**；VID/优先级/tagged/untagged 与口独占校验；helper 返回 `wait` 应进入 job。执行官方 `port_service restart` 及 firewall `reloadfw` 链，不套用其他型号 swconfig/DSA 教程 | Internet/机顶盒分别取地址、组播/IGMP、VLAN egress/tag、无跨域泄露、回滚后原 WAN/LAN 正常；**D port/VLAN adapter** |
| **自定义 WAN/LAN 口 / 自动识别 / 游戏口 / LAN LAG / 速率**（UI/F/C） | R `api/misystem/{get_ps_map,get_ps_service}` + WAN 速度状态；W `set_ps_service`、`api/xqnetwork/{set_wan_lan_port,set_wan_lan_swap,set_wan_speed}`；实际服务枚举发现 | `port_service`、`port_map`、`qca_nss_dp` 与 QCA8386/PHY；`lag/game/wandt` 不是一般 UCI network device 字段 | **B2**；物理口能力、服务互斥和管理口保留预检；driver/port_service 是唯一 owner；不同时发多口重配置 | 口图/速率/link/错误计数、WAN/LAN 角色、LAG 协商与双向吞吐、IPTV/游戏口功能与隔离；**D port adapter** |

## 3. 无线、Wi-Fi 7 与 Mesh 矩阵

| 官方功能与 UI 分类 | Read / Write 官方路由 | 配置归属与驱动 | reload / commit 安全边界 | 验证与 adapter 状态 |
|---|---|---|---|---|
| **2.4/5 GHz 无线**：SSID/密码/隐藏/开关、信道/带宽、功率、多频合一、MU-MIMO/波束赋形、Wi-Fi 5 兼容（UI/F） | R `api/xqnetwork/wifi_detail_all`；W `{set_wifi,set_all_wifi,set_wifi_txpwr,set_wifi_ax,set_wifi_txbf}`；区域 `{xqsystem/country_code,set_country_code}` | `wireless` + `misc` 映射、厂商 `XQWifiUtil`、QCA unified driver/firmware 与 hostapd；不能假定标准 mac80211 radio schema | **B2**；合并频段需一起修改关联 BSS/认证；Wi-Fi 5 兼容与 MLO/TWT/OFDMA 联动；使用官方 `forkRestartWifiNotify`/Mesh 通知生命周期，不只 commit wireless | SSID/加密/带宽/信道读回、终端关联与 DHCP、吞吐、旧设备兼容、所有 radio/BSS 正常、Mesh 配置同步；**D wifi adapter** |
| **Wi-Fi 7 / MLO 多链路**（UI/F） | R `api/xqnetwork/get_hostap_mlo`；W `set_hostap_mlo` | `misc.mld.hostap/hostap_mlo` 引用实际 hostap BSS；`wireless.*.mlo_enable`；QCA5332/QCN9224 firmware/driver 的 MLD 关系 | **B2**；按 feature、合一状态、ax 兼容模式验证依赖。官方 enable/disable 写关联 BSS，再 `forkRestartWifiNotify`；一个 MLO 域作为事务 | API 配置 + driver/hostapd 的 MLD/link 状态 + Wi-Fi 7 终端实际并发链路。普通单链路终端连接不是 MLO 验证；**D mlo adapter** |
| **TWT 节能**（UI/F） | R `api/xqnetwork/get_twt`；W `set_twt`（`on`） | `wireless.*.twt_responder`，厂商 `set_twt_hostap` | **B2**；所有关联 BSS一致；Wi-Fi 5 兼容模式禁止开启；官方 wifi restart+通知；保留旧终端兼容回退 | 配置读回、TWT-capable client/driver 会话信息、兼容终端连通与恢复；**D twt adapter** |
| **访客 Wi-Fi**：SSID/认证/开关、隔离、带宽（UI/F） | R `wifi_detail_all` + guest 状态；W `api/xqnetwork/set_wifi`（guest BSS）；QoS `api/misystem/qos_guest` | guest BSS/bridge、firewall guest/sysapi、`dhcp`、`miqos`；`misc.modules.guestwifi` 为映射，不硬编码 wl14 | **B2**；guest 不是单独一个 SSID。与网络隔离/DHCP/DNS/QoS同域编排；保留厂商特殊服务规则，但清楚展示例外 | guest→WAN、guest→LAN/管理接口、guest-client 隔离、IPv6 与 bridge fast path、新旧连接；**D guest adapter** |
| **miOT 快连 / IoT relay / 扫描**（UI/F/C） | R `api/xqnetwork/get_miotrelay_switch`；W `{miotrelay_switch,miscan_switch}`；配网 API `misystem/{get_unconfig_iotdev,set_config_iotdev,get_iotdev_status}` 作为 App 扩展 | `wireless.miot_2G.{bindstatus,userswitch}` + `network/dhcp` miot、sysapi `miot`、MiIO/配网服务；真实 radio 映射在 misc | **B1/B2**；relay setter 直接 enable/disable hostapd BSS、commit wireless、`sysapi miot`、miot LED 与 WHC/可选 mapd 同步；不将 IoT 专用 BSS 变普通 guest | 用户开关/绑定状态分别显示；IoT 关联/地址/隔离、快连、Mesh 传播；配网敏感值后端留存；**D miot adapter** |
| **Mesh**：扫描/加入/进度、拓扑、回程模式/开关、漫游弱信号（UI/F/C） | R `api/misystem/{topo_graph,get_mesh_bh_mode}`、`api/xqnetwork/{get_addnode_status,get_mesh_switch,wifi_detail_all}`；W `{scan_mesh_node,add_mesh_node,set_mesh_switch,set_wifi_weak}`、`misystem/set_mesh_bh_mode` | WHC/CAP/RE、backhaul/MLO BSS、minet/mesh broker、QCA emesh；官方脚本 `/sbin/whc_to_re_common_api.sh`，mapd 按存在发现 | **B2/B3**；加入是异步 job，配置变更在 controller 单点发起并验证 RE同步；回程切换可能失联，不能与无线重配同时执行 | 节点/角色/固件、回程类型/频段/链路、终端漫游、地址连续性、各 RE一致状态、失败节点重试/移除；**D mesh adapter** |

`split5g` / 电竞第三频段在通用模板有条件分支，RN02 静态 `XQFeatures` 未声明该项。`api/xqsystem/set_wifi_split` 可以纳入能力发现，但 UI 不应凭模板存在给当前两频设备增加第三个 radio。国家/信道/功率选项来自当前校准、区域与 driver 能力集合。

## 4. 流量、访问、安全与服务矩阵

| 官方功能与 UI 分类 | Read / Write 官方路由 | 配置与 lifecycle owner | reload / commit 安全边界 | 验证与 adapter 状态 |
|---|---|---|---|---|
| **QoS 智能限速**：总开关/模式、上下行带宽、设备优先级/限速、guest、应用优化（UI/F/API） | R `api/misystem/{qos_info,qos_dev_info}`；W `{qos_switch,qos_mode,qos_limits,qos_limit,qos_offlimit,qos_set_dev_info,qos_guest,set_band}`；`qos2_*` 作为当前 handler/能力发现项 | `miqos` + `tc-full`/HTB、厂商 mark；`XQQoSUtil`、`/etc/init.d/miqos`、`/lib/miwifi/arch/lib_arch_accel.sh` | **B2**；走 miqos on/off/apply，与 ECM service 协同。原厂 QoS start 选择 SFE、更新接口、flush ECM；stop 恢复 auto 条件。不得另一个服务同时占根 qdisc | `tc` class/filter/counter、受控负载下吞吐/延迟/公平性、CPU、ECM实际前端与命中、关闭恢复；**D qos+accel adapter** |
| **VPN PPTP/L2TP**：配置列表、选择/连接/断开/自动连；智能设备/域名分流（UI/API） | R `api/xqsystem/{vpn,vpn_status}`、`api/misystem/{smartvpn_info,mi_vpn_info}`；W `xqsystem/{set_vpn,del_vpn,vpn_switch,set_vpnauto}`、`misystem/{smartvpn_switch,smartvpn_url,smartvpn_mac,mi_vpn}` | `vpnlist`、`network.vpn`、`smartvpn`、`/usr/sbin/vpn.lua`、netifd PPTP/L2TP、PPP；路由表 `252 vpn` | **B2**；厂商列表与激活网络一起管理。PPP 凭据必须按 PPP tokenizer 正确编码，不能直接复用旧 handler 的文本边界缺陷；配置不会落入可执行 shell。协调 tunnel/link事件、路由、防火墙、ECM | 隧道 up/地址/路由、DNS、选中设备/域名出口、WAN恢复重连、防泄漏策略、MTU、失败原因；**D vpn adapter** |
| **DDNS**：服务商、增删改/开关、刷新/状态（UI） | R `api/xqnetwork/{ddns,get_server}`；W `{add_server,del_server,ddns_edit,server_switch,ddns_reload}` | `ddns`、`xiaoqiang.module.XQDDNS`、`/etc/init.d/ddns`、updater scripts | **B1**；显式认证字段与服务商枚举；一次只启动对应实例；不给浏览器原凭据。官方脚本已有域名 shell 引号处理，root adapter 使用 argv/明确数据合同 | 公网地址来源、provider更新结果、权威 DNS读回、失败重试/IPv6与双WAN选源；**D ddns adapter** |
| **端口转发**：单端口/区间/TCP/UDP、删除/应用（UI） | R `api/xqsystem/portforward`；W `{add_redirect,add_range_redirect,delete_redirect,redirect_apply}` | `XQFirewall` 与 firewall redirect；官方 staging/apply 有实际规则生成 | **B1/B2**；端口范围/协议/IP/LAN containment冲突检查；规则修改与 apply明确区分。fw3 reload 必须保留原厂 includes，不直接覆盖 iptables | 外部测试机→内网端口、TCP/UDP、hairpin按支持显示、IPv4/IPv6语义、删除后失效、规则 counters；**D forwarding adapter** |
| **DMZ**（UI） | R `api/xqsystem/dmz`；W `{set_dmz,dmz_off,dmz_reload}` | `XQFirewall`、firewall + LAN IP change hooks | **B2**；与普通转发/UPnP优先级一起预览；显式大范围入站暴露确认；不是“关闭 firewall” | 外部未占用端口转发、已定义转发优先级、关闭复原、目标地址变化联动；**D dmz adapter** |
| **UPnP / NAT-PMP**（UI/F） | R `api/xqsystem/upnp`；W `upnp_switch` | `upnpd`、miniupnpd、firewall include；静态实现通过 `disable_upnp/enable_upnp` 项名切换，不能假定一个 `enabled` boolean | **B1/B2**；按 LAN zone/内部接口/权限配置；使用厂商 switch+启动/停止/规则链生命周期。静态 boot 默认不自动启动，应显示真实 daemon 状态 | 自动创建/删除映射、租期、访问来源限制、reload 后动态规则/会话；NAT-PMP按实际 daemon能力测试；**D upnp adapter** |
| **高级 NAT**：虚拟服务器、端口触发、ALG（UI/C/API） | R `api/xqsystem/{get_vs_rules,get_pt_rules,get_alg_rules}`；W `{set_vs_rules,del_vs_rules,set_pt_rules,del_pt_rules,set_alg_rules}` | `XQFirewall`、协议 helper/conntrack、厂商 rules；按当前 feature与 handler开放 | **B2**；规则限额/冲突、ALG按协议、触发源/租期；统一 firewall owner，不把触发功能降级成永久 DNAT | 控制/数据连接、触发自动打开/回收、不同客户端争用、关闭 helper效果；**D advanced-NAT adapter** |
| **基础防火墙 / SPI / WAN ping / DoS 与扫描防护**（F/API） | R `api/xqsystem/{get_firewall_enable,get_spi_firewall,get_dos_firewall,get_wanping_firewall}`；W 对应 `set_*`；`api/anti_attack/{get_status,set_rpfilter,set_dos,set_scan}` 是独立 controller 路由 | `firewall` 扩展 defaults、`firewall_cpp.anti_attack`、fw3、firewall_cpp；init `anti_attack`按 NETMODE覆盖 disable | **B2**；展示 defaults、zones、generated rules和模式影响；通过原厂 fw3/includes/anti_attack生命周期。不得误以六包 firewall 单独代表最终策略 | WAN/LAN/guest/IPv6行为、有效 rule counters、rpfilter/DoS/drop统计、重启/模式切换后的状态；**D firewall/security adapter** |
| **设备访问控制 / 黑白名单 / 管理访问控制 / HTTPS / 网关安全**（UI/F） | R `api/xqnetwork/wifi_macfilter_info`、`misystem/web_access_info`、`xqsystem/get_access_force_https`、`xqnetwork/get_gw_security`；W `{set_wifi_macfilter,edit_device,manually_add}`、`misystem/web_access_opt`、`xqsystem/set_access_force_https`、`xqnetwork/set_gw_security` | `macfilter/wifiblist/wifiwlist`、`sdkfilter`/管理允许表（依实际helper）、`nginx`、`local_gw_security`；sysapi macfilter持锁更新规则 | **B2**（管理路径）/B1；防止把当前管理设备从允许表移除；HTTPS切换新入口先核验；guest/无线名单与WAN规则语义分离 | 当前管理员访问、允许/拒绝客户端、IPv4/IPv6管理端口、HTTPS会话、重启保持；**D access adapter** |
| **家庭上网保护 / parental**：家庭用户、设备归属、禁网时段、临时禁网、网址过滤、应用时长（F/API，官方拦截页） | v2 R `api/misystem/{mipctl_get_user_list,mipctl_get_device_list,mipctl_get_deny_time,mipctl_get_app_antiaddict,mipctl_get_filting_net,mipctl_get_temp_deny,mipctl_get_user_stats,mipctl_get_dpi_info}`；W `mipctl_set`；旧 `{parctl_info,parctl_add,parctl_update,parctl_delete,parctl_get_filter,parctl_set_filter,parctl_get_url,parctl_set_url,netacctl_status,netacctl_set}`按当前路径发现 | RN02 `mipctlv2=1`；`mipctl_user`、mipctld、`ubus mipctl reload`、firewall_cpp/DPI；旧 `parentalctl`、`/usr/sbin/parentalctl.sh` 生成 DNS/ipset/iptables | **B1/B2**；优先官方 v2类型化 `opt_list`，不并写旧v1。时间/时区/DPI库与用户设备映射为同域；reload和DNS缓存/规则一起核验 | 实际家庭设备在时间窗内外、临时禁网恢复、网址/应用限制、配额统计、IPv6/已有连接、系统时间变化；**D parental-v2 adapter** |
| **安全中心/恶意网址/隐私与 DPI 状态**（F/C/API） | 已有 `security`、`antiy_url_policy/antiy_url_class`、`nfnlq_userland`、`firewall_cpp`；UI/公开官方安全接口根据当前可达handler补齐，不用猜路由名 | antiy-url-fw、antiy_dpi/haohan_dpi、厂商规则数据库；`sec_center/local_gw_security`是RN02 feature | **B1/B2**；签名/策略/进程由对应厂商owner控制；查看策略来源/更新时间/启停影响；不把现代 OpenWrt防火墙替换视为等价升级 | 测试域/应用分类、实际拦截页/计数、开关回读、CAP/RE模式与重启保持；**D security-center adapter** |

## 5. 系统、维护与可扩展管理矩阵

| 官方功能与 UI 分类 | Read / Write 官方路由 | 配置 / 系统操作 | reload / commit 安全边界 | 验证与 adapter 状态 |
|---|---|---|---|---|
| **状态/终端/拓扑/测速**（UI） | `api/misystem/{status,newstatus,devicelist,topo_graph,bandwidth_test}`、`xqnetdetect` 当前测速路由 | 流量计数/trafficd、CPU/RAM/接口、QCA状态；外部服务保存历史 | **B0**；测速是耗资源异步任务，显示目标/时长/带宽影响；轮询限流，不写flash时序库 | 图表与设备/driver统计对照；测速取消/超时/错误；**D observability adapter** |
| **路由器名/时间/NTP/语言、管理员密码与登录记录**（UI/API/C） | `api/misystem/{router_name,set_router_name,sys_time,set_sys_time}`；`api/xqsystem/{set_name_password,get_login_record,clear_login_record,get_languages,set_language}` | `system/timezone/xiaoqiang/nginx`与厂商管理员凭据；root SSH账户与Web管理员密码是两个身份域 | **B1/B2**；凭据改变独立任务，自己认证与厂商认证分别更新；当前会话重新认证；不把“改Web密码”等同修改 root 密码 | 名称/时间/时区读回、NTP、parental时窗、重新登录/旧会话行为；**D system/auth adapter** |
| **LED/网口灯/温控**（UI/F/C） | `api/misystem/{led,allled,ethled,effect_light,get_temp_control,set_temp_control}` | `xqled`、厂商 thermal/LED控制；按设备实际灯效/温控feature显示 | **B1**；固定可用模式，不写原始sysfs控制以绕开thermal策略 | 读回/肉眼效果、温度/负载、自动恢复/重启保持；**D device-settings adapter** |
| **重启 / 计划重启 / 关机**（UI/C） | `api/xqsystem/{reboot,shutdown}`；计划功能依当前官方handler或受控system schedule发现 | procd/system +厂商 reboot helpers；关机静态feature为0，隐藏而非假按钮 | **B3**；先flush管理job/配置，单任务，提供重新连接窗口；计划与时区有类型管理 | 离线→开机→管理入口恢复、原厂业务服务健康、root代理启动与版本；**D lifecycle adapter** |
| **配置备份 / 下载 / 上传 / 选择项恢复**（UI） | `api/misystem/{c_backup,c_download,c_upload,c_restore}` | `XQBackup` 的固定 getter/restorer key；不等同 sysupgrade全量文件备份 | **B0 下载 / B3 恢复**；备份保存到外部加密存储；导入限制总大小/展开大小、成员规范化/类型/超时、key白名单；恢复前做版本与拓扑预览 | key集合、可恢复元数据、恢复后配置+runtime/新LAN地址、重启保持；**D backup/restore adapter** |
| **恢复出厂**（UI） | `api/xqsystem/reset`，厂商 `restore_defaults.sh` | NVRAM恢复标志、overlay/data配置重建 | **B4**；明确将清空配置/root代理状态的真实范围；先导出恢复点；任务不能跟普通保存捆绑 | 首次初始化UI/网络/默认地址、重新绑定/部署流程、恢复后根管理策略；**D reset adapter** |
| **OTA/手工官方 ROM 上传/升级进度/自动OTA设置**（UI） | R `api/xqsystem/{check_rom_update,ota,upgrade_status}`；W `{set_ota,upload_rom,flash_rom,upgrade_rom}` | `XQSysUtil.verifyImage`、`mkxqimage -v`、条件 secboot wrapper、`flash.sh`→boardupgrade/libupgrade；A/B和配置迁移 | **B4**；用厂商签名/型号/版本校验链，不改成 generic sysupgrade/force。独立job/升级互斥、电源/空间检查、配置/代理恢复manifest；跨OTA重新探测全部schema与root入口 | 上传校验/拒绝错误型号或损坏包、进度、重启版本、boot健康、配置迁移、原厂网络/无线/加速、代理恢复；**D ota adapter** |
| **日志/诊断下载/官方上传开关**（UI/API） | `api/misystem/sys_log`、`api/xqsystem/upload_log`；`api/mi_log/{get_onoff,set_onoff,overview,get_logs,del_logs}`按当前feature；syslog-ng/logread/诊断公开路径 | `/tmp`日志、milog、stat_points、MiIO/messaging；各机制分别有owner和门控 | **B0 本地读取 / B1 开关**；日志默认易失/外部采集；显式区分“本地下载”与“发送厂商”；诊断包可含秘密，仅认证后下载，输出不写会话/凭据 | 产生日志/分页/容量/撤销采集；各上传机制实际状态独立显示，不用milog单开关代表全部；**D logs/diagnostics adapter** |
| **SSH/root服务管理**（专业管理，root已完成） | 当前 root入口的service/绑定/key状态；`dropbear`有类型管理；原厂Web没有等价的已启用root开关 | dropbear当前启动路径与自身代理；static release init仍有双gate，不再按“改UCI就自动SSH”处理 | **B2/B3**；先确认备用LAN会话与入口，新增配置健康后再替换旧入口；持久性按当前挂载/启动方案验证，不依赖RAMFS `rc.local/init.d`文件单改 | 新旧会话、LAN绑定、重启/断电冷启、OTA后入口；**D maintenance-access adapter** |
| **存储/USB/Samba/swap/Docker/NFC/SFP**（通用模板C） | 模板包含 storage/docker/NFC/SFP API；仅在当前实际设备/feature/外设发现后纳入页面 | 用户态服务、挂载、可用内核/外设、实际物理接口 | B1–B3依操作；USB feature文字或generic模板不是实物端口证据；不安装不匹配依赖来补“按钮” | 实际设备枚举/挂载/空间、可用服务/卸载、资源预算；属于能力扩展发现而非RN02默认必有功能 |

## 6. OpenWrt 专业管理 parity：在原厂底座上补齐

| 专业页面 | 现有底座与新增 typed adapter 最小合同 | 应用/验证要点 |
|---|---|---|
| 网络总览/接口/地址/静态路由/IPv4-IPv6规则 | netifd/UCI、ip-full、多路由表；network adapter提供 interface/route/route6/rule/rule6 schema，保留未知vendor字段 | 图示关联 bridge/zone/physical port；路由冲突/下一跳可达/管理路径影响；netifd读回与真实FIB对照 |
| VLAN/桥/端口/链路诊断 | port_service/QCA8386/PHY与现有网络字段；ports adapter返回实际能力图并拥有其服务写域 | 不套用 generic DSA/swconfig schema；错误/丢包/速率/LAG/IPTV按driver读回 |
| PBR / 多WAN / 按设备与目标出口 | mwan3、现有 ip rule、路由表、ipset；policy adapter规划 table/priority/mark、flow invalidation | 既有 mark：QoS `0xffff8000`、mwan3 `0x3f00`、parentctrl `0xf`、UU plugin `0xf0`；新策略先做冲突预算；测试IPv4/IPv6、新旧连接/DNS/隧道 |
| 防火墙专业编辑 | fw3/UCI typed zone/forwarding/rule/redirect；有效规则浏览器展示vendor includes/生成来源 | 一个规则owner，保留 sysapi/port_service/ECM插件；不要把nftables存在视为运行firewall4，也不直接替换firewall4 |
| 高级DNS/DHCP | dnsmasq现有能力、静态host/域名转发/option与odhcpd | 有类型的DNS option/protocol schema，作用域清楚；配置预检、租约/DNS运行核验 |
| 无线专业状态/射频/站点 | QCA unified driver/vendor hostapd，radio/BSS/MLD/guest/miot/mesh实际映射 | driver公布哪些字段就开放哪些字段；信道/带宽/发射功率使用区域与校准允许集合，保留原厂固件 |
| QoS/整形/offload诊断 | tc-full，HTB/PRIO/SFQ/fq_codel/ingress、miqos、ECM前端选择 | 先做原厂QoS完整管理；自定义qdisc另建单一owner并显式替代miqos。该kernel没有CAKE；需要CAKE应单独走精确kernel构建/模块验证工程，不当作普通软件包按钮 |
| VPN与用户态代理/隧道扩展 | 当前PPTP/L2TP/TUN能力；新服务以匹配ARM EABI/armhf、musl的独立用户态程序接入 | service adapter声明binary/依赖/端口/配置路径/资源上限/健康与移除计划；需要新kernel模块者独立评估，先不破坏无线/offload |
| 进程/服务/定时任务/日志/资源 | 当前procd/init/cron、UBI/mount、syslog-ng；system adapter仅开放已发现的固定服务action | restart/reload/enable/disable语义分别显示；临时与持久路径区别；数据库与历史图表移外部，控制flash写频率 |
| 配置历史/漂移/变更差异/恢复 | 全域配置读取+typed intent+实际规则/状态快照 | 显示stock UI或daemon改写的drift；恢复的是各adapter拥有的整组字段和生命周期，不回写整份未经辨识运行文件 |
| 软件与内核扩展审查 | 原厂18.06/QSDK32、ARM32 musl、kernel `5.4.213` 的精确构建依赖 | 先只读包/ELF/依赖/空间视图；新增用户态隔离部署。kmod匹配厂商配置、补丁、symbols/vermagic和联动模块，通用OpenWrt kmod不作为直接安装列表 |

### 6.1 硬件潜力由哪些 owner 释放

| 硬件/路径 | 管理策略 |
|---|---|
| QCA5332 / QCN9224、Wi-Fi7 MLO/TWT/EMESH、QCA wireless firmware | 保留 vendor driver/firmware 与校准数据；增加MLO/射频/站点/回程专用UI与adapter，利用已有driver操作接口 |
| QCA8386、PHY、动态WAN/LAN口、IPTV/LAG | 保留 `port_service` 与交换/PHY驱动；port topology能力图支配配置，不能凭通用UCI文件替代它 |
| PPE/SFE/ECM premium / PPE VP/DS | 保留匹配QSDK模块组合；引擎实际发现，QoS/PBR/VPN事件走厂商accel hooks；吞吐/延迟/CPU/命中一起看 |
| CPU/RAM/NAND | 展示可用资源和温控策略，优先轻量用户态扩展；OPP中的1.5GHz需实际speed-bin/thermal能力，不能将其作为普通“root解锁”开关 |
| 启动/升级/校准分区 | 日常fullcontrol管理不写bootloader/校准/原始MTD；官方OTA保持签名链。低层启动链研究单独工程，和日常管理plane分离 |

## 7. 最小 generic adapter 边界

不要先做一个能执行任意 shell 或任意 HTTP 路由的大框架。保留少量共同原语，把业务完整性留给专用 adapter。

### 7.1 通用层只负责六件事

1. **Transport/auth**：现有LAN root通道与可选厂商API通道；自身会话鉴权/超时；固定动作/schema；秘密只在后端。厂商会话过期可重新认证，不把token/密码写入响应、URL历史或日志。
2. **Capabilities/read**：当前身份/模式、实际对象/schema、资源映射、只读字段投影；每条数据有证据来源/时间。
3. **Config transaction**：有类型intent与expected revision、设备级写锁、影响域恢复点、差异、未知字段保留、CAS/漂移检测。只允许当前adapter注册的配置域与字段，不以扩大六包whitelist替代建模。
4. **Apply job**：`planned → checkpointed → applying → verifying → awaiting-confirmation → committed`；失败进入 `restoring → restored/needs-recovery`。知道需要何种reload、网络中断、reboot；后台执行，不绑浏览器连接。
5. **Typed service/action executor**：已发现且adapter注册的固定服务/操作，argv或ubus JSON参数；shell模板/路径/接口名字不直接来自用户。按依赖次序执行；重启/OTA/复位是独立维护job。
6. **Verification/audit**：配置读回 + 业务状态 + 对自身连接的健康核验；操作记录只有公共字段、变化摘要和结果。恢复动作也走相同adapter与生命周期。

### 7.2 专用层需要的最小合同

```text
id / feature predicate / read schema
owned config domains + fields + related generated state
validate(intent, current capabilities)
plan(diff): checkpoint scope, dependency order, impact, reload/actions
apply(plan): vendor API 或 root typed implementation（二选一 owner）
verify(plan): config + runtime + functional checks
restore(checkpoint): domain-aware recovery + matching lifecycle
```

- `vendor API` 是首选 parity复用方式：有成熟 setter就调用其已知、鉴权、类型化接口，不把厂商内部 helper 的副作用忽略掉。
- `root typed implementation` 用于补UI缺口、避开旧handler的输入边界问题、提供可控回退；需要复制**业务语义和生命周期**，不是复制含shell字符串的旧实现。
- 同一域不同时走两条写通道。stock UI、厂商 daemon仍能写配置时，应用前后检查 revision/drift，避免悄悄覆盖用户刚在官方面板做的修改。
- 原始UCI专家编辑可以保留，但应显示所属owner/影响域，允许adapter给出正确reload/验证计划；用户越出schema后不能自动记为该官方功能已成功配置。

### 7.3 安全 commit 的实际边界

| 变更类型 | 必须的处理 |
|---|---|
| 普通UCI staged变更 | 写入独立staging；验证后提交受影响包，不做全局`uci commit`；随后按计划reload和核验 |
| 厂商自动commit setter | 应用前对全部影响域checkpoint；执行后立即读回其副作用；若失败由adapter恢复这些域并执行对应reload，不能仅返回HTTP失败 |
| LAN/管理ACL/无线/port等断连风险 | 后台维护job + 独立超时恢复机制；管理健康与客户端显式confirm；当前rpcd是否有apply/confirm/rollback能力先发现，不能按现代OpenWrt教程假定 |
| 官方LAN setter直接重启/模式转换 | 不装作无中断临时apply；改前展示新地址/重连计划与恢复路径。恢复任务须能在重启后继续，必要时用户明确选择有线维护入口 |
| QoS/PBR/firewall/VPN | 同一个域锁覆盖mark/qdisc/ECM conncache；reload与connection invalidation编排，检测fast path与慢路径一致 |
| 备份恢复/OTA/出厂重置 | 独立维护队列，阻止其他写操作；元数据/签名/型号/版本/空间校验；完成后重新发现capability与验证全部关键服务 |

## 8. Root phase 拆分与交付顺序

| Phase | 目标与明确交付 | 完成验证 |
|---|---|---|
| **R0 已完成：root admission** | 当前ROM 1.0.43 + LAN root；记录当前入口归属与备用维护路径。不重做解锁研究 | 父任务现场事实；本子任务没有连接设备 |
| **R1 当前状态面与恢复基础** | 完成§1 read matrix；capability registry；公开资源图；配置域checkpoint/CAS；固定后台job/验证/恢复机制；明确持久目录和启动机制 | 只读结果与官方UI对照；独立恢复作业能在浏览器断开后继续；普通重启/冷启动分别核验 |
| **R2 官方日常基础parity** | typed WAN/LAN/DHCP/static lease/DNS/IPv6/NAT-DMZ/UPnP/DDNS/system-auth/logs adapters；不是再加一层raw UCI表单 | 各adapter双向读写、读回/functional验证、恢复；LAN/ACL断连场景单独演练 |
| **R3 RN02厂商能力全覆盖** | Wi-Fi7/MLO/TWT/guest/miOT/Mesh；ports/IPTV/VLAN/LAG/game/dualWAN；QoS+ECM；VPN；firewall/security/parentalv2 | driver与配置状态一致、真实终端/出口/隔离测试、Mesh同步、fast-path一致；逐adapter交付，不一次大改底座 |
| **R4 OpenWrt专业管理** | routes/rules/PBR、专业DNS、防火墙有效规则、offload/qdisc诊断、资源/服务/定时任务、独立用户态扩展；参数受当前底座capabilities约束 | 每项有owned domain与恢复合同；吞吐/延迟/CPU/稳定性与原厂基线对照 |
| **R5 生命周期生产化** | 官方备份restore/OTA/reset维护工作流；外部restore bundle；代理重启/冷启动/OTA/复位边界；升级后schema重发现 | 普通重启、冷启动、官方OTA、恢复出厂四项分别记录；OTA后原厂无线/端口/加速与管理恢复验收 |

R1恢复基础应先落地；R2与R3可以按独立配置域并行开发，但同设备写测串行。完整 parity验收用本矩阵逐行打勾：`discover/read/write/verify/restore/lifecycle`。`D` 行只有补齐专用adapter与实际验证后才计入fullcontrol功能完成率。

## 9. 本地证据索引与几个必须修正的别名

### 9.1 足够完成首轮映射的证据

- 四份主报告：`static/api-review.md`（API/auth与输入边界）、`boot-services-review.md`（服务/init/OTA/模式门控）、`control-options.md`（QSDK、QoS/mark/offload、ABI与持久化）、`platform-review.md`（RN02/ARM32/QCA硬件/ROM结构）。
- 官方Web：`/usr/lib/lua/luci/view/web/setting/{wan,lannetset,dhcp_ip_mac,wifi,qos,ddns,vpn,upnp,nat_dmz,iptv,safe,upgrade,upgrade_manual}.htm`。
- 相关UI include：`web/inc/{ipv6.js,dual-wan.js,guestwifi,meshbhmode.js,accesscontrol.js,sysinfo.js,natpro.js,port_custom.js,game_port,lan_lag,reboot.js,netmod.js}.htm`；`/usr/lib/lua/luci/view/mipctl/home.htm`显示官方“家人上网保护”阻断原因。
- RN02 feature：`/usr/lib/lua/xiaoqiang/XQFeatures.lua`及`/etc/config/misc`；派生源码只导航字段/函数名，复杂分支按原字节码/当前调用结果校验。
- API表：`static/api-routes.json`是14个API controller的623个静态entry，包含条件注册；独立`anti_attack` controller不在这14个表内。
- 生命周期：`/etc/init.d/{network,dnsmasq,odhcpd,port_service,miqos,mipctld,firewall,firewall_cpp,miniupnpd,ddns}`、`/usr/sbin/{sysapi,sysapi.firewall,ipv6.sh,parentalctl.sh}`、`/lib/miwifi/arch/lib_arch_accel.sh`与`/etc/config/firewall`includes。

### 9.2 不直接照抄旧模板API别名

1. `inc/dual-wan.js.htm`引用 `api/xqnetwork/set_weight`；路由清单实际注册 **`api/xqnetwork/set_multiwan_weight`**。adapter绑定已注册route并现场发现其可用性。
2. 旧 `setting/nat.htm` / `dmz.htm`引用 `xqnetwork` NAT/DMZ路由；该清单未注册这些名称。`nat_dmz.htm` 与注册表一致使用 **`api/xqsystem/{portforward,add_redirect,add_range_redirect,delete_redirect,redirect_apply,dmz,set_dmz,dmz_off,dmz_reload}`**。
3. `inc/speedtest.js.htm`存在 `xqnetdetect/netupspeed`引用，但该14-controller清单没有此项；测速route由当前registry/官方页面响应发现，不把这个名称写成已可用接口。
4. RN02 `ipv6_wired_v2=1`、`mipctlv2=1`、`multiwan/mlo/twt/wifiguest=1`，优先v2与RN02当前feature。通用模板中的SFP/NFC/Docker/磁盘/第三频段不自动扩大本机物理能力。
