# system / Dropbear 字段帮助：静态研究记录

## 边界与清单

- 修改范围仅 `field-help/system.ts`、`field-help/dropbear.ts` 和本文件。共 40 个字段：system.system 13、system.timeserver 5、system.led 9、dropbear.dropbear 13。
- 基线：Xiaomi RN02 1.0.43，静态 rootfs 位于 `/Volumes/RN02_STATIC/rootfs`。未读取私有 UCI 设置值或密钥内容，未执行固件二进制，未 SSH、探测或重载设备。
- 原厂 Lua 已检查反编译文件 `static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQSysUtil.lua` 等；此相对路径按 `/Users/nkanf/docs/miwifibe6500` 解析。
- 父代理提供了只读 1.0.64 公共脚本快照。`etc/init.d/system`、`etc/init.d/dropbear` 与 `lib/netifd/proto/dhcp.sh` 的 SHA 与基线相同；metadata 另引快照的具体行。1.0.64 的 dnsmasq 不同，不能把其整体或其他未抓取消费者视为已验证。
- 原厂 Dropbear `start_service` 的 `ssh_en` / release 双 gate 只适用于该启动路径。be6500panel 独立管理的救援 SSH 属于产品集成上下文，不是从原厂 gate 推导；本帮助不锁字段、不阻止编辑，也不声称这些原厂 UCI 项必定重配救援实例。

## 正向消费链

- system init 校验 hostname/conloglevel/timezone/zonename，设置内核名称、dmesg 和时区。hostname 在特定 dnsmasq 路由模式中还用于本机记录；WAN DHCP 名称另取硬件配置。
- 厂商 timezone init 可按地区或 timezoneindex/webtimezone 重写 POSIX 时区。zonename 只有对应 zoneinfo 文件存在才建立 localtime 链接。
- cron init 读取首个 system 章节的 cronloglevel，命令参数空值回退 5，未注册 system 配置重载触发器。
- ntpsetclock 只读取命名 system.ntp 的 enabled/server。server 为空时写入源码中的五个 pool 主机；随后以 -p 列表运行 ntpd，失败再尝试 HTTP 时间源。官方 XQSysUtil 用 get_list/set_list 操作同一字段，并可通过时间模式改变 enabled。
- 原厂灯服务读取 xqled JSON/profile；GPIO 后端读取 xqled.trigger 与 msec_on/msec_off，sysfs 后端读取 xqled 的 led/option 动作。通用 uci-defaults LED helper 只生成板级 JSON，不能冒认 system.led 的运行时消费。
- 原厂 Dropbear 校验 12 个目录字段，生成 -p/-s/-g/-w/-a/-I/-K/-T/-b 参数或 mDNS/enable 行为。目录 keyfile 的原厂读取未找到；其实际字段为 rsakeyfile。

## ELF 选项证据（仅能力，不单独证明 UCI 读取）

提取方法：Python 按原始 ELF 字节匹配可打印 ASCII 与制表符，记录零基 byte offset。下表将制表符展示为 `\t`。`%d` 仍是格式占位符，不能据此推导编译缺省或最大次数。只把主机密钥路径当作路径，不读取密钥。

| 标识 | artifact | SHA-256 | byte offset | 原始可读字符串（转义制表符） |
| --- | --- | --- | ---: | --- |
| dropbear:port | `usr/sbin/dropbear` | `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2` | `153239` (`0x25697`) | `-p [address:]port` |
| dropbear:port_desc | `usr/sbin/dropbear` | `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2` | `153257` (`0x256a9`) | `\t\tListen on specified tcp port (and optionally address),` |
| dropbear:password | `usr/sbin/dropbear` | `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2` | `152962` (`0x25582`) | `-s\t\tDisable password logins` |
| dropbear:rootpassword | `usr/sbin/dropbear` | `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2` | `152990` (`0x2559e`) | `-g\t\tDisable password logins for root` |
| dropbear:rootlogin | `usr/sbin/dropbear` | `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2` | `152937` (`0x25569`) | `-w\t\tDisallow root logins` |
| dropbear:gateway | `usr/sbin/dropbear` | `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2` | `153184` (`0x25660`) | `-a\t\tAllow connections to forwarded ports from any host` |
| dropbear:idle | `usr/sbin/dropbear` | `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2` | `153557` (`0x257d5`) | `-I <idle_timeout>  (0 is never, default %d, in seconds)` |
| dropbear:keepalive | `usr/sbin/dropbear` | `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2` | `153504` (`0x257a0`) | `-K <keepalive>  (0 is never, default %d, in seconds)` |
| dropbear:authtries | `usr/sbin/dropbear` | `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2` | `153059` (`0x255e3`) | `-T <1 to %d> \tMaximum authentication tries (default %d)` |
| dropbear:banner | `usr/sbin/dropbear` | `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2` | `152688` (`0x25470`) | `-b bannerfile\tDisplay the contents of bannerfile before user login` |
| dropbear:banner_desc | `usr/sbin/dropbear` | `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2` | `152688` (`0x25470`) | `-b bannerfile\tDisplay the contents of bannerfile before user login` |
| dropbear:hostkey | `usr/sbin/dropbear` | `f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2` | `152773` (`0x254c5`) | `-r keyfile  Specify hostkeys (repeatable)` |
| led_srv:config | `usr/sbin/led_srv` | `4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733` | `18932` (`0x49f4`) | ` -c <config>:          Config file` |
| led_srv:profile | `usr/sbin/led_srv` | `4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733` | `18967` (`0x4a17`) | ` -p <profile>:         Config profile` |
| led_srv:status | `usr/sbin/led_srv` | `4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733` | `19005` (`0x4a3d`) | ` -s <status>:          Led status (0: off, 1: on, default 1)` |

