# RN02 网络字段静态证据

## 范围与方法

- 基线：Xiaomi RN02 1.0.43；仅离线检查 /Volumes/RN02_STATIC/rootfs。
- 网络启动链：etc/init.d/network → /sbin/netifd；交换芯片由 lib/network/switch.sh → swconfig dev <name> load network。
- netifd 为无 section header 的 ARM32 little-endian PIE。使用研究目录 tools/binary-env 的 pyelftools 读取 PT_LOAD，然后扫描 8 字节 `<uint32 name_ptr, uint32 type>` 表项，并解析指针指向的 NUL 结尾字符串。
- type 值：1=array、2=table、3=string、5=int32、7=bool/int8；与 lib/netifd/utils.sh 的 JSON 类型声明一致。
- netifd SHA256：`7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d`。
- 下表组名由连续表布局及相邻对象推定；表项证明字段名与数据类型，不单独证明内核处理、回退值或硬件支持。
- 文本消费者搜索覆盖 etc/init.d、lib、sbin、usr/sbin、usr/share 的可读文件，以及 static/lua-analysis/decompiled 中全部可读 Lua；未读取私有配置值。
- 对缺少文本实现的核心字段，逐项列明剩余不确定性；未以其他字段或同名无线参数冒充证据。

## netifd 参数表

