# Wireless 字段静态使用追踪

## 范围与输出

- 实现：`web/src/components/configuration/field-help/wireless.ts`，导出 `wirelessFieldHelp`。
- 完整覆盖 `field-schema.ts` 的 wireless 库存：`wifi-device` 15 项、`wifi-iface` 32 项，共 47 项。
- 每项独立说明用途、依赖与服务影响，并提供有具体事实的证据。
- 基线：`/Volumes/RN02_STATIC/rootfs` 的 **Xiaomi RN02 1.0.43**。
- Lua：研究目录的 `static/lua-analysis/decompiled`。引用的是反编译源行，不是原始字节码行。
- 父任务提供的 1.0.64 公共脚本比较集合目前不含无线消费者。此文件未据此声称 Wi-Fi 行为已在 1.0.64 验证。
- 未进行 SSH、探测、修改运行设备或重启。未读取/复制私有 Wi-Fi 密码或配置样本值。所有缺省来自源码分支，不来自当前设置。

## 消费链

1. `etc/init.d/qcawifi-config-cmd:21–29` 调用 `/sbin/wifi config`，生成缺失无线配置。
2. `sbin/wifi:212–224` 按 `wifi-device.type` 分派厂商扫描与启停。
3. `lib/wifi_interface_helper.sh:94–98` 将 iface 关联到对应 radio；`:10–21` 和 `:46–51` 关联逻辑网络。
4. `lib/wifi/qcawificfg80211.sh` 定位 `phy/macaddr`，建 VAP，下发 cfg80211tool/iw/wlanconfig；不应将通用 OpenWrt 名称视为已接入厂商。
5. `lib/wifi/hostapd.sh` 和 `lib/wifi/wpa_supplicant.sh` 生成 AP/STA 认证配置。
6. `etc/init.d/qca-hostapd:56–65` 运行 hostapd 全局实例。消费者生成行为不等于验证所有芯片能力。
7. `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQWifiUtil.lua` 另有厂商 SAE、MLO 和 TWT 参数同步。

## 关键厂商差异

| 字段 | 已证实事实 | 解释边界 |
| --- | --- | --- |
| `band` | QCA 使用数值比较；3 用于 6 GHz 分支 | 通用 2g/5g/6g 不保证可互换，不声称 RN02 硬件增加频段 |
| `phy` / `macaddr` | 按 `/sys/class/net/wifi*` 与地址定位 | 不是通用路径匹配，也不是设置每个 BSSID |
| `htmode` | 读取缺省 auto，随后被厂商 `bw`/channel/硬件宽度重算 | 不保证下拉所有通用模式直接生效 |
| `hwmode` | 驱动映射包括 11ax/11be；`ax=0` 改为 11ng/11ac | 并非仅用于旧标准 |
| `country` | 数值/代码分别调用 setCountryID/setCountry；缺失且有 AP 类 VAP 时下发 156 | 保留条件，不搬用文件上方已过时的 US 注释 |
| `txpower` | 被 `txpwr` 档位和最大功率重算；CN/156 有条件下发 | 不把最大功率缺省当成此字段缺省，不声称固定 0–40 |
| `distance` | 明确报告此驱动不支持 | 不承诺可调整 ACK 超时 |
| `mode=mesh` | 创建为 `__ap` | 不把 Xiaomi Mesh 等同于普通 802.11s |
| `encryption` | 厂商 WPA3 参数还需 `sae`、`sae_password`、PMF 等 | 通用字符串不保证能单独替代整套厂商方案 |
| `wmm` | QCA 可下发 wmm；hostapd BSS 固定写 wmm_enabled=1 | 不承诺单关该字段关闭宣告 |
| `macfilter` | allow/deny 下发第二套 ACL；其他值+非空 maclist 仍下发拒绝 | disable 不保证关闭；回程 AP 被排除 |
| `maclist` | 非空时清空后逐项写第二 ACL | 空值在此分支不执行清空，不承诺清除旧状态 |
| `acct_port` | 外层非空守卫包裹 `${acct_port:-1813}` | 不能据内层表达式声称空值默认 1813 |
| `dtim_period` | 普通 AP 缺省 1；厂商 ap_lp_iot 为 41 | 保留模式条件 |