## 逐项未证实范围

静态检索覆盖 rootfs 的 etc/init.d、etc/hotplug.d、etc/syslog-ng.d、lib、usr/share、usr/lib/lua、usr/sbin、usr/bin，以及其余可读取文本脚本和已有反编译 Lua。过滤私有 etc/config、密钥与证书，排除二进制字节内容作为普通源码。对原厂 ELF 另外检查固定字符串；缺少字符串不是“不支持”的全局证明。

| 目录字段 | 具体检索消费者与结果 |
| --- | --- |
| system.system.description | system init 未读；XQSysUtil 的 description 命中是 `/tmp/backup_files_status` 产生的备份描述，不是设备描述。 |
| system.system.notes | system init、lib 与反编译 Lua 未找到精确 notes 消费。 |
| system.system.log_size | syslog-ng/system/miwifi-logd init、日志配置和 Lua 未读；syslog-ng 独立配置的 log_fifo_size/log_msg_size 不能作为此项缺省。 |
| system.system.log_ip | 同上未读；配置回环 UDP 地址是接收源，不能冒认远程发送目标。 |
| system.system.log_port | 同上未读；接收源的 514 不是此 UCI 发送项缺省。 |
| system.system.log_proto | 同上未读；本地 UDP 接收不能证明 UCI tcp/udp 选择生效。 |
| system.system.log_remote | 同上未读；miwifi-logd 按 NETMODE 启动，监听 xiaoqiang/milog 重载，而不是本开关。 |
| system.system.log_file | lib.scripthelper 的 system.log_file 读取被注释；syslog-ng 的 `/tmp/messages` 来自独立配置，不是此字段回退。 |
| system.timeserver.enable_server | ntpsetclock 命令构建未读；基线没有 sysntpd init。未找到原厂 Lua 读取。 |
| system.timeserver.use_dhcp | DHCP 脚本上报 ntpserver/timeserver 元数据；未找到送入 ntpsetclock 的消费者。WAN IPv6 的同名 use_dhcp 不是本项。 |
| system.timeserver.interface | ntpsetclock/init/lib/反编译 XQSysUtil 未读取该 NTP 绑定字段。 |
| system.led.name | helper 能写板级 JSON name；xqled 原厂启动链未找到 system.led name 消费。 |
| system.led.sysfs | helper 能写板级 JSON sysfs；xqled sysfs 动作用 led 字段，未证实别名。 |
| system.led.default | helper 能写 default；原厂 BLUE_LED/xqled profile 是另一套控制，未证实本字段。 |
| system.led.trigger | 通用 helper 写 trigger，厂商另有 blink/on/off；未找到 system.led 读取。 |
| system.led.dev | netdev helper 写 device，未发现将 dev UCI 项绑定到运行 LED 的加载器。 |
| system.led.mode | netdev helper 有参数回退 link tx rx，但未证实此 UCI 项读取；不将参数回退登记为字段缺省。 |
| system.led.delayon | helper 写板级 JSON；厂商用 msec_on，未证实别名。 |
| system.led.delayoff | helper 写板级 JSON；厂商用 msec_off，未证实别名。 |
| system.led.interval | 通用 netdev helper 未输出 interval；xqled/init/Lua 未找到此 UCI 消费或单位转换。 |
| dropbear.dropbear.keyfile | init 校验/变量/命令生成均用 rsakeyfile；ELF 的 -r keyfile 只证明 CLI 支持，未证实 UCI keyfile。 |

共 19 个字段有原厂读取证据（其中 keyfile 不计入），21 个目录字段保留具体 discovery。LED 通用约定和协议约束注明其来源性质；不添加未经源码证明的缺省值。

## BusyBox 压缩帮助与日志库解析器的追加证据

`bin/busybox` 与 `usr/sbin/ntpd` 的 ELF 字节相同，SHA-256 `d9fa606a3c706a56bfd4f2e7d1c538a2f0ae132b016cd5392004b93123d46234`。从 ELF byte offset `443357` (`0x6c3dd`) 读取 `h1` bzip2 流，为解压器补上省略的 `BZ` 前缀；解压后按 NUL 分隔帮助块。以下 offset 字段是 ELF 内压缩流起点，行内另记解压帮助块 offset。不执行 ARM 程序。