| 参数组      | 字段                              | 类型          | 文件偏移  | 虚拟地址  | 名称指针  |
| ----------- | --------------------------------- | ------------- | --------- | --------- | --------- |
| interface   | `device`                          | string (3)    | `0x2a248` | `0x3a248` | `0x27411` |
| interface   | `ifname`                          | string (3)    | `0x2a250` | `0x3a250` | `0x27b66` |
| interface   | `proto`                           | string (3)    | `0x2a258` | `0x3a258` | `0x278c4` |
| interface   | `auto`                            | bool/int8 (7) | `0x2a260` | `0x3a260` | `0x25dec` |
| interface   | `jail`                            | string (3)    | `0x2a268` | `0x3a268` | `0x26264` |
| interface   | `jail_ifname`                     | string (3)    | `0x2a270` | `0x3a270` | `0x26269` |
| interface   | `defaultroute`                    | bool/int8 (7) | `0x2a278` | `0x3a278` | `0x26275` |
| interface   | `peerdns`                         | bool/int8 (7) | `0x2a280` | `0x3a280` | `0x26282` |
| interface   | `dns`                             | array (1)     | `0x2a288` | `0x3a288` | `0x26286` |
| interface   | `dns_search`                      | array (1)     | `0x2a290` | `0x3a290` | `0x2628a` |
| interface   | `dns_metric`                      | int32 (5)     | `0x2a298` | `0x3a298` | `0x26295` |
| interface   | `metric`                          | int32 (5)     | `0x2a2a0` | `0x3a2a0` | `0x26299` |
| interface   | `interface`                       | string (3)    | `0x2a2a8` | `0x3a2a8` | `0x277ef` |
| interface   | `ip6assign`                       | int32 (5)     | `0x2a2b0` | `0x3a2b0` | `0x262a0` |
| interface   | `ip6hint`                         | string (3)    | `0x2a2b8` | `0x3a2b8` | `0x262aa` |
| interface   | `ip4table`                        | string (3)    | `0x2a2c0` | `0x3a2c0` | `0x262b2` |
| interface   | `ip6table`                        | string (3)    | `0x2a2c8` | `0x3a2c8` | `0x262bb` |
| interface   | `ip6class`                        | array (1)     | `0x2a2d0` | `0x3a2d0` | `0x262c4` |
| interface   | `delegate`                        | bool/int8 (7) | `0x2a2d8` | `0x3a2d8` | `0x262cd` |
| interface   | `ip6ifaceid`                      | string (3)    | `0x2a2e0` | `0x3a2e0` | `0x262d6` |
| interface   | `force_link`                      | bool/int8 (7) | `0x2a2e8` | `0x3a2e8` | `0x262e1` |
| interface   | `ip6weight`                       | int32 (5)     | `0x2a2f0` | `0x3a2f0` | `0x262ec` |
| route       | `interface`                       | string (3)    | `0x2a334` | `0x3a334` | `0x277ef` |
| route       | `target`                          | string (3)    | `0x2a33c` | `0x3a33c` | `0x266a2` |
| route       | `netmask`                         | string (3)    | `0x2a344` | `0x3a344` | `0x266a9` |
| route       | `gateway`                         | string (3)    | `0x2a34c` | `0x3a34c` | `0x266b1` |
| route       | `metric`                          | int32 (5)     | `0x2a354` | `0x3a354` | `0x26299` |
| route       | `mtu`                             | int32 (5)     | `0x2a35c` | `0x3a35c` | `0x28fa8` |
| route       | `valid`                           | int32 (5)     | `0x2a364` | `0x3a364` | `0x266b9` |
| route       | `table`                           | string (3)    | `0x2a36c` | `0x3a36c` | `0x262b5` |
| route       | `source`                          | string (3)    | `0x2a374` | `0x3a374` | `0x266bf` |
| route       | `onlink`                          | bool/int8 (7) | `0x2a37c` | `0x3a37c` | `0x266c6` |
| route       | `type`                            | string (3)    | `0x2a384` | `0x3a384` | `0x266cd` |
| route       | `proto`                           | string (3)    | `0x2a38c` | `0x3a38c` | `0x278c4` |
| route       | `disabled`                        | bool/int8 (7) | `0x2a394` | `0x3a394` | `0x26014` |
| rule        | `in`                              | string (3)    | `0x2a3e4` | `0x3a3e4` | `0x28f65` |
| rule        | `out`                             | string (3)    | `0x2a3ec` | `0x3a3ec` | `0x2695a` |
| rule        | `invert`                          | bool/int8 (7) | `0x2a3f4` | `0x3a3f4` | `0x2695e` |
| rule        | `src`                             | string (3)    | `0x2a3fc` | `0x3a3fc` | `0x26965` |
| rule        | `dest`                            | string (3)    | `0x2a404` | `0x3a404` | `0x26969` |
| rule        | `priority`                        | int32 (5)     | `0x2a40c` | `0x3a40c` | `0x28d03` |
| rule        | `tos`                             | int32 (5)     | `0x2a414` | `0x3a414` | `0x28f5b` |
| rule        | `mark`                            | string (3)    | `0x2a41c` | `0x3a41c` | `0x2696e` |
| rule        | `lookup`                          | string (3)    | `0x2a424` | `0x3a424` | `0x26973` |
| rule        | `action`                          | string (3)    | `0x2a42c` | `0x3a42c` | `0x2697a` |
| rule        | `goto`                            | int32 (5)     | `0x2a434` | `0x3a434` | `0x26981` |
| rule        | `suppress_prefixlength`           | int32 (5)     | `0x2a43c` | `0x3a43c` | `0x26986` |
| static      | `ipaddr`                          | array (1)     | `0x2a498` | `0x3a498` | `0x26697` |
| static      | `ip6addr`                         | array (1)     | `0x2a4a0` | `0x3a4a0` | `0x26abc` |
| static      | `netmask`                         | string (3)    | `0x2a4a8` | `0x3a4a8` | `0x266a9` |
| static      | `broadcast`                       | string (3)    | `0x2a4b0` | `0x3a4b0` | `0x26a94` |
| static      | `ptpaddr`                         | string (3)    | `0x2a4b8` | `0x3a4b8` | `0x26ac4` |
| static      | `gateway`                         | string (3)    | `0x2a4c0` | `0x3a4c0` | `0x266b1` |
| static      | `ip6gw`                           | string (3)    | `0x2a4c8` | `0x3a4c8` | `0x26acc` |
| static      | `ip6prefix`                       | array (1)     | `0x2a4d0` | `0x3a4d0` | `0x26ad2` |
| static      | `ip6deprecated`                   | bool/int8 (7) | `0x2a4d8` | `0x3a4d8` | `0x26adc` |
| bridge-vlan | `vlan`                            | int32 (5)     | `0x2a5f0` | `0x3a5f0` | `0x26d7a` |
| bridge-vlan | `local`                           | bool/int8 (7) | `0x2a5f8` | `0x3a5f8` | `0x2901b` |
| bridge-vlan | `ports`                           | array (1)     | `0x2a600` | `0x3a600` | `0x26d60` |
| bridge-vlan | `alias`                           | array (1)     | `0x2a608` | `0x3a608` | `0x275c8` |
| device      | `type`                            | string (3)    | `0x2a610` | `0x3a610` | `0x266cd` |
| device      | `mtu`                             | int32 (5)     | `0x2a618` | `0x3a618` | `0x28fa8` |
| device      | `mtu6`                            | int32 (5)     | `0x2a620` | `0x3a620` | `0x272f8` |
| device      | `macaddr`                         | string (3)    | `0x2a628` | `0x3a628` | `0x2757c` |
| device      | `txqueuelen`                      | int32 (5)     | `0x2a630` | `0x3a630` | `0x272fd` |
| device      | `enabled`                         | bool/int8 (7) | `0x2a638` | `0x3a638` | `0x28fcc` |
| device      | `ipv6`                            | bool/int8 (7) | `0x2a640` | `0x3a640` | `0x2897d` |
| device      | `promisc`                         | bool/int8 (7) | `0x2a648` | `0x3a648` | `0x2731a` |
| device      | `rpfilter`                        | string (3)    | `0x2a650` | `0x3a650` | `0x27322` |
| device      | `acceptlocal`                     | bool/int8 (7) | `0x2a658` | `0x3a658` | `0x2732b` |
| device      | `igmpversion`                     | int32 (5)     | `0x2a660` | `0x3a660` | `0x27337` |
| device      | `mldversion`                      | int32 (5)     | `0x2a668` | `0x3a668` | `0x27343` |
| device      | `neighreachabletime`              | int32 (5)     | `0x2a670` | `0x3a670` | `0x27418` |
| device      | `dadtransmits`                    | int32 (5)     | `0x2a678` | `0x3a678` | `0x273a9` |
| device      | `multicast_to_unicast`            | bool/int8 (7) | `0x2a680` | `0x3a680` | `0x28790` |
| device      | `multicast_router`                | int32 (5)     | `0x2a688` | `0x3a688` | `0x28a97` |
| device      | `multicast_fast_leave`            | bool/int8 (7) | `0x2a690` | `0x3a690` | `0x2880e` |
| device      | `multicast`                       | bool/int8 (7) | `0x2a698` | `0x3a698` | `0x273da` |
| device      | `learning`                        | bool/int8 (7) | `0x2a6a0` | `0x3a6a0` | `0x2883c` |
| device      | `unicast_flood`                   | bool/int8 (7) | `0x2a6a8` | `0x3a6a8` | `0x2885e` |
| device      | `neighgcstaletime`                | int32 (5)     | `0x2a6b0` | `0x3a6b0` | `0x2742b` |
| device      | `sendredirects`                   | bool/int8 (7) | `0x2a6b8` | `0x3a6b8` | `0x273b6` |
| device      | `neighlocktime`                   | int32 (5)     | `0x2a6c0` | `0x3a6c0` | `0x2743c` |
| device      | `isolate`                         | bool/int8 (7) | `0x2a6c8` | `0x3a6c8` | `0x2744a` |
| device      | `ip6segmentrouting`               | bool/int8 (7) | `0x2a6d0` | `0x3a6d0` | `0x27308` |
| device      | `drop_v4_unicast_in_l2_multicast` | bool/int8 (7) | `0x2a6d8` | `0x3a6d8` | `0x273c4` |
| device      | `drop_v6_unicast_in_l2_multicast` | bool/int8 (7) | `0x2a6e0` | `0x3a6e0` | `0x273e4` |
| device      | `drop_gratuitous_arp`             | bool/int8 (7) | `0x2a6e8` | `0x3a6e8` | `0x29239` |
| device      | `drop_unsolicited_na`             | bool/int8 (7) | `0x2a6f0` | `0x3a6f0` | `0x29268` |
| device      | `arp_accept`                      | bool/int8 (7) | `0x2a6f8` | `0x3a6f8` | `0x29297` |
| device      | `auth`                            | bool/int8 (7) | `0x2a700` | `0x3a700` | `0x27404` |
| bridge      | `ports`                           | array (1)     | `0x2a71c` | `0x3a71c` | `0x26d60` |
| bridge      | `stp`                             | bool/int8 (7) | `0x2a724` | `0x3a724` | `0x2750a` |
| bridge      | `forward_delay`                   | int32 (5)     | `0x2a72c` | `0x3a72c` | `0x289d6` |
| bridge      | `priority`                        | int32 (5)     | `0x2a734` | `0x3a734` | `0x28d03` |
| bridge      | `igmp_snooping`                   | bool/int8 (7) | `0x2a73c` | `0x3a73c` | `0x2750e` |
| bridge      | `ageing_time`                     | int32 (5)     | `0x2a744` | `0x3a744` | `0x28d2f` |
| bridge      | `hello_time`                      | int32 (5)     | `0x2a74c` | `0x3a74c` | `0x28d5e` |
| bridge      | `max_age`                         | int32 (5)     | `0x2a754` | `0x3a754` | `0x28d8c` |
| bridge      | `bridge_empty`                    | bool/int8 (7) | `0x2a75c` | `0x3a75c` | `0x2751c` |
| bridge      | `multicast_querier`               | bool/int8 (7) | `0x2a764` | `0x3a764` | `0x28a3d` |
| bridge      | `hash_max`                        | int32 (5)     | `0x2a76c` | `0x3a76c` | `0x28a75` |
| bridge      | `robustness`                      | int32 (5)     | `0x2a774` | `0x3a774` | `0x27529` |
| bridge      | `query_interval`                  | int32 (5)     | `0x2a77c` | `0x3a77c` | `0x28c9f` |
| bridge      | `query_response_interval`         | int32 (5)     | `0x2a784` | `0x3a784` | `0x28b91` |
| bridge      | `last_member_interval`            | int32 (5)     | `0x2a78c` | `0x3a78c` | `0x28bd6` |
| bridge      | `vlan_filtering`                  | bool/int8 (7) | `0x2a794` | `0x3a794` | `0x28cd1` |
| bridge      | `__has_vlans`                     | bool/int8 (7) | `0x2a79c` | `0x3a79c` | `0x26d7f` |
| 8021q       | `ifname`                          | string (3)    | `0x2a880` | `0x3a880` | `0x27b66` |
| 8021q       | `vid`                             | string (3)    | `0x2a888` | `0x3a888` | `0x2798d` |
| 8021q       | `ingress_qos_mapping`             | array (1)     | `0x2a890` | `0x3a890` | `0x27991` |
| 8021q       | `egress_qos_mapping`              | array (1)     | `0x2a898` | `0x3a898` | `0x279a5` |