## MLO 与 TWT

- `XQWifiUtil.lua:11935–11961` 关联接口 `mld` 并写 `mlo_enable`。
- `qcawificfg80211.sh:3932–3938` 按 MLO 链路 `mode` 选择客户端类型；`:7839–7849` 根据 MLD 关联创建接口。
- `qcawificfg80211.sh:4398–4455` 检查 MLO STA 的 SSID、加密、密钥、PMF 等一致性。
- `qcawificfg80211.sh:6436–6461` 将信道、认证和 TWT 等变化纳入更新处理，且含伙伴 MLO 链路协调。
- `qcawificfg80211.sh:9785–9786` 从独立 `twt_responder` 下发响应模式。
- `XQWifiUtil.lua:12386–12411` 写入接口 `twt_responder`；读取接口 `get_twt_hostap` 的回退与驱动启动回退并不相同。
- 因此 `channel`、`htmode`、`mode`、`encryption` 帮助明确说明相关原厂依赖，而不声称某信道/带宽/加密值可自动开启 MLO/TWT。

## 未接入或未证实的字段

静态搜索覆盖 rootfs 中可读的 `lib`、`etc/init.d`、`etc/hotplug.d`、`sbin`、`usr/sbin`、`usr/lib/lua`，排除模块/固件二进制与私有配置；另搜索全部 235 份反编译 Lua。重点逐行跟踪无线消费者。对通用语义与厂商消费分别说明，没有把其他字段的存在冒充缺失字段生效。

| 字段 | 具体发现 | 不确定性 |
| --- | --- | --- |
| `path` | QCA 定位读取 phy/macaddr | 未找到读取 wifi-device.path |
| `legacy_rates` | 公共 basic_rate/basic_rates；厂商 dis_legacy | 未找到 legacy_rates 或两者间转换 |
| `noscan` | netifd 公共声明包含该字符串；QCA 读 disablecoext | 只有声明，无 noscan 到驱动的消费映射 |
| `beacon_int` | 公共生成器支持；主 VAP 路径读 iface.bintval | 未证实设备级字段接入主路径 |
| `ieee80211k` | 厂商 rrm 命令；hostapd ELF 有 rrm_neighbor_report | 无此字段到任一入口映射 |
| `maxassoc` | 厂商 maxsta/misc 上限；hostapd ELF 有 max_num_sta | 无 maxassoc 到任一入口映射 |
| `mesh_id` | 厂商 mesh_id 命令从 NETWORK_ID 取整数标识 | 无 wifi-iface.mesh_id 读取，通用 802.11s 与厂商整数标识不同 |
| `mesh_fwding` | supplicant ELF 有 mesh_fwding；厂商 mesh 建为 AP | 无 UCI 字段到 supplicant/驱动的传递 |

另外，`distance` 是已验证不支持，故不是单纯“没搜到”。

## ELF 补充发现的证据级别

采用静态 ASCII 字符串扫描，不运行目标架构二进制。以下为基线文件字节偏移，提供可复查边界，**不能证明 UCI 转换或运行效果**：

- `usr/sbin/hostapd`：`max_num_sta` 偏移 2115836，`beacon_int` 偏移 2117952，`rrm_neighbor_report` 偏移 2125104。
- `usr/sbin/wpa_supplicant`：`mesh_fwding` 偏移 2850412。
- `legacy_rates`、`ieee80211k`、`maxassoc` 没有在已查消费者中找到同名 UCI 消费。

目录证据仍指向有意义的脚本入口，以说明实际使用的对应字段；缺失结论独立写在 discovery 中，不伪造“这一行证明整个固件没有该字段”。

## 局部核验

- 库存逐项比对：47/47，无多余字段。
- 124 个行号范围均存在于相应基线/反编译文件。
- 7 个密码字段全部标记 credential，全部不含 defaultValue。
- 8 个 discovery 均明确写出具体缺失或尚未接入的字段。
- 冲突的原有简短提示用 summary 修正；未修改 widget、选项或 min/max。
- 单文件 Prettier 格式化；父任务执行总体类型、库存及 UI 测试。
