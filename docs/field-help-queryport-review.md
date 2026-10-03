# dnsmasq queryport=0 静态复核（RN02 1.0.43）

## 范围与结论

只读取 `/Volumes/RN02_STATIC/rootfs/usr/sbin/dnsmasq` 和启动脚本，用本机 radare2 静态反汇编。没有运行目标固件程序、SSH、探测或读取私有配置。

**原厂 `queryport=0` 不等于省略此字段的随机查询端口策略。** 它会启用共享查询 socket 的复用分支。各 socket 的端口可由系统分配，或由 `minport`/`maxport` 指定的端口策略选择；不应声称所有地址族、绑定地址和接口都必然共用一个绝对端口。

建议字段说明：`设置上游 DNS 查询的来源端口。0 启用单端口复用模式，由系统或端口范围策略选取端口；不等于留空时的随机查询端口策略。`

建议 summary：`0 开启单端口复用，不等于留空时的默认随机端口策略。`

## 原始制品

- 原始文件：`usr/sbin/dnsmasq`。
- SHA-256：`f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c`。
- 文件大小：233355 字节。ELF32 little-endian ARM。
- 第一个 PT_LOAD 的文件偏移和虚拟地址均为 0，文件大小为 224372。因此本报告中 `0x00013668` 等代码地址同时也是文件字节偏移；数据表 `0x37138` 是明确的文件偏移，不当作虚拟地址。
- 启动消费者：`etc/init.d/dnsmasq:1083` 将 UCI `queryport` 非空值原样写为 `--query-port`；`:1084–1085` 独立写入 `minport`/`maxport`；`:170–178` 的 `append_parm` 仅为空时省略，不会省略字符串 `0`。

## 选项表与分派

文件 `usr/sbin/dnsmasq` 偏移 `0x37138` 的四个 little-endian uint32 为 `0x34f15, 1, 0, 81`。`0x34f15` 的 NUL 结尾字符串是 `query-port`；第四项 81 是 `Q` 的 ASCII 值。

原厂选项解析函数 `0x13668` 以选项值减去 `0x30` 索引跳转表。`Q` 对应跳转条目文件偏移 `0x137a0`，目标为 `0x15df8`：

```text
0x00013708 sub r3, r6, 0x30
0x00013714 addls pc, pc, r3, lsl 2
0x000137a0 b 0x15df8
```

## 0 值确实切换模式

查询端口解析分支文件偏移 `0x15df8–0x15e30`：

```text
0x00015dfc ldr r0, [sp, 0x34]
0x00015e04 ldr r1, [r4]
0x00015e08 add r1, r1, 0x108
0x00015e0c bl 0xc9fc
0x00015e18 ldr r3, [r4]
0x00015e1c ldr r2, [r3, 0x108]
0x00015e20 cmp r2, 0
0x00015e24 moveq r4, 1
0x00015e28 streq r4, [r3, 0x90]
0x00015e2c beq 0x136b4
```

`0xc9fc` 调用纯十进制解析器并检查小于 `0x10000`（65536），见文件偏移 `0xc9b0–0xca20`。解析结果写入共享状态偏移 `+0x108`。结果为 0 时明确把模式标志 `+0x90` 写为 1。它不是“0 就省略或恢复默认”。

## 共享 socket 查找与复用

socket 分配函数文件偏移 `0x1b50c`，在 `0x1b54c–0x1b56c` 读取上述 `+0x90` 标志，为非零时遍历共享 socket 列表：

```text
0x0001b54c ldr r5, [r3, 0x90]
0x0001b550 cmp r5, 0
0x0001b554 beq 0x1b5b8
0x0001b564 ldr r6, [r3, 0x28c]
0x0001b568 cmp r6, 0
0x0001b56c bne 0x1b620
```

文件偏移 `0x1b620–0x1b658` 用接口/地址/名称条件匹配已有 socket；匹配即返回它，而不是再次分配随机端口：

