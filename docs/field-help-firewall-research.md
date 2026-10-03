# Xiaomi RN02 1.0.43：firewall 逐字段静态证据

## 范围与限制

- 仅离线读取 `/Volumes/RN02_STATIC/rootfs` 和既有 Lua 反编译；未执行固件程序、未 SSH、未修改路由器。
- 固件二进制：`sbin/fw3`；SHA256：`7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a`；大小：111100 字节。
- `etc/init.d/firewall` 第 66、77 行使用 fw3 start/reload。当前 1.0.64 没有 firewall 消费者比对样本，因此本文件不证明 1.0.64 行为。
- 本表证明原厂 fw3 中的解析字段、类型函数与静态规则生成分支；不是已执行的规则集，也不证明内核模块或硬件卸载可用。
- 未读取或复制私有配置值。表中值是 ELF 常量、结构偏移或代码地址，不是 `/etc/config` 的设置。

## 可复现提取方法

ELF32 ARM 小端。使用 Python struct 读取程序头；PT_LOAD `[file=0, vaddr=0, filesz=103680]` 与 `[file=105140, vaddr=170676, filesz=5960]` 给出 VA→文件偏移映射。
每个选项记录为 16 字节 `<IIII>`：字段名 VA、解析函数 VA、目标结构偏移、列表头结构偏移（0 表示非列表记录）。NULL 字段名结束表。
字段名使用其 VA 经 PT_LOAD 映射后的 NUL 结尾字符串。下表逐行记录文件偏移、字符串 VA、解析函数 VA 和目标偏移，可从只读 binary 重算。
表归属来自原厂 ARM 加载器的 section 字符串比较和邻接 option-table 指针，不是仅凭字段名猜测；Capstone ARM 静态反汇编检查 PC 相对 ldr/add 常量引用。
rule: section 字符串 0x14564→0x1649c，option table 0x14568/0x14578→VA 0x2a4e4；redirect: 0x145a8→0x164dd，0x145ac/0x145bc→0x2a304；nat: 0x145ec→0x16521，0x145f0/0x14600→0x2a704。
forwarding: section 0x14630→0x18200，table 0x14634/0x14644→0x29f4c；include: 0x14674→0x165a7，table 0x14678/0x14688→0x2a1b4。
ipset: section 0x14f2c→0x17a29，table 0x14f34/0x14f48→0x2a214；zone: section 0x1501c→0x179c3，table 0x15100/0x15104→0x2a8b4；defaults: section 0x14d84 比较 0x183f5，解析表起于 file 0x19cdc。

## 原厂 fw3 选项表