## 核心消费目标与常量

| 二进制      | 内容                                                                                                                                          | 文件偏移  |
| ----------- | --------------------------------------------------------------------------------------------------------------------------------------------- | --------- |
| sbin/netifd | `bridge-vlan`                                                                                                                                 | `0x26d73` |
| sbin/netifd | `/sys/devices/virtual/net/%s/bridge/stp_state`                                                                                                | `0x28982` |
| sbin/netifd | `/sys/devices/virtual/net/%s/bridge/priority`                                                                                                 | `0x28ce0` |
| sbin/netifd | `/sys/devices/virtual/net/%s/bridge/ageing_time`                                                                                              | `0x28d0c` |
| sbin/netifd | `/sys/devices/virtual/net/%s/bridge/multicast_snooping`                                                                                       | `0x289e4` |
| sbin/netifd | `/sys/devices/virtual/net/%s/bridge/multicast_querier`                                                                                        | `0x28a1a` |
| sbin/netifd | `/sys/devices/virtual/net/%s/bridge/vlan_filtering`                                                                                           | `0x28cae` |
| sbin/netifd | `/proc/sys/net/ipv6/conf/%s/disable_ipv6`                                                                                                     | `0x2895a` |
| sbin/netifd | `8021q`                                                                                                                                       | `0x279c5` |
| sbin/netifd | `8021ad`                                                                                                                                      | `0x279be` |
| sbin/netifd | `unicast`                                                                                                                                     | `0x2879d` |
| sbin/netifd | `local`                                                                                                                                       | `0x27331` |
| sbin/netifd | `blackhole`                                                                                                                                   | `0x285bc` |
| sbin/netifd | `unreachable`                                                                                                                                 | `0x26396` |
| sbin/netifd | `prohibit`                                                                                                                                    | `0x285b3` |
| sbin/netifd | `throw`                                                                                                                                       | `0x285cf` |
| sbin/netifd | `main`                                                                                                                                        | `0x1e9a`  |
| sbin/netifd | `You have delegated IPv6-prefixes but haven't assigned them to any interface. Did you forget to set option ip6assign on your lan-interfaces?` | `0x2641a` |