```text
0x0001b620 ldr r3, [r6, 0x34]
0x0001b624 cmp r3, r7
0x0001b628 bne 0x1b654
0x0001b62c mov r1, r4
0x0001b630 add r0, r6, 4
0x0001b634 bl 0xd760
0x0001b638 cmp r0, 0
0x0001b63c beq 0x1b654
0x0001b640 add r1, r6, 0x20
0x0001b644 mov r0, sl
0x0001b648 bl sym.imp.strcmp
0x0001b64c cmp r0, 0
0x0001b650 beq 0x1b608
0x0001b654 ldr r6, [r6, 0x40]
0x0001b658 b 0x1b568
```

新建 socket 成功后，文件偏移 `0x1b710–0x1b720` 将其加入同一个 `+0x28c` 列表。

## 端口实际如何选取

新建 socket 调用 `0x1a22c` 的绑定函数，见 `0x1b668–0x1b680`。绑定函数在输入来源端口为 0 时检查端口范围字段；范围非零时用随机函数在范围内选择，见 `0x1a294–0x1a2e8`：

```text
0x0001a294 cmp r0, 0
0x0001a298 bne 0x1a374
0x0001a2a8 ldr r5, [r3, 0x110]
0x0001a2ac cmp r5, 0
0x0001a2b0 beq 0x1a374
0x0001a2b4 ldr r6, [r3, 0x10c]
0x0001a2d4 bl 0xdfcc
0x0001a2dc bl sym.imp.__aeabi_uidivmod
0x0001a2e0 add r1, r6, r1
0x0001a2e8 bl sym.imp.htons
```

没有范围且来源地址为通配地址时，可不显式 bind（`0x1a300–0x1a310`）；系统在实际发送时选取端口。有绑定来源地址时会以端口 0 进入 bind；见 `0x1a314–0x1a324`。因此精确结论是 **先选择一个来源端口并复用 socket**，而非“每次查询继续采用默认随机端口策略”。

## 可用于目录的 evidence

1. `source: "docs/field-help-queryport-review.md"` 的选项表与分支段，artifact 使用上述 SHA-256、`source: "usr/sbin/dnsmasq"`、`offset: 89592`（`0x15df8`）。fact 应写“query-port=0 分支将共享查询 socket 模式标志设为 1”，不只写字符串存在。
2. 同文档共享 socket 段，artifact 使用上述 SHA-256，偏移 `112160`（`0x1b620`）。fact 写“同地址/接口条件匹配时复用已有共享查询 socket”。
3. 脚本证据继续保留 `etc/init.d/dnsmasq:1083–1085`，明确 0 值不是被启动脚本过滤掉。


## RA 补充：原厂默认路由通告还受 WAN6 状态门控

RA 原始制品：`usr/sbin/odhcpd`，SHA-256 `afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798`，70173 字节，ELF32 little-endian ARM。第一个 PT_LOAD 的文件偏移与虚拟地址均为 0，文件大小为 67424；以下代码地址同时是文件字节偏移。本段同样只做静态反汇编，没有执行固件程序。

**`ra_default=2` 不能解释为“原厂始终向客户端通告有效默认路由”。** 选项参与策略，但最终 Router Lifetime 还可能被厂商 WAN6 检查强制归零。

`ra_default` 在 ELF 名称/类型表的文件偏移 `0x10a50`（68176）对应整数选项。配置解析函数中，栈中第 24 项是该字段；文件偏移 `0x4874–0x4884` 将其读入接口结构的 `+0x1a0`：

```text
0x00004874 ldr r0, [sp, 0x88]
0x00004878 cmp r0, 0
0x0000487c beq 0x4888
0x00004880 bl 0x3a14
0x00004884 str r0, [r4, 0x1a0]
```

生成 RA 的函数在文件偏移 `0x5eac–0x5ed0` 按该值设置默认路由/前缀判定条件。此片段能够证明 0、1、2 参与不同条件，但不能单独把 2 当作最终不可覆盖承诺：

```text
0x00005eac ldr r0, [r4, 0x1a0]
0x00005eb4 cmp r0, 0
0x00005eb8 beq 0x60ac
0x00005ebc cmp r0, 1
0x00005ec0 movle r3, 0
0x00005ec4 movgt r3, 1
0x00005ec8 str r3, [fp, -0x2d0]
0x00005ecc mov r3, 1
0x00005ed0 str r3, [fp, -0x2dc]
```