| 标识 | artifact | SHA-256 | ELF stream offset | 解压帮助 block offset | 确切帮助内容（换行/制表符转义） |
| --- | --- | --- | ---: | ---: | --- |
| busybox:cron | `bin/busybox` | `d9fa606a3c706a56bfd4f2e7d1c538a2f0ae132b016cd5392004b93123d46234` | `443357` | `2328` | `[-fbS] [-l N] [-L LOGFILE] [-c DIR]\n\n\t-f\tForeground\n\t-b\tBackground (default)\n\t-S\tLog to syslog (default)\n\t-l N\tSet log level. Most verbose 0, default 8\n\t-L FILE\tLog to FILE\n\t-c DIR\tCron dir. Default:/etc/crontabs` |
| busybox:ntp | `bin/busybox` | `d9fa606a3c706a56bfd4f2e7d1c538a2f0ae132b016cd5392004b93123d46234` | `443357` | `20160` | `[-dnqNwl] [-I IFACE] [-S PROG] [-p PEER]...\n\nNTP client/server\n\n\t-d[d]\tVerbose\n\t-n\tRun in foreground\n\t-q\tQuit after clock is set\n\t-N\tRun at high priority\n\t-w\tDo not set time (only query peers), implies -n\n\t-S PROG\tRun PROG after stepping time, stratum change, and every 11 min\n\t-p PEER\tObtain time from PEER (may be repeated)\n\t-l\tAlso run as server on port 123\n\t-I IFACE Bind server to IFACE, implies -l` |
| busybox:dmesg | `bin/busybox` | `d9fa606a3c706a56bfd4f2e7d1c538a2f0ae132b016cd5392004b93123d46234` | `443357` | `5231` | `[-cr] [-n LEVEL] [-s SIZE]\n\nPrint or control the kernel ring buffer\n\n\t-c\t\tClear ring buffer after printing\n\t-n LEVEL\tSet console logging level\n\t-s SIZE\t\tBuffer size\n\t-r\t\tPrint raw message buffer` |

crond 编译帮助的 default 8 不是 UCI 缺省：etc/init.d/cron 的 -l 参数空值回退 5，因此 metadata 只登记 5。ntpd 的 -l/-I 能力同样不能证明 system.ntp.enable_server/interface 已接线；已确认的 ntpsetclock 命令没有生成这些参数。

| 标识 | artifact | SHA-256 | byte offset | 原始可读字符串 |
| --- | --- | --- | ---: | --- |
| syslog-lib:config | `usr/lib/libsyslog-ng-3.5.6.so` | `2c514cedebfc5032e75ab49a02c2866b46a37a91808d07583d6c9774f33b9765` | `331073` (`0x50d41`) | `Set config file name, default=/etc/syslog-ng.conf` |
| syslog-lib:fifo | `usr/lib/libsyslog-ng-3.5.6.so` | `2c514cedebfc5032e75ab49a02c2866b46a37a91808d07583d6c9774f33b9765` | `320977` (`0x4e5d1`) | `log_fifo_size` |
| syslog-lib:msgsize | `usr/lib/libsyslog-ng-3.5.6.so` | `2c514cedebfc5032e75ab49a02c2866b46a37a91808d07583d6c9774f33b9765` | `321019` (`0x4e5fb`) | `log_msg_size` |
| syslog-lib:tcp | `usr/lib/libsyslog-ng-3.5.6.so` | `2c514cedebfc5032e75ab49a02c2866b46a37a91808d07583d6c9774f33b9765` | `335753` (`0x51f89`) | `tcp6` |
| syslog-lib:udp | `usr/lib/libsyslog-ng-3.5.6.so` | `2c514cedebfc5032e75ab49a02c2866b46a37a91808d07583d6c9774f33b9765` | `335758` (`0x51f8e`) | `udp6` |
| syslog-lib:file | `usr/lib/libsyslog-ng-3.5.6.so` | `2c514cedebfc5032e75ab49a02c2866b46a37a91808d07583d6c9774f33b9765` | `335763` (`0x51f93`) | `unix-stream` |

日志解析库的 log_fifo_size、log_msg_size、tcp6、udp6、unix-stream 字符串证明配置解析器含这些词法 token，不单独证明对应远程目标是否被启用，也不证明 system.log_* UCI 字段支持。原厂实际服务路径仍以 etc/init.d/syslog-ng 与 etc/syslog-ng.conf 的正向读写为准。
- 额外检查 `usr/sbin/syslog-ng`、`usr/lib/libsyslog-ng-3.5.6.so`、`usr/sbin/miwifi-logd`、`usr/sbin/led_srv`、`usr/sbin/ntpd` 的可读固定字符串，没有精确 log_size/log_ip/log_port/log_proto/log_remote/log_file。该有限 ELF 字符串发现只补充文本搜索边界，不作为不可编辑或固件绝不支持的结论。