## 已定位的核心解析与回调

以下记录由 ARM32 反汇编、PC-relative 地址计算和参数槽索引得到；没有执行固件程序。参数表证据仅证明类型，下面单独证明实际消费及已验证的初始化回退。

| 路径                     | 文件偏移 / PC | 已核对事实                                                                                                                                                       |
| ------------------------ | ------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| bridge-object            | `0x2b144`     | bridge 对象含名称指针、参数描述符 0x3aaa4、创建回调 0x14f54、重配回调 0x14bc4；0x3aaa4 含 count=17 和 table=0x3a71c。                                            |
| bridge-parse             | `0x14c3c`     | 网桥重配函数以 count=17 解析表 0x3a71c，再按数组槽读取并保存 STP、priority、IGMP、querier、empty、VLAN-filtering 与时间参数。                                    |
| bridge-defaults          | `0x14c7c`     | 在存在值覆盖前，网桥配置初始化 stp=0、igmp_snooping=0、multicast_querier=0、bridge_empty=0、vlan_filtering=0、priority=32767；ageing_time 只在提供时标记并保存。 |
| bridge-querier           | `0x14d04`     | igmp_snooping 槽读取值后同时写 snooping 与 querier；随后单独 multicast_querier 槽可覆盖 querier，因此 querier 未设置时跟随 snooping。                            |
| device-consume           | `0x1199c`     | 设备通用参数消费读取 enabled 槽0x14、MTU槽0x4、MAC槽0xc、IPv6槽0x18；MTU 只有大于67才保存，IPv6 MTU 只有至少1280才保存。                                         |
| vlan-object              | `0x2b624`     | 8021q 对象参数描述符 0x3aaec，创建回调0x19698、重配0x19480；0x3aaec count=4、table=0x3a880。8021ad 对象复用同一参数描述符和回调。                                |
| vlan-consume             | `0x194f8`     | VLAN 重配回调解析四槽 VLAN 参数并保存 ifname/vid；对象区分 ethertype 0x8100 和 0x88a8。                                                                          |
| interface-consume        | `0x6980`      | 接口创建以count=22解析表0x3a248；auto槽带默认1读取，force_link槽使用协议标志作为回退；defaultroute、peerdns、delegate各以默认1读取。                             |
| interface-hint           | `0x6b28`      | ip6hint 先按字符串取值，再调用整数转换，显式 base=16；结果按 ip6assign 相关掩码保存，未提供时初始化为全1。                                                       |
| interface-dns-search     | `0x6a18`      | 接口创建从 dns_search 槽取数组并传给搜索域添加函数，证明手动输入不只存在于参数表。                                                                               |
| route-consume            | `0xa510`      | 核心路由消费以count=13解析表0x3a334，读取disabled、interface、目标、掩码、网关、metric、mtu、table、source、onlink和type。                                       |
| rule-consume             | `0xc44c`      | 核心规则消费解析表0x3a3e4；依槽位保存 in/out、invert、src/dest、priority、mark、lookup、action、goto、suppress_prefixlength，结束加入规则树。                    |
| bridge-vlan-consume      | `0x107ec`     | bridge-vlan 消费解析四参数表0x3a5f0，要求vlan存在；范围检查为 vlan-1≤4094（即1–4095）；local未提供时初始化1。                                                    |
| bridge-vlan-ports        | `0x109b4`     | 端口条目按冒号拆名称与标记；成员缺省为untagged，t清除untagged位，\*设置PVID位，未见u的额外动作。                                                                 |
| globals-ula              | `0x106bc`     | globals分支读取ula_prefix键（字符串位于0x26e4f），交给0xac74的前缀处理函数。                                                                                     |
| bool-default-helper      | `0x5e14`      | 布尔读取辅助函数在属性指针为空时返回传入的r1回退；非空读1字节并规范化为0/1。核对auto/defaultroute/peerdns/delegate回退1。                                        |
| interface-device-binding | `0x5f54`      | 接口主设备绑定先读取device槽，为空才读取ifname槽；非空字符串保存为主设备名。                                                                                     |
| bridge-ageing-units      | `0x22704`     | 网桥ageing_time值乘100、格式化后写/sys/devices/virtual/net/%s/bridge/ageing_time；输入单位为秒，输出为内核USER_HZ计数。                                          |
| bridge-vlan-device       | `0x10768`     | 主UCI遍历在bridge-vlan分支读取device字符串，按设备名查设备对象，要求其桥类型状态，并从同章节读取vlan参数。                                                       |
| ula-prefix-consume       | `0xac74`      | ULA前缀处理函数按/拆分IPv6地址和长度，解析IPv6地址；检查长度-1≤63即1–64，再替换/建立全局前缀对象，缺省前缀不是此处生成。                                         |
| route-source-consume     | `0xa7b8`      | route.source按路由地址族inet_pton解析来源地址；可选/后缀长度保存为来源前缀长度，否则按IPv4=32、IPv6=128回退。                                                    |
| route-onlink-consume     | `0xa7e4`      | route.onlink非零时给路由对象设置0x200标记，独立于网关地址解析。                                                                                                  |
| route-type-consume       | `0xa908`      | route.type字符串交给0x23b54→0x1ef80类型转换函数；成功保存结果并标记类型已指定，失败走错误日志路径。                                                              |
| device-name-consume      | `0xfe94`      | UCI device章节分支读取name字符串，要求非空；随后读取type并选择设备实现，再以name建立/更新设备对象。                                                              |
| route-types-map          | `0x1ef9c`     | route.type转换函数逐一比较local、nat、broadcast、anycast、multicast、prohibit、unreachable等名称并写入类型数值；与参数表/实际调用合并证明类型解析。              |