| section | 字段 | 记录 file offset | 字符串 VA | parser VA / 静态类型 | 结构偏移 | 列表头偏移 |
| --- | --- | --- | --- | --- | --- | --- |
| defaults | `input` | `0x19cdc` | `0x17968` | `0xb498` 动作枚举解析器 | `0x0` | `0x0` |
| defaults | `forward` | `0x19cec` | `0x16560` | `0xb498` 动作枚举解析器 | `0x8` | `0x0` |
| defaults | `output` | `0x19cfc` | `0x17976` | `0xb498` 动作枚举解析器 | `0x4` | `0x0` |
| defaults | `drop_invalid` | `0x19d0c` | `0x18bbc` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0xc` | `0x0` |
| defaults | `tcp_reject_code` | `0x19d1c` | `0x18bc9` | `0xb470` 拒绝代码解析器 | `0x10` | `0x0` |
| defaults | `any_reject_code` | `0x19d2c` | `0x18bd9` | `0xb470` 拒绝代码解析器 | `0x14` | `0x0` |
| defaults | `syn_flood` | `0x19d3c` | `0x17fe6` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x18` | `0x0` |
| defaults | `synflood_protect` | `0x19d4c` | `0x18be9` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x18` | `0x0` |
| defaults | `synflood_rate` | `0x19d5c` | `0x18bfa` | `0xb35c` 速率解析器 | `0x1c` | `0x0` |
| defaults | `synflood_burst` | `0x19d6c` | `0x18c08` | `0xaf58` 32 位数值解析器 | `0x24` | `0x0` |
| defaults | `rst_flood` | `0x19d7c` | `0x18010` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x2c` | `0x0` |
| defaults | `rstflood_protect` | `0x19d8c` | `0x18c17` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x2c` | `0x0` |
| defaults | `rstflood_rate` | `0x19d9c` | `0x18c28` | `0xb35c` 速率解析器 | `0x30` | `0x0` |
| defaults | `rstflood_burst` | `0x19dac` | `0x18c36` | `0xaf58` 32 位数值解析器 | `0x38` | `0x0` |
| defaults | `icmp_flood` | `0x19dbc` | `0x17f73` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x40` | `0x0` |
| defaults | `icmpflood_protect` | `0x19dcc` | `0x18c45` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x40` | `0x0` |
| defaults | `icmpflood_rate` | `0x19ddc` | `0x18c57` | `0xb35c` 速率解析器 | `0x44` | `0x0` |
| defaults | `icmpflood_burst` | `0x19dec` | `0x18c66` | `0xaf58` 32 位数值解析器 | `0x4c` | `0x0` |
| defaults | `udp_flood` | `0x19dfc` | `0x1801a` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x68` | `0x0` |
| defaults | `udpflood_protect` | `0x19e0c` | `0x18c76` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x68` | `0x0` |
| defaults | `udpflood_rate` | `0x19e1c` | `0x18c87` | `0xb35c` 速率解析器 | `0x6c` | `0x0` |
| defaults | `udpflood_burst` | `0x19e2c` | `0x18c95` | `0xaf58` 32 位数值解析器 | `0x74` | `0x0` |
| defaults | `spi_rule` | `0x19e3c` | `0x18079` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x7c` | `0x0` |
| defaults | `port_trigger` | `0x19e4c` | `0x18ca4` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x7d` | `0x0` |
| defaults | `fw_enable` | `0x19e5c` | `0x18cb1` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x7e` | `0x0` |
| defaults | `dos_enable` | `0x19e6c` | `0x18cbb` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x7f` | `0x0` |
| defaults | `tcp_syncookies` | `0x19e7c` | `0x18cc6` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x80` | `0x0` |
| defaults | `tcp_ecn` | `0x19e8c` | `0x18cd5` | `0xaf58` 32 位数值解析器 | `0x84` | `0x0` |
| defaults | `tcp_window_scaling` | `0x19e9c` | `0x18cdd` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x88` | `0x0` |
| defaults | `accept_redirects` | `0x19eac` | `0x18cf0` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x89` | `0x0` |
| defaults | `accept_source_route` | `0x19ebc` | `0x18d01` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x8a` | `0x0` |
| defaults | `auto_helper` | `0x19ecc` | `0x18d15` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x8c` | `0x0` |
| defaults | `custom_chains` | `0x19edc` | `0x1841c` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x8b` | `0x0` |
| defaults | `disable_ipv6` | `0x19eec` | `0x18d21` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x8f` | `0x0` |
| defaults | `flow_offloading` | `0x19efc` | `0x18d2e` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x8d` | `0x0` |
| defaults | `flow_offloading_hw` | `0x19f0c` | `0x18d3e` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x8e` | `0x0` |
| defaults | `__flags_v4` | `0x19f1c` | `0x183fe` | `0xaf58` 32 位数值解析器 | `0x90` | `0x0` |
| defaults | `__flags_v6` | `0x19f2c` | `0x18409` | `0xaf58` 32 位数值解析器 | `0x94` | `0x0` |
| forwarding | `enabled` | `0x19f4c` | `0x18d51` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x8` | `0x0` |
| forwarding | `name` | `0x19f5c` | `0x17acf` | `0x4c10` 字符串解析器 | `0xc` | `0x0` |
| forwarding | `family` | `0x19f6c` | `0x16c5e` | `0xa9d0` 地址族解析器 | `0x10` | `0x0` |
| forwarding | `src` | `0x19f7c` | `0x18d8b` | `0xa468` 区域/设备引用解析器 | `0x1c` | `0x0` |
| forwarding | `dest` | `0x19f8c` | `0x18d95` | `0xa468` 区域/设备引用解析器 | `0x68` | `0x0` |
| include | `enabled` | `0x1a1b4` | `0x18d51` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x8` | `0x0` |
| include | `path` | `0x1a1c4` | `0x166a0` | `0x4c10` 字符串解析器 | `0x14` | `0x0` |
| include | `type` | `0x1a1d4` | `0x178e0` | `0xb068` include 类型枚举解析器 | `0x18` | `0x0` |
| include | `family` | `0x1a1e4` | `0x16c5e` | `0xa9d0` 地址族解析器 | `0x10` | `0x0` |
| include | `reload` | `0x1a1f4` | `0x18593` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x1c` | `0x0` |
| ipset | `enabled` | `0x1a214` | `0x18d51` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x8` | `0x0` |
| ipset | `name` | `0x1a224` | `0x17acf` | `0x4c10` 字符串解析器 | `0xc` | `0x0` |
| ipset | `family` | `0x1a234` | `0x16c5e` | `0xa9d0` 地址族解析器 | `0x10` | `0x0` |
| ipset | `storage` | `0x1a244` | `0x1843f` | `0xb330` 集合存储枚举解析器 | `0x14` | `0x0` |
| ipset | `match` | `0x1a254` | `0x16dd4` | `0xb1ec` 集合匹配类型解析器 | `0x18` | `0x88` |
| ipset | `iprange` | `0x1a264` | `0x17d4b` | `0xab5c` 地址范围解析器 | `0x20` | `0x0` |
| ipset | `portrange` | `0x1a274` | `0x18447` | `0xa20c` 端口/范围解析器 | `0x50` | `0x0` |
| ipset | `netmask` | `0x1a284` | `0x1936d` | `0xaf58` 32 位数值解析器 | `0x60` | `0x0` |
| ipset | `maxelem` | `0x1a294` | `0x19375` | `0xaf58` 32 位数值解析器 | `0x64` | `0x0` |
| ipset | `hashsize` | `0x1a2a4` | `0x1937d` | `0xaf58` 32 位数值解析器 | `0x68` | `0x0` |
| ipset | `timeout` | `0x1a2b4` | `0x19386` | `0xaf58` 32 位数值解析器 | `0x6c` | `0x0` |
| ipset | `external` | `0x1a2c4` | `0x19109` | `0x4c10` 字符串解析器 | `0x70` | `0x0` |
| ipset | `entry` | `0x1a2d4` | `0x1938e` | `0x9718` 集合成员解析器 | `0x74` | `0x88` |
| ipset | `loadfile` | `0x1a2e4` | `0x19394` | `0x4c10` 字符串解析器 | `0x7c` | `0x0` |
| redirect | `enabled` | `0x1a304` | `0x18d51` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x8` | `0x0` |
| redirect | `name` | `0x1a314` | `0x17acf` | `0x4c10` 字符串解析器 | `0xc` | `0x0` |
| redirect | `family` | `0x1a324` | `0x16c5e` | `0xa9d0` 地址族解析器 | `0x10` | `0x0` |
| redirect | `src` | `0x1a334` | `0x18d8b` | `0xa468` 区域/设备引用解析器 | `0x1c` | `0x0` |
| redirect | `dest` | `0x1a344` | `0x18d95` | `0xa468` 区域/设备引用解析器 | `0x68` | `0x0` |
| redirect | `ipset` | `0x1a354` | `0x17a29` | `0x79ac` IP 集合引用解析器 | `0xb4` | `0x0` |
| redirect | `helper` | `0x1a364` | `0x17896` | `0x9780` 连接助手引用解析器 | `0xe8` | `0x0` |
| redirect | `proto` | `0x1a374` | `0x1627c` | `0xa800` 协议解析器 | `0x118` | `0x280` |
| redirect | `src_ip` | `0x1a384` | `0x19264` | `0xadec` 地址/网络解析器 | `0x120` | `0x0` |
| redirect | `src_mac` | `0x1a394` | `0x1926b` | `0xaa94` MAC 解析器 | `0x150` | `0x280` |
| redirect | `src_port` | `0x1a3a4` | `0x19273` | `0xa20c` 端口/范围解析器 | `0x158` | `0x0` |
| redirect | `src_dip` | `0x1a3b4` | `0x192f3` | `0xadec` 地址/网络解析器 | `0x168` | `0x0` |
| redirect | `src_dport` | `0x1a3c4` | `0x192fb` | `0xa20c` 端口/范围解析器 | `0x198` | `0x0` |
| redirect | `dest_ip` | `0x1a3d4` | `0x1927c` | `0xadec` 地址/网络解析器 | `0x1a8` | `0x0` |
| redirect | `dest_port` | `0x1a3e4` | `0x19284` | `0xa20c` 端口/范围解析器 | `0x1d8` | `0x0` |
| redirect | `extra` | `0x1a3f4` | `0x18d7f` | `0x4c10` 字符串解析器 | `0x274` | `0x0` |
| redirect | `limit` | `0x1a404` | `0x17ce5` | `0xb35c` 速率解析器 | `0x1e8` | `0x0` |
| redirect | `limit_burst` | `0x1a414` | `0x19298` | `0xaf58` 32 位数值解析器 | `0x1f0` | `0x0` |
| redirect | `utc_time` | `0x1a424` | `0x192a4` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x1f8` | `0x0` |
| redirect | `start_date` | `0x1a434` | `0x192ad` | `0xa580` 日期解析器 | `0x1fc` | `0x0` |
| redirect | `stop_date` | `0x1a444` | `0x192b8` | `0xa580` 日期解析器 | `0x228` | `0x0` |
| redirect | `start_time` | `0x1a454` | `0x192c2` | `0x9ee4` 时间解析器 | `0x254` | `0x0` |
| redirect | `stop_time` | `0x1a464` | `0x192cd` | `0x9ee4` 时间解析器 | `0x258` | `0x0` |
| redirect | `weekdays` | `0x1a474` | `0x17dd6` | `0xb090` 星期解析器 | `0x260` | `0x0` |
| redirect | `monthdays` | `0x1a484` | `0x17dca` | `0x9dc8` 月日期解析器 | `0x25c` | `0x0` |
| redirect | `mark` | `0x1a494` | `0x17d30` | `0xa368` 标记/掩码解析器 | `0x264` | `0x0` |
| redirect | `reflection` | `0x1a4a4` | `0x1833f` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x279` | `0x0` |
| redirect | `reflection_src` | `0x1a4b4` | `0x19305` | `0xb040` 回环来源枚举解析器 | `0x27c` | `0x0` |
| redirect | `target` | `0x1a4c4` | `0x16d5f` | `0xb498` 动作枚举解析器 | `0x270` | `0x0` |
| rule | `enabled` | `0x1a4e4` | `0x18d51` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x8` | `0x0` |
| rule | `name` | `0x1a4f4` | `0x17acf` | `0x4c10` 字符串解析器 | `0xc` | `0x0` |
| rule | `family` | `0x1a504` | `0x16c5e` | `0xa9d0` 地址族解析器 | `0x10` | `0x0` |
| rule | `src` | `0x1a514` | `0x18d8b` | `0xa468` 区域/设备引用解析器 | `0x24` | `0x0` |
| rule | `dest` | `0x1a524` | `0x18d95` | `0xa468` 区域/设备引用解析器 | `0x70` | `0x0` |
| rule | `device` | `0x1a534` | `0x16275` | `0x4c10` 字符串解析器 | `0x1c` | `0x0` |
| rule | `direction` | `0x1a544` | `0x1924f` | `0x50d4` 方向解析器 | `0x20` | `0x0` |
| rule | `ipset` | `0x1a554` | `0x17a29` | `0x79ac` IP 集合引用解析器 | `0xbc` | `0x0` |
| rule | `helper` | `0x1a564` | `0x17896` | `0x9780` 连接助手引用解析器 | `0xf0` | `0x0` |
| rule | `set_helper` | `0x1a574` | `0x19259` | `0x9780` 连接助手引用解析器 | `0x204` | `0x0` |
| rule | `proto` | `0x1a584` | `0x1627c` | `0xa800` 协议解析器 | `0x120` | `0x238` |
| rule | `src_ip` | `0x1a594` | `0x19264` | `0xadec` 地址/网络解析器 | `0x128` | `0x238` |
| rule | `src_mac` | `0x1a5a4` | `0x1926b` | `0xaa94` MAC 解析器 | `0x130` | `0x238` |
| rule | `src_port` | `0x1a5b4` | `0x19273` | `0xa20c` 端口/范围解析器 | `0x138` | `0x238` |
| rule | `dest_ip` | `0x1a5c4` | `0x1927c` | `0xadec` 地址/网络解析器 | `0x140` | `0x238` |
| rule | `dest_port` | `0x1a5d4` | `0x19284` | `0xa20c` 端口/范围解析器 | `0x148` | `0x238` |
| rule | `icmp_type` | `0x1a5e4` | `0x1928e` | `0x9fe8` ICMP 类型解析器 | `0x150` | `0x238` |
| rule | `extra` | `0x1a5f4` | `0x18d7f` | `0x4c10` 字符串解析器 | `0x234` | `0x0` |
| rule | `limit` | `0x1a604` | `0x17ce5` | `0xb35c` 速率解析器 | `0x158` | `0x0` |
| rule | `limit_burst` | `0x1a614` | `0x19298` | `0xaf58` 32 位数值解析器 | `0x160` | `0x0` |
| rule | `utc_time` | `0x1a624` | `0x192a4` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x168` | `0x0` |
| rule | `start_date` | `0x1a634` | `0x192ad` | `0xa580` 日期解析器 | `0x16c` | `0x0` |
| rule | `stop_date` | `0x1a644` | `0x192b8` | `0xa580` 日期解析器 | `0x198` | `0x0` |
| rule | `start_time` | `0x1a654` | `0x192c2` | `0x9ee4` 时间解析器 | `0x1c4` | `0x0` |
| rule | `stop_time` | `0x1a664` | `0x192cd` | `0x9ee4` 时间解析器 | `0x1c8` | `0x0` |
| rule | `weekdays` | `0x1a674` | `0x17dd6` | `0xb090` 星期解析器 | `0x1d0` | `0x0` |
| rule | `monthdays` | `0x1a684` | `0x17dca` | `0x9dc8` 月日期解析器 | `0x1cc` | `0x0` |
| rule | `mark` | `0x1a694` | `0x17d30` | `0xa368` 标记/掩码解析器 | `0x1d4` | `0x0` |
| rule | `set_mark` | `0x1a6a4` | `0x192d7` | `0xa368` 标记/掩码解析器 | `0x1e8` | `0x0` |
| rule | `set_xmark` | `0x1a6b4` | `0x192e0` | `0xa368` 标记/掩码解析器 | `0x1f4` | `0x0` |
| rule | `dscp` | `0x1a6c4` | `0x178f3` | `0x9ccc` DSCP 解析器 | `0x1e0` | `0x0` |
| rule | `set_dscp` | `0x1a6d4` | `0x192ea` | `0x9ccc` DSCP 解析器 | `0x200` | `0x0` |
| rule | `target` | `0x1a6e4` | `0x16d5f` | `0xb498` 动作枚举解析器 | `0x1e4` | `0x0` |
| nat | `enabled` | `0x1a704` | `0x18d51` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x8` | `0x0` |
| nat | `name` | `0x1a714` | `0x17acf` | `0x4c10` 字符串解析器 | `0xc` | `0x0` |
| nat | `family` | `0x1a724` | `0x16c5e` | `0xa9d0` 地址族解析器 | `0x10` | `0x0` |
| nat | `src` | `0x1a734` | `0x18d8b` | `0xa468` 区域/设备引用解析器 | `0x18` | `0x0` |
| nat | `device` | `0x1a744` | `0x16275` | `0x4c10` 字符串解析器 | `0xc8` | `0x0` |
| nat | `ipset` | `0x1a754` | `0x17a29` | `0x79ac` IP 集合引用解析器 | `0x64` | `0x0` |
| nat | `proto` | `0x1a764` | `0x1627c` | `0xa800` 协议解析器 | `0xcc` | `0x228` |
| nat | `src_ip` | `0x1a774` | `0x19264` | `0xadec` 地址/网络解析器 | `0xd4` | `0x0` |
| nat | `src_port` | `0x1a784` | `0x19273` | `0xa20c` 端口/范围解析器 | `0x104` | `0x0` |
| nat | `snat_ip` | `0x1a794` | `0x19314` | `0xadec` 地址/网络解析器 | `0x154` | `0x0` |
| nat | `snat_port` | `0x1a7a4` | `0x1931c` | `0xa20c` 端口/范围解析器 | `0x184` | `0x0` |
| nat | `dest_ip` | `0x1a7b4` | `0x1927c` | `0xadec` 地址/网络解析器 | `0x114` | `0x0` |
| nat | `dest_port` | `0x1a7c4` | `0x19284` | `0xa20c` 端口/范围解析器 | `0x144` | `0x0` |
| nat | `extra` | `0x1a7d4` | `0x18d7f` | `0x4c10` 字符串解析器 | `0x224` | `0x0` |
| nat | `limit` | `0x1a7e4` | `0x17ce5` | `0xb35c` 速率解析器 | `0x194` | `0x0` |
| nat | `limit_burst` | `0x1a7f4` | `0x19298` | `0xaf58` 32 位数值解析器 | `0x19c` | `0x0` |
| nat | `connlimit_ports` | `0x1a804` | `0x19326` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x21c` | `0x0` |
| nat | `utc_time` | `0x1a814` | `0x192a4` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x1a4` | `0x0` |
| nat | `start_date` | `0x1a824` | `0x192ad` | `0xa580` 日期解析器 | `0x1a8` | `0x0` |
| nat | `stop_date` | `0x1a834` | `0x192b8` | `0xa580` 日期解析器 | `0x1d4` | `0x0` |
| nat | `start_time` | `0x1a844` | `0x192c2` | `0x9ee4` 时间解析器 | `0x200` | `0x0` |
| nat | `stop_time` | `0x1a854` | `0x192cd` | `0x9ee4` 时间解析器 | `0x204` | `0x0` |
| nat | `weekdays` | `0x1a864` | `0x17dd6` | `0xb090` 星期解析器 | `0x20c` | `0x0` |
| nat | `monthdays` | `0x1a874` | `0x17dca` | `0x9dc8` 月日期解析器 | `0x208` | `0x0` |
| nat | `mark` | `0x1a884` | `0x17d30` | `0xa368` 标记/掩码解析器 | `0x210` | `0x0` |
| nat | `target` | `0x1a894` | `0x16d5f` | `0xb498` 动作枚举解析器 | `0x220` | `0x0` |
| zone | `enabled` | `0x1a8b4` | `0x18d51` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x8` | `0x0` |
| zone | `name` | `0x1a8c4` | `0x17acf` | `0x4c10` 字符串解析器 | `0xc` | `0x0` |
| zone | `family` | `0x1a8d4` | `0x16c5e` | `0xa9d0` 地址族解析器 | `0x10` | `0x0` |
| zone | `network` | `0x1a8e4` | `0x185c1` | `0xa468` 区域/设备引用解析器 | `0x20` | `0x88` |
| zone | `device` | `0x1a8f4` | `0x16275` | `0xa468` 区域/设备引用解析器 | `0x28` | `0x88` |
| zone | `subnet` | `0x1a904` | `0x18430` | `0xadec` 地址/网络解析器 | `0x30` | `0x88` |
| zone | `input` | `0x1a914` | `0x17968` | `0xb498` 动作枚举解析器 | `0x14` | `0x0` |
| zone | `forward` | `0x1a924` | `0x16560` | `0xb498` 动作枚举解析器 | `0x1c` | `0x0` |
| zone | `output` | `0x1a934` | `0x17976` | `0xb498` 动作枚举解析器 | `0x18` | `0x0` |
| zone | `masq` | `0x1a944` | `0x16420` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x40` | `0x0` |
| zone | `masq_allow_invalid` | `0x1a954` | `0x18d59` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x41` | `0x0` |
| zone | `masq_src` | `0x1a964` | `0x18d6c` | `0xadec` 地址/网络解析器 | `0x44` | `0x88` |
| zone | `masq_dest` | `0x1a974` | `0x18d75` | `0xadec` 地址/网络解析器 | `0x4c` | `0x88` |
| zone | `extra` | `0x1a984` | `0x18d7f` | `0x4c10` 字符串解析器 | `0x38` | `0x0` |
| zone | `extra_src` | `0x1a994` | `0x18d85` | `0x4c10` 字符串解析器 | `0x38` | `0x0` |
| zone | `extra_dest` | `0x1a9a4` | `0x18d8f` | `0x4c10` 字符串解析器 | `0x3c` | `0x0` |
| zone | `mtu_fix` | `0x1a9b4` | `0x18414` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x54` | `0x0` |
| zone | `custom_chains` | `0x1a9c4` | `0x1841c` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x74` | `0x0` |
| zone | `log` | `0x1a9d4` | `0x18d9a` | `0xaf58` 32 位数值解析器 | `0x60` | `0x0` |
| zone | `log_limit` | `0x1a9e4` | `0x18d9e` | `0xb35c` 速率解析器 | `0x64` | `0x0` |
| zone | `auto_helper` | `0x1a9f4` | `0x18d15` | `0x5174` 布尔解析器（true / yes / 1 为真） | `0x75` | `0x0` |
| zone | `helper` | `0x1aa04` | `0x17896` | `0x9780` 连接助手引用解析器 | `0x58` | `0x88` |
| zone | `__flags_v4` | `0x1aa14` | `0x183fe` | `0xaf58` 32 位数值解析器 | `0x78` | `0x0` |
| zone | `__flags_v6` | `0x1aa24` | `0x18409` | `0xaf58` 32 位数值解析器 | `0x7c` | `0x0` |
| zone | `__addrs` | `0x1aa34` | `0x18437` | `0xab5c` 地址范围解析器 | `0x80` | `0x88` |

## 静态生成与解析事实

- bool：parser 0x5174–0x51d8 比较常量 true、yes、1（字符串 VA 0x17e92 / 0x17e97 / 0x19138），写入一个布尔字节；它本身不提供缺省值。
- policies：0x8238–0x827c 读取策略枚举；缺失/无效策略的消息分别为 VA 0x16a30 / 0x16a5f（defaulting to DROP）；LuCI model/firewall.lua 789–824 明确对 defaults input/forward/output 返回 DROP。
- synflood：defaults syn_flood 与 synflood_protect 两个记录写入同一结构偏移 0x18；生成器含 --syn（VA 0x17fe0）和 syn_flood 链名（0x17fe6），还有 SYN 速率/突发选项记录；这是别名，不是两种独立保护。
- invalid：生成器 VA 0x17fc1 为 -m conntrack --ctstate INVALID；defaults.drop_invalid 是 bool、目标偏移 0xc；LuCI model 776–779 仅在读取值为 1 时返回真。
- offload：flow_offloading/flow_offloading_hw 表项写入 defaults 0x8d/0x8e；0x14e1c–0x14e60 检查保存后的卸载标志和模块文件；生成 0xcb28–0xcb8c 使用 Traffic offloading、RELATED,ESTABLISHED、FLOWOFFLOAD（0x162c9），硬件分支另外追加 --hw（0x17fbc）。仅静态分支，不证明硬件支持。
- zone-match：zone.network/device 是引用解析器的列表，subnet 是地址解析器列表；生成模板 -i %s / -o %s / -s %s/%s / -d %s/%s（VA 0x1884d–0x18865）。逻辑 network 和物理 device 不是同一个命名空间。
- masq：zone.masq 是 bool；生成器 0xf834/0xf83c 引用 MASQUERADE（VA 0x176f4），并逐项消费 masq_src、masq_dest 地址列表；VA 0x163fd / 0x16425 的报错明确 unresolved masq_src/masq_dest 会 disabling masq。
- mss：0x101e0/0x101f0 引用 --clamp-mss-to-pmtu（0x18803）；邻接常量 TCPMSS（0x187fc）、SYN,RST（0x187bd）；zone.mtu_fix 表项为 bool，偏移 0x54。
- zone-log：zone.log 是 32 位数值 parser，不是独立 bool parser；LuCI zone-details 460–468 将其作为开关写 1；生成器引用 --log-prefix（0xfcbc/0xfccc、0xfd80/0xfd90、0x1013c/0x1014c），zone.log_limit 使用速率解析器。
- forwarding：0x10678/0x10684 引用 section forwarding；VA 0x185c9 / 0x185e3 为 Forward src->dest 与 Zone src to dest forwarding policy，跳转目标模板 zone_%s_dest_ACCEPT（0x18603）；该段有独立 src、dest 字段。
- name：rule/nat/redirect 名称记录由字符串解析器写入结构偏移 0xc；静态显示格式 Rule %s / NAT %s / Redirect %s（VA 0x17849 / 0x1797d / 0x1823f），名字不是地址或端口匹配条件。
- rule-direction：原厂规则消息 VA 0x17315 明确没有 src/dest 时 assuming an output rule；规则生成器引用 zone_%s_input / zone_%s_output / zone_%s_forward（0x17960 / 0x1796e / 0x17950），两端区域决定安装方向。
- proto-fallback：原厂 rule 与 redirect 消息 VA 0x17379 / 0x16f76 为 does not specify a protocol, assuming TCP+UDP；nat 消息 0x177bd 为 assuming all，且 0x12280–0x12290 向空协议列表添加 all。
- ports：parser 0xa20c–0xa35c 识别前缀 !、范围分隔 - 或 :，将端点存入 uint16；生成 0x9020/0x9030→--sport、0x9074/0x9084→--dport。协议端口位宽是 0–65535；不把宽松转换实现视为输入校验保证。
- mac：MAC parser 0xaa94；生成常量 --mac-source（VA 0x17d17）和六段十六进制格式（0x17cf9）；redirect SNAT 对 src_mac 的报错 VA 0x16ee3 明确 must not use src_mac option for SNAT target。
- ipset-ref：parser 0x79ac 解析规则的 ipset 引用；生成 0x8ddc/0x8de8→--match-set（0x17cd1）；原厂消息 0x17040 / 0x16b52 / 0x174c0 为 rule/redirect/nat skipped due to disabled ipset support，0x1707e / 0x16b94 / 0x174fd 为未知集合。
- mark：parser 0xa368–0xa45c 识别 ! 与 /，读取 mark 及 mask；0xa418–0xa41c 未指定 mask 时写入 0xffffffff；生成 0x892c/0x893c→--mark（0x17d2e）及 0x%x/0x%x（0x17d24）。这是匹配，不是 set_mark。
- rate：limit 与 limit_burst 由速率/32 位数值 parser 读取；0x8ef8/0x8f08→--limit，0x8f2c/0x8f3c→--limit-burst；速率单位表有 second/minute/hour/day（表 VA 0x2b178），字符串格式 %u/%s（0x17cdd）。
- rule-target：rule target 缺失/无效消息 VA 0x173bf / 0x17405 均 defaulting to REJECT；动作枚举还包括 ACCEPT、DROP、MARK、NOTRACK、HELPER、DSCP；0x12f7c–0x12fcc 的 MARK 分支要求 set_mark 或 set_xmark。
- set-mark：rule.set_mark 与 set_xmark 使用 mark/mask parser；0xf1ac/0xf1b4→--set-mark（0x17832），0xeea8/0xeeb0→--set-xmark（0x1783d）；原厂 0x17228 消息禁止 inverted set_mark/set_xmark/set_dscp。
- helper：rule.helper（偏移 0xf0）与 rule.set_helper（0x204）是两个不同字段；--helper 常量用于 helper 匹配（0x8704/0x8710→0x17cc8）；HELPER 动作缺少 set_helper 的报错 VA 0x1727d。填写 helper 本身不等于指定 HELPER 动作的助手。
- time：start_time/stop_time 的 parser 0x9ee4–0x9fdc 检查小时≤23、分钟/秒≤59，允许 HH:MM 或 HH:MM:SS；生成 --timestart / --timestop（0x8b4c/0x8b5c、0x8bc4/0x8bd4），utc_time=假时的 --kerneltz 引用为 0x8a60/0x8a6c。
- weekdays：weekdays parser 0xb090 的枚举表 VA 0x2b1a4 指向 monday…sunday；生成 0x8cc0/0x8ccc→--weekdays（0x17dd4）。
- redirect-target：redirect target 缺失/无效消息 VA 0x16c8c / 0x16cd4 均 defaulting to DNAT；init validator 23–31 仅接受 SNAT/DNAT；转发 DNAT 的 dest_ip/dest_port 是转换目的地，SNAT 的 src_dip/src_dport 是改写后的来源地址/端口。
- reflection：redirect.reflection bool、reflection_src enum；回环来源枚举表 VA 0x2b19c 只有 internal/external；生成器中 reflection 常量 VA 0x1833f。LuCI forward-details 417–441 说明 NAT Loopback 且缺值显示 1；这不是对所有来源区域都可生成回环的证明。
- nat-src：nat.src 是区域引用；源 NAT 生成器 0x1797d–0x17a8f 的显示与链模板为 NAT、zone_%s_postrouting。该 src 指源 NAT 挂载的出站区域，不是 rule.src 的入站区域语义；nat 表没有 dest 区域字段。
- nat-target：0x12110–0x1215c 缺 target 时将枚举 15 写入 nat 偏移 0x220；消息 VA 0x17668 / 0x176b1 明确默认 MASQUERADE。SNAT 必须提供 snat_ip 或 snat_port（0x176ff），非 SNAT 不得使用这两字段（0x17745 / 0x17780）。
- include-type：include.type enum parser 0xb068→表 VA 0x2b194，元素仅 script / restore（字符串 VA 0x165af / 0x1822d），没有 nftables；这是 fw3 的脚本或 iptables-restore 类型，非 fw4 nftables。
- include-path：静态脚本执行消息 VA 0x1887d；0x188bb 包含 shell config() 禁止 UCI 的包装并 source 附加路径；路径不存在的消息 0x18894；lib/firewall.sysapi.loader 第 7–9 行继续调用 vendor sysapi.firewall。
- include-reload：include.reload bool 的结构偏移 0x1c；0x7e00–0x7e10 在 reload 时测试该偏移、为 0 则跳过 restore 配置；脚本类型由独立执行分支处理。静态表证明此字段被解析，不证明脚本幂等性。
- ipset-family：ipset.family 使用地址族 parser；原厂检查消息 VA 0x1675d 明确集合 must not have family any；IPv4/IPv6 集合分开，不是双栈集合。 名称检查消息 VA 0x16724 为 ipset must have a name assigned，加载引用为 0x10dfc/0x10e00。
- ipset-match：match parser 0xb1ec–0xb308 识别 dest_、dst_、src_ 前缀并使用 ip/port/mac/net/set 类型表；原厂检查 0x167ce / 0x16801 要求至少一个、最多三个数据类型；storage/matches 组合无效消息 VA 0x169de。
- ipset-storage：storage parser 0xb330→枚举表 VA 0x2b150（bitmap/hash/list）；未给 storage 的消息 0x16847 明确按 matches 推定方法，未从配置样本推定一个固定默认。
- ipset-create：生成 0x13858/0x13864→Creating ipset；0x13910/0x13914→timeout %u；0x13928/0x1392c→maxelem %u；VA 0x189af 为 create %s %s，VA 0x18a21 / 0x18a67 为 add %s %s。字段解析存在不等于集合已成功创建。
- ipset-maxelem：maxelem 为 32 位数值 parser；原厂 VA 0x1698d 消息为 maxelem ignored，说明某些 storage/type 组合不使用此值；创建格式 maxelem %u，影响集合容量而非流量速率。
- ipset-timeout：timeout 为 32 位数值 parser；原厂创建格式 timeout %u（VA 0x189f0），成员可由 ipset 内核按超时删除；秒和 0 不超时为 ipset 协议语义，不是读取固件当前设置。

## 有界缺失与不确定性

- `zone.masq6`：在该 section 的完整 fw3 选项表中没有此字段；fw3 全部 NUL 字符串中也没有该名称；同时搜索 etc/init.d/firewall、etc/hotplug.d/iface/20-firewall、lib/firewall*、usr/sbin/sysapi.firewall、全部既有 Lua 反编译以及包含 firewall 的 shell/vendor 消费者，未找到该 section/字段的原厂读取。
- `redirect.reflection_zone`：在该 section 的完整 fw3 选项表中没有此字段；fw3 全部 NUL 字符串中也没有该名称；同时搜索 etc/init.d/firewall、etc/hotplug.d/iface/20-firewall、lib/firewall*、usr/sbin/sysapi.firewall、全部既有 Lua 反编译以及包含 firewall 的 shell/vendor 消费者，未找到该 section/字段的原厂读取。
- `include.fw4_compatible`：在该 section 的完整 fw3 选项表中没有此字段；fw3 全部 NUL 字符串中也没有该名称；同时搜索 etc/init.d/firewall、etc/hotplug.d/iface/20-firewall、lib/firewall*、usr/sbin/sysapi.firewall、全部既有 Lua 反编译以及包含 firewall 的 shell/vendor 消费者，未找到该 section/字段的原厂读取。
- `include.type=nftables`：type 字段存在，但其枚举表仅 script/restore；同范围字符串与脚本搜索没有 nftables 消费者。
- `nat.dest`：在该 section 的完整 fw3 选项表中没有此字段（名称可能存在于其他 section，不能据此当作 nat 消费）；同时搜索 etc/init.d/firewall、etc/hotplug.d/iface/20-firewall、lib/firewall*、usr/sbin/sysapi.firewall、全部既有 Lua 反编译以及包含 firewall 的 shell/vendor 消费者，未找到该 section/字段的原厂读取。
- `nat.src_mac`：在该 section 的完整 fw3 选项表中没有此字段（名称可能存在于其他 section，不能据此当作 nat 消费）；同时搜索 etc/init.d/firewall、etc/hotplug.d/iface/20-firewall、lib/firewall*、usr/sbin/sysapi.firewall、全部既有 Lua 反编译以及包含 firewall 的 shell/vendor 消费者，未找到该 section/字段的原厂读取。
- 已知通用 schema 含上述字段；目录字段仍保留可编辑。缺失条目只报告 1.0.43 的静态边界，不声称一定无效于其他固件，也不以 IPv6 vendor 额外 NAT 脚本冒充 masq6 读取。
- zone-name：0x15158–0x151b4 要求 zone 名称非空，strlen 后比较 14，超过时丢弃该区域；这是 fw3 的区域名称长度限制，不是 UCI section ID 长度。 同段 0x150f4–0x1510c 读取 enabled 偏移 8，为 0 时释放并跳过区域。