文件偏移 `0x6540–0x6568` 先把算得的 lifetime 写入 RA 首部偏移 `+6`，然后调用 `system()` 执行厂商检查；返回 0 时将该 lifetime 写成 0：

```text
0x00006540 ldr r3, [fp, -0x2e8]
0x00006544 uxth r0, r3
0x00006548 bl sym.imp.htons
0x0000654c sub r3, fp, 0x2c4
0x00006550 strh r0, [r3, 6]
0x00006554 ldr r0, [0x00006b3c]
0x00006558 add r0, pc, r0
0x0000655c bl sym.imp.system
0x00006560 sub r3, fp, 0x2c4
0x00006564 subs r5, r0, 0
0x00006568 strheq r5, [r3, 6]
```

`0x6b3c` 的值为 `0x9944`，在 `0x6558` 的 PC 相对加法得到字符串文件偏移 `0xfea4`。该字符串是 `/usr/sbin/wan6_link_check.sh`。

可读脚本 `usr/sbin/wan6_link_check.sh:6–23` 初始化 `ipv6_conn_status=0`。它遍历 `ipv6.wan6*` 的 WAN 节点；只有非 dedicated、接口已 up 且存在 IPv6 网关时才改为 1。结尾 `exit "$ipv6_conn_status"`。因此当脚本判定没有符合条件的 IPv6 上联时，返回 0，以上厂商 ELF 分支把 Router Lifetime 归零。归零表示该 RA 不提供有效默认路由，而非关闭所有 RA 前缀/选项。

建议 `ra_default` description：`选择默认路由通告策略。此项不能绕过原厂 WAN6 连通状态检查；检查未发现可用上联时，有效 Router Lifetime 会归零。`

建议 summary：`默认路由通告仍受原厂 WAN6 检查限制；2 不保证始终提供有效默认路由。`

目录 evidence 应同时引用本段 ELF 片段与可读脚本：artifact 的 `source: "usr/sbin/odhcpd"`、上述 SHA-256、`offset: 25920`（`0x6540`）；另引用 `usr/sbin/wan6_link_check.sh:6–23`。

## RA 补充：已取得的实际数值消费者（非仅名称表）

下列为同一个 RN02 原厂 odhcpd 的静态消费者，供定点完善已有 discovery。没有完整追踪全部 DHCPv6 行为。

- `ra_slaac`：配置解析 `0x499c–0x49ac` 从栈第 27 项读取布尔值写至 `+0x185`。RA 前缀生成 `0x64b0–0x64cc` 对前缀长度不大于 64 且此开关为真时置前缀 Autonomous 位 `0x40`；前缀长度大于 64 时不会置该位。不能把开关理解为无需合适前缀也能生成客户端地址。
- `ra_mtu`：解析 `0x4988–0x4998` 将字段写至 `+0x1c4`。RA 生成 `0x5e28–0x5e5c` 在值为 0 时查询接口 MTU；最终发送值至少为 `0x500`（1280），随后 `htonl` 写入 RA MTU 选项。**已证明下限处理，但没有从此片段证明 65535 上限。**
- `ra_mininterval` / `ra_maxinterval`：解析 `0x49ec–0x4a10` 分别写至 `+0x1ac` / `+0x1a8`。定时生成 `0x5f30–0x5fd4` 会结合有效期先收窄 max，再把有效 max 控制在 4–1800 秒；min 不大于 2 时改为 3，过大时也重新计算。配置数值不是直接不变地作为发送间隔。
- `ra_lifetime`：解析 `0x4a14–0x4a24` 写至 `+0x1b0`。生成 `0x600c–0x6044` 对非负值有夹限处理：正值小于有效 max interval 时提升至该间隔，较大的值限制到 9000；0 保持 0。此值之后仍受上述 WAN6 门控。默认负值路径另按有效 max interval 计算，不从此片段推广为固定租期。

以上可用于替换“完全没有取得内部分支”的旧表述，但不应把二进制验证范围扩大到其他未追踪字段。