## 覆盖、验证与未决边界

- 覆盖 network 的10种章节、112个面板字段：interface34、device16、bridge-vlan4、switch7、switch_vlan4、route11、route6 10、rule12、rule6 12、globals2。
- 关键支持由真实核心消费或协议脚本跟踪：接口主设备device→ifname回退、DNS搜索域输入、IPv6子网提示16进制解析、路由/规则对象构建、网桥/VLAN配置回调、ULA前缀对象更新。
- 仅证明初始化回退的默认值：接口auto/defaultroute/peerdns/delegate=1；网桥STP/IGMP/empty/filtering=0、priority=32767；querier未设置跟随snooping；bridge-vlan.local=1。这些不是当前配置采样。
- device.disabled：设备参数表使用enabled，未找到disabled的设备消费；rule/rule6.disabled：完整规则表无此键。保留面板字段，不承诺停用效果。
- switch镜像4项：可见swconfig加载链将属性交给驱动，但脚本/反编译Lua没有精确镜像键，未在线枚举驱动能力。
- globals.packet_steering：完整可读脚本/Lua搜索和netifd参数表没有输入消费；不与RN02 ECM/NSS硬件加速混为一谈。
- 其余精细行为边界见各字段discovery：ip6class的最终分配顺序、IPv4/IPv6 route.source最终内核语义，以及action中未被源校验列出的unicast。
- 1.0.64只使用父代理提供的公开协议/服务脚本副本。dhcp.sh、ppp.sh SHA与基线一致，具体相同位置独立引用1.0.64；dhcpv6.sh已变更（接口IPv6重启修复与日本-g参数），本网络字段的请求策略仍按基线证据解释。所有核心二进制表与回退均仅确认1.0.43。
- 离线提取工具：研究目录现有binary-env的pyelftools与capstone；没有执行固件、SSH、probe、重启或读取私有配置值。
