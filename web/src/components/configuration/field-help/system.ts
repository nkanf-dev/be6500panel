import type { ModuleFieldHelp } from "./types";

// Field-specific source tracing; unknown vendor consumers are recorded in discovery.
export const systemFieldHelp: ModuleFieldHelp = {
  system: {
    hostname: {
      description:
        "设置路由器的系统主机名。原厂写入内核主机名，但不会改无线 SSID，也不决定 WAN DHCP 客户端发送的名称。",
      defaultValue: "OpenWrt（system 启动校验缺省）",
      dependencies: [
        "本机 DNS 名称还取决于 dnsmasq.add_local_hostname、LAN 地址和运行模式。",
      ],
      impact:
        "system 重载会写入内核主机名。dnsmasq 在路由模式且启用 add_local_hostname 时还用它生成本机 DNS 记录，需其重新加载才更新。",
      evidence: [
        {
          source: "etc/init.d/system",
          line: 9,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "system 校验器将 hostname 缺省设为 OpenWrt。",
          endLine: 10,
        },
        {
          source: "etc/init.d/system",
          line: 37,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "hostname 写入 /proc/sys/kernel/hostname。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 1289,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "非 AP/中继模式且 add_local_hostname 生效时，以 system 主机名生成本机记录。",
          endLine: 1294,
        },
        {
          source: "lib/netifd/proto/dhcp.sh",
          line: 47,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "WAN DHCP 主机名另取 misc.hardware.dhcp_hostname，缺省由硬件型号生成。",
          endLine: 52,
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/system",
          line: 37,
          endLine: 37,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "1.0.64 system 脚本同样将 hostname 写入内核主机名文件。",
        },
      ],
    },
    timezone: {
      description:
        "设置系统时间显示和本地定时任务使用的 POSIX 时区。原厂写入 /tmp/TZ；这是时区规则，不是 NTP 服务器地址。",
      defaultValue:
        "UTC（system 校验与 timezone 脚本回退；官方界面另有显示回退）",
      dependencies: [
        "有效 zonename 对应文件会替代 /tmp/TZ。",
        "厂商 timezoneindex、webtimezone、初始化状态及地区映射可能覆盖 timezone。",
      ],
      flags: ["version-dependent"],
      impact:
        "system 重载会设置用户空间和内核时区。厂商 timezone 服务也会根据地区或官方界面选择重写此字段；不要只把它当作显示标签。",
      evidence: [
        {
          source: "etc/init.d/system",
          line: 13,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "timezone 校验缺省为 UTC。",
        },
        {
          source: "etc/init.d/system",
          line: 39,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "timezone 写入 /tmp/TZ；有效 zonename 时改用 /tmp/localtime。",
          endLine: 46,
        },
        {
          source: "etc/init.d/timezone",
          line: 24,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "按地区获得的 tz 写入 /tmp/TZ，并提交到 system.timezone。",
          endLine: 29,
        },
        {
          source: "etc/init.d/timezone",
          line: 48,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "timezoneindex、初始化状态与 webtimezone 决定使用地区时区还是已保存 timezone。",
          endLine: 73,
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQSysUtil.lua",
          line: 6072,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商设置流程同时写 timezone、webtimezone、timezoneindex，随后重启 timezone 服务。",
          endLine: 6112,
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/system",
          line: 39,
          endLine: 46,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "1.0.64 system 脚本同样写 /tmp/TZ、在有效 zonename 时建 localtime 链接并应用内核时区。",
        },
      ],
    },
    zonename: {
      description:
        "时区数据库文件名，例如 IANA 区域名称。仅当 /usr/share/zoneinfo 下的对应文件存在时使用；不是自动转换 timezone 的输入。",
      dependencies: [
        "/usr/share/zoneinfo/<zonename> 必须存在。",
        "与 timezone 一起保存；二者不是同一个字段。",
      ],
      flags: ["version-dependent"],
      impact:
        "有效文件会链接为 /tmp/localtime 并移除 /tmp/TZ；缺少文件时继续使用 POSIX timezone。实际可用区域取决于固件中安装的 zoneinfo。",
      evidence: [
        {
          source: "etc/init.d/system",
          line: 14,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "zonename 校验为字符串，没有指定回退值。",
        },
        {
          source: "etc/init.d/system",
          line: 39,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "先写 /tmp/TZ；非空 zonename 且 zoneinfo 文件存在时建立 /tmp/localtime 链接并删除 /tmp/TZ。",
          endLine: 43,
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/system",
          line: 40,
          endLine: 41,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "1.0.64 脚本同样仅在指定 zoneinfo 文件存在时建立 /tmp/localtime 链接。",
        },
      ],
    },
    description: {
      description:
        "系统描述文本，与 hostname 的内核名称作用不同；1.0.43 未证实原厂服务使用此字段。",
      flags: ["version-dependent"],
      discovery:
        "已检索 etc/init.d、lib、原厂 Lua 和反编译 XQSysUtil 的 system 读写，未找到 system 类型章节的 description 读取；XQSysUtil 同名项是备份状态描述，不是此字段。",
      impact:
        "可保存设备说明；不能据此断言会改变官方设备名称、发现广播或主机名。已检查的 system 启动路径不展示描述。",
      evidence: [
        {
          source: "etc/init.d/system",
          line: 9,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "system 校验声明 hostname、conloglevel、buffersize、timezone 和 zonename。",
          endLine: 14,
        },
        {
          source: "etc/init.d/system",
          line: 37,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "此启动路径写内核主机名、dmesg 参数和时区文件。",
          endLine: 43,
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQSysUtil.lua",
          line: 5430,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "XQSysUtil 的 description 命中属于备份状态输出，状态来自 /tmp/backup_files_status。",
          endLine: 5435,
        },
      ],
      summary: "系统描述文本；1.0.43 未找到原厂 system.description 消费。",
    },
    notes: {
      description: "设备备注文本；1.0.43 未证实原厂运行服务读取此备注。",
      flags: ["version-dependent"],
      discovery:
        "已检索 system init、lib 脚本、原厂 Lua 与反编译 XQSysUtil，未找到精确 notes 字段或 system.notes 消费；备注语义来自现有字段定义，不作为原厂支持结论。",
      impact:
        "编辑可保留管理记录，但未确认官方界面会显示，也未确认它会改变服务启动或系统标识。",
      evidence: [
        {
          source: "etc/init.d/system",
          line: 9,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "system 校验器列出启动所需的五个配置字段。",
          endLine: 14,
        },
        {
          source: "etc/init.d/system",
          line: 51,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "reload_service 加载 system 并只遍历 system 类型章节执行 system_config。",
          endLine: 54,
        },
      ],
      summary: "设备备注文本；1.0.43 未找到原厂 system.notes 消费。",
    },
    log_size: {
      description:
        "通用系统日志缓冲区大小，界面以 KiB 表示。1.0.43 未证实用此 UCI 字段配置原厂 syslog-ng。",
      unit: "KiB（现有字段约定）",
      flags: ["version-dependent"],
      discovery:
        "已检索 system、syslog-ng、miwifi-logd init、lib 脚本及反编译 Lua，未找到精确 log_size 的读取或 UCI 到 syslog-ng 的转换。不能从 syslog-ng 的队列参数推导本字段缺省。",
      impact:
        "不能保证扩大内存日志容量。原厂 syslog-ng 从独立配置读取队列和消息大小，不等同于此字段；实际内存影响未验证。",
      evidence: [
        {
          source: "etc/init.d/syslog-ng",
          line: 10,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "syslog-ng 服务使用 /etc/syslog-ng.conf，启动前检查该文件与语法。",
          endLine: 18,
        },
        {
          source: "etc/syslog-ng.conf",
          line: 9,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "syslog-ng 配置直接指定 log_fifo_size 和 log_msg_size。",
          endLine: 10,
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 87,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 syslog-ng 动态解析库含 log_fifo_size token；它与配置中的 log_msg_size 都不等于 UCI log_size。",
          artifact: {
            source: "usr/lib/libsyslog-ng-3.5.6.so",
            sha256:
              "2c514cedebfc5032e75ab49a02c2866b46a37a91808d07583d6c9774f33b9765",
            offset: 320977,
          },
        },
      ],
      summary:
        "通用日志缓冲大小（KiB）；1.0.43 未证实 log_size 接入 syslog-ng。",
    },
    log_ip: {
      description:
        "远程 syslog 目标地址或主机名；是通用字段语义，1.0.43 未找到原厂发送路径读取它。",
      dependencies: [
        "若其他日志实现支持它，通常还需 log_remote、log_proto 和 log_port；原厂联动未证实。",
      ],
      flags: ["version-dependent"],
      discovery:
        "已检索 syslog-ng init、etc/syslog-ng.conf 与其包含目录、miwifi-logd init、lib 和反编译 Lua，未找到 system.log_ip 读取或远程目标生成。",
      impact:
        "更改此地址不保证日志外发。原厂配置中的 127.0.0.1 UDP 入口是接收源，不是此远程目标。",
      evidence: [
        {
          source: "etc/init.d/syslog-ng",
          line: 11,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂日志服务配置文件为 /etc/syslog-ng.conf。",
        },
        {
          source: "etc/syslog-ng.conf",
          line: 24,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "net 日志源在回环地址监听 UDP。",
          endLine: 26,
        },
        {
          source: "etc/syslog-ng.conf",
          line: 45,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "日志流汇集 src、net、kernel 源，输出到 d_messages。",
          endLine: 51,
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 86,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 syslog-ng 解析库的 cfgfile 帮助指向 /etc/syslog-ng.conf；地址配置仍属于该独立文件。",
          artifact: {
            source: "usr/lib/libsyslog-ng-3.5.6.so",
            sha256:
              "2c514cedebfc5032e75ab49a02c2866b46a37a91808d07583d6c9774f33b9765",
            offset: 331073,
          },
        },
      ],
      summary: "通用远程 syslog 目标；1.0.43 未找到 log_ip 的原厂发送路径。",
    },
    log_port: {
      description:
        "远程 syslog 目标端口；不要与原厂回环 UDP 日志接收端口混为一谈。",
      range: "1–65535（传输层端口约束）",
      dependencies: [
        "远程目标需与 log_ip、log_proto、log_remote 配合；原厂是否读取未证实。",
      ],
      flags: ["version-dependent"],
      discovery:
        "已检索 syslog-ng init/配置/包含目录、lib 和反编译 Lua，未找到精确 log_port 读取。配置中的监听 514 不能用作此 UCI 字段的缺省。",
      impact:
        "1.0.43 未证实此字段改变日志目的端口。即使保存合法端口，也不代表远程日志发送服务已启用。",
      evidence: [
        {
          source: "etc/syslog-ng.conf",
          line: 24,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "配置中的 UDP 514 属于 net 日志接收源。",
          endLine: 26,
        },
        {
          source: "etc/init.d/syslog-ng",
          line: 21,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "syslog-ng 由 procd 启动，命令未携带 UCI 远程端口参数。",
          endLine: 25,
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 86,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 syslog-ng 解析库默认使用 /etc/syslog-ng.conf；本证据不指定 UCI 远程端口缺省。",
          artifact: {
            source: "usr/lib/libsyslog-ng-3.5.6.so",
            sha256:
              "2c514cedebfc5032e75ab49a02c2866b46a37a91808d07583d6c9774f33b9765",
            offset: 331073,
          },
        },
      ],
      summary: "通用远程日志端口；原厂回环 UDP 514 接收源不是本项缺省。",
    },
    log_proto: {
      description:
        "远程日志传输协议，字段定义支持 udp 或 tcp；1.0.43 未证实此项会切换原厂日志输出。",
      range: "udp / tcp（现有字段约定）",
      dependencies: ["需要支持此项的发送服务和可达 log_ip；原厂支持未确认。"],
      flags: ["version-dependent"],
      discovery:
        "已检索 system/syslog-ng/miwifi-logd init、syslog-ng 配置目录、lib 和反编译 Lua，未找到精确 log_proto 读取或根据它生成 tcp/udp 目的地。",
      impact:
        "不保证建立 TCP 连接或改用 UDP 发送。原厂配置里的 UDP 是本地接收源，不能据此推断远程发送协议。",
      evidence: [
        {
          source: "etc/syslog-ng.conf",
          line: 24,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "net 源使用 udp(ip(127.0.0.1) port(514)) 接收日志。",
          endLine: 26,
        },
        {
          source: "etc/syslog-ng.conf",
          line: 32,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "d_messages 是 /tmp/messages 文件目的地。",
          endLine: 34,
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 89,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "syslog-ng 解析库含 tcp6 token，证明解析器能力而非 system.log_proto 消费或运行目标。",
          artifact: {
            source: "usr/lib/libsyslog-ng-3.5.6.so",
            sha256:
              "2c514cedebfc5032e75ab49a02c2866b46a37a91808d07583d6c9774f33b9765",
            offset: 335753,
          },
        },
      ],
      summary: "通用 UDP/TCP 发送选择；1.0.43 未证实原厂读取 log_proto。",
    },
    log_remote: {
      description: "通用远程日志开关；不同于原厂 miwifi-logd 的启用条件。",
      dependencies: [
        "log_ip、log_port、log_proto 只有在消费者支持时才决定远程发送。",
      ],
      flags: ["version-dependent"],
      discovery:
        "已检索 system、syslog-ng、miwifi-logd init、lib 与反编译 Lua，未找到精确 log_remote 的读取；不能将厂商日志服务的运行条件当作此开关实现。",
      impact:
        "不能保证开启后外发日志，也不能保证关闭后停止厂商日志服务。miwifi-logd 的已知启动条件读取 NETMODE，配置触发器监听 xiaoqiang 与 milog。",
      evidence: [
        {
          source: "etc/init.d/miwifi-logd",
          line: 9,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "miwifi-logd 仅在 NETMODE 等于 whc_cap 时启动，命令不携带 system 日志参数。",
          endLine: 16,
        },
        {
          source: "etc/init.d/miwifi-logd",
          line: 19,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "该服务的重载触发器是 xiaoqiang 和 milog。",
          endLine: 22,
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 90,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "syslog-ng 解析库含 udp6 token，不证明 UCI log_remote 驱动远程发送。",
          artifact: {
            source: "usr/lib/libsyslog-ng-3.5.6.so",
            sha256:
              "2c514cedebfc5032e75ab49a02c2866b46a37a91808d07583d6c9774f33b9765",
            offset: 335758,
          },
        },
      ],
      summary: "通用远程日志开关；原厂 miwifi-logd 的启动条件不是本项。",
    },
    log_file: {
      description:
        "通用本地日志文件路径。1.0.43 活跃 syslog-ng 配置直接指定文件，未证实使用此 UCI 路径。",
      flags: ["legacy", "version-dependent"],
      discovery:
        "已检索 syslog-ng init/配置、lib/lib.scripthelper.sh 和反编译 Lua，system.log_file 读取只见于注释；qcawificfg80211 的同名 shell 变量是独立调试文件，不是本字段。",
      impact:
        "编辑不能保证改变 /tmp/messages 的输出位置。脚本库中读取 log_file 的旧代码已注释，不会创建此路径或为它配置轮转。",
      evidence: [
        {
          source: "etc/syslog-ng.conf",
          line: 32,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 d_messages 直接输出 /tmp/messages。",
          endLine: 34,
        },
        {
          source: "lib/lib.scripthelper.sh",
          line: 38,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "DAEMONSYSLOGFILE 回退为 syslog；从 system 取 log_file 的旧代码及创建目录代码均已注释。",
          endLine: 48,
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 86,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "syslog-ng 解析库默认配置文件为 /etc/syslog-ng.conf；运行文件目的地需看该配置，不证明 UCI log_file 读取。",
          artifact: {
            source: "usr/lib/libsyslog-ng-3.5.6.so",
            sha256:
              "2c514cedebfc5032e75ab49a02c2866b46a37a91808d07583d6c9774f33b9765",
            offset: 331073,
          },
        },
      ],
      summary:
        "通用日志路径；原厂相关读取仅见注释，syslog-ng 直接配置文件目的地。",
    },
    conloglevel: {
      description:
        "内核控制台打印级别，加载时传给 dmesg -n；不设置用户空间 syslog 的过滤级别。",
      range: "0–8（内核控制台级别约定；init 仅校验非负整数）",
      dependencies: [
        "system 重载时生效；与 buffersize 共同决定是否执行 dmesg。",
      ],
      impact:
        "改变控制台上出现的内核消息数量。级别较高通常更详细；不会清空已有日志，也不等于更改远程日志策略。",
      evidence: [
        {
          source: "etc/init.d/system",
          line: 11,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "conloglevel 校验为非负整数，无指定缺省。",
        },
        {
          source: "etc/init.d/system",
          line: 38,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "存在 conloglevel 时将其传入 dmesg -n；两项均为空时跳过 dmesg。",
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/system",
          line: 38,
          endLine: 38,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "1.0.64 system 脚本同样通过 dmesg -n 应用 conloglevel。",
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 80,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "BusyBox 解压帮助将 dmesg -n LEVEL 解释为设置控制台日志级别，区别于 ring buffer 大小。",
          artifact: {
            source: "bin/busybox",
            sha256:
              "d9fa606a3c706a56bfd4f2e7d1c538a2f0ae132b016cd5392004b93123d46234",
            offset: 443357,
          },
        },
      ],
    },
    cronloglevel: {
      description: "crond 的日志详细程度，传给 -l；与内核控制台日志级别分开。",
      defaultValue: "5（crond 启动参数回退）",
      range: "非负整数；界面提供 0–8，init 未规定该上限",
      dependencies: ["/etc/crontabs 中必须有任务；由 cron 启动读取。"],
      impact:
        "下次 cron 启动时改变定时任务日志量，不改变任务周期。数值较小通常更详细；cron 没有注册 system 配置重载触发器，不能保证单独重载 system 即更新。",
      evidence: [
        {
          source: "etc/init.d/cron",
          line: 15,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "没有 crontab 时不启动；读取第一个 system 章节的 cronloglevel 并校验非负整数。",
          endLine: 25,
        },
        {
          source: "etc/init.d/cron",
          line: 30,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "crond 使用 -l 参数，空值回退为 5。",
          endLine: 35,
        },
        {
          source: "etc/init.d/cron",
          line: 39,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "service_triggers 仅注册校验函数。",
          endLine: 41,
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 78,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "BusyBox crond 帮助说明 -l 设置日志级别，0 最详细，帮助中的编译默认是 8。",
          artifact: {
            source: "bin/busybox",
            sha256:
              "d9fa606a3c706a56bfd4f2e7d1c538a2f0ae132b016cd5392004b93123d46234",
            offset: 443357,
          },
        },
      ],
    },
  },
  timeserver: {
    enabled: {
      description:
        "控制厂商自动校时脚本；该脚本只读取名为 ntp 的章节，值为 0 时直接退出。",
      defaultValue: "未设置时继续尝试（脚本仅对 0 退出）",
      dependencies: [
        "原厂读取 system.ntp.enabled；其他命名 timeserver 章节未证实被遍历。",
        "需要已初始化且网络可用；厂商 timemode 可能重写此项。",
      ],
      flags: ["version-dependent"],
      impact:
        "停用会跳过以后的 ntpsetclock 调用，不会回退已经设置的时间。脚本还受初始化、联网配置和连通性检查限制；官方自动/手动时间模式可重写此项。",
      evidence: [
        {
          source: "usr/sbin/ntpsetclock",
          line: 95,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "读取 system.ntp.enabled，仅值为 0 时退出。",
          endLine: 96,
        },
        {
          source: "usr/sbin/ntpsetclock",
          line: 105,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "校时前检查初始化、联网配置与网络就绪；失败则退出。",
          endLine: 122,
        },
        {
          source: "etc/crontabs/root",
          line: 3,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "定时任务每 15 分钟调用 ntpsetclock。",
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQSysUtil.lua",
          line: 7406,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "自动模式将 system.ntp.enabled 设为 1 并触发校时，其他模式将其设为 0。",
          endLine: 7436,
        },
      ],
      summary: "原厂只读取命名 ntp 章节；enabled=0 跳过自动校时。",
    },
    enable_server: {
      description:
        "通用 NTP 对外服务开关；与本机自动校时 enabled 不同。原厂 ntpd 支持 -l 服务模式，但未证实此 UCI 字段生成该参数。",
      dependencies: [
        "若服务实现支持，对外校时还需监听接口和防火墙允许 UDP 123；原厂实现未确认。",
      ],
      flags: ["version-dependent"],
      discovery:
        "已检索 ntpsetclock、etc/init.d（该静态固件无 sysntpd 文件）、lib/netifd、原厂及反编译 XQSysUtil，未找到精确 system.ntp.enable_server 读取。客户端命令不能证明本字段支持。",
      impact:
        "不能保证开启后让局域网设备从本机同步。已确认的厂商流程以单次客户端模式运行 ntpd，不是根据此字段建立常驻 NTP 服务器。",
      evidence: [
        {
          source: "usr/sbin/ntpsetclock",
          line: 38,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "ntp_sync 构建 /usr/sbin/ntpd -N -q -n -4 客户端命令。",
          endLine: 43,
        },
        {
          source: "usr/sbin/ntpsetclock",
          line: 51,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "每个保存服务器附加 -p，再执行命令；该构建路径没有 enable_server 配置。",
          endLine: 55,
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 79,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "BusyBox ntpd 帮助说明 -q 校时后退出、-l 同时作为 123 端口服务器；这是命令行能力。",
          artifact: {
            source: "bin/busybox",
            sha256:
              "d9fa606a3c706a56bfd4f2e7d1c538a2f0ae132b016cd5392004b93123d46234",
            offset: 443357,
          },
        },
      ],
      summary:
        "通用 NTP 对外服务开关；原厂仅证实单次客户端校时，未证实读取本项。",
    },
    server: {
      description:
        "本机校时使用的 NTP 服务器列表。原厂读取 system.ntp.server，每项生成 ntpd -p；通过 -q 单次校时，不是长期对外服务。",
      defaultValue:
        "0.pool.ntp.org、1.pool.ntp.org、2.pool.ntp.org、3.pool.ntp.org、0.cn.pool.ntp.org（空列表时脚本回退）",
      dependencies: [
        "必须位于名为 ntp 的章节；enabled 不能为 0，且网络需通过检查。",
        "主机名需要可用 DNS；保持原生列表格式，每项一个服务器。",
      ],
      flags: ["version-dependent"],
      impact:
        "下次校时使用新服务器；不能保证立即改时。ntpd 失败后厂商脚本尝试 HTTP 时间源；官方管理接口也能替换此列表并主动调用校时。",
      evidence: [
        {
          source: "usr/sbin/ntpsetclock",
          line: 38,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "空 system.ntp.server 回退到脚本定义的五个 pool 主机，并保存该回退列表。",
          endLine: 49,
        },
        {
          source: "usr/sbin/ntpsetclock",
          line: 51,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "遍历服务器，为 ntpd 加 -p 参数并执行。",
          endLine: 55,
        },
        {
          source: "usr/sbin/ntpsetclock",
          line: 154,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "NTP 失败时尝试 htp_sync。",
          endLine: 160,
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQSysUtil.lua",
          line: 7266,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "getNTPServerList 使用 get_list 读取 system.ntp.server。",
          endLine: 7275,
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQSysUtil.lua",
          line: 7333,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "设置流程删除旧 server、写入新列表并提交，再调用 ntpsetclock now。",
          endLine: 7357,
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 79,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "BusyBox ntpd 帮助说明可重复的 -p PEER 从指定服务器获取时间，-q 在设时后退出。",
          artifact: {
            source: "bin/busybox",
            sha256:
              "d9fa606a3c706a56bfd4f2e7d1c538a2f0ae132b016cd5392004b93123d46234",
            offset: 443357,
          },
        },
      ],
      summary: "原厂读取 system.ntp.server 列表；其他命名章节未证实参与校时。",
    },
    use_dhcp: {
      description:
        "通用“采用 DHCP 下发时间服务器”开关。DHCP 客户端确实上报 ntpserver/timeserver，但未证实此开关连接到厂商校时。",
      dependencies: [
        "上游 DHCP 必须提供时间服务器；还需有将接口数据送入校时服务的消费者。",
      ],
      flags: ["version-dependent"],
      discovery:
        "已检索 ntpsetclock、init、lib/netifd/dhcp.script、原厂与反编译 Lua，未找到 timeserver.use_dhcp 或 system.ntp.use_dhcp 读取；XQLanWanUtil 的同名 use_dhcp 属于 WAN IPv6 配置，不是此开关。",
      impact:
        "不能保证开启后自动采用上游 NTP 地址。已确认的 ntpsetclock 仍取 system.ntp.server，未在该路径读取 DHCP 元数据。",
      evidence: [
        {
          source: "lib/netifd/dhcp.script",
          line: 117,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "DHCP 结果将 ntpsrv 与 timesvr 写入接口数据的 ntpserver/timeserver 字段。",
          endLine: 120,
        },
        {
          source: "usr/sbin/ntpsetclock",
          line: 41,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商脚本从 system.ntp.server 读取校时服务器，空值才用其内置列表。",
          endLine: 49,
        },
      ],
      summary: "DHCP 会上报时间服务器，但未证实本项会将其用于原厂校时。",
    },
    interface: {
      description:
        "通用 NTP 逻辑接口选择。原厂 ntpd 有 -I 的服务器接口绑定能力，但未证实 ntpsetclock 把此字段转为 -I。",
      dependencies: [
        "CLI 的 -I 用于服务器接口并隐含 -l；UCI 逻辑接口是否转成运行设备名未找到消费者。",
      ],
      flags: ["version-dependent"],
      discovery:
        "已检索 ntpsetclock 全文、init、lib/netifd 和反编译 XQSysUtil 的 ntp 读写，未找到 system.ntp.interface 读取或转换；ELF -I 能力不证明 UCI 接线。",
      impact:
        "不能保证修改后限制 NTP 的监听或发送接口。已确认的客户端命令只携带运行模式和服务器参数，实际出站路径仍取决于网络路由。",
      evidence: [
        {
          source: "usr/sbin/ntpsetclock",
          line: 38,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "ntp_sync 初始化客户端命令并从 system.ntp.server 取服务器。",
          endLine: 43,
        },
        {
          source: "usr/sbin/ntpsetclock",
          line: 51,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "该命令只追加服务器 -p 参数后执行，未生成接口绑定参数。",
          endLine: 55,
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 79,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "BusyBox ntpd 支持 -I IFACE 绑定服务器接口且隐含 -l；这是命令行能力。",
          artifact: {
            source: "bin/busybox",
            sha256:
              "d9fa606a3c706a56bfd4f2e7d1c538a2f0ae132b016cd5392004b93123d46234",
            offset: 443357,
          },
        },
      ],
      summary: "通用 NTP 接口选择；未证实原厂 ntpsetclock 使用本项绑定接口。",
    },
  },
  led: {
    name: {
      description:
        "这条 LED 配置的识别名称；不是硬件 sysfs 设备名。通用板级 helper 有此项，原厂灯服务未证实读取 system.led.name。",
      flags: ["version-dependent"],
      discovery:
        "已检索 etc/init.d、lib/functions/uci-defaults.sh、lib/xqled、led_ctl 和反编译 XQSysUtil，未找到遍历 system 的 led 章节并读取 name；板级 helper 写 JSON 不等于运行时消费此 UCI 项。",
      impact:
        "改名不等于切换物理灯。原厂 xqled 服务从自己的配置及状态 profile 启动，未确认会在官方界面显示此名称。",
      evidence: [
        {
          source: "lib/functions/uci-defaults.sh",
          line: 367,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "通用板级 LED helper 将名称和 sysfs 名加入 JSON led 对象。",
          endLine: 376,
        },
        {
          source: "etc/init.d/xqled",
          line: 8,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂启动 led_srv，使用 /lib/xqled/xqled.json 与 xqled.driver.profile。",
          endLine: 16,
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 38,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 led_srv 的 -c 选择配置文件；这是 init 使用的厂商 JSON 输入，不证明 system.led 字段读取。",
          artifact: {
            source: "usr/sbin/led_srv",
            sha256:
              "4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733",
            offset: 18932,
          },
        },
      ],
      summary: "通用 LED 配置名称；未证实原厂 xqled 读取 system.led.name。",
    },
    sysfs: {
      description:
        "通用 LED 类设备名称，对应 /sys/class/leds 下的目录；必须与实际硬件导出的名称匹配。1.0.43 未证实 system.led.sysfs 接入厂商灯服务。",
      dependencies: ["实际 /sys/class/leds 设备和相应灯控制后端必须存在。"],
      flags: ["hardware-dependent", "version-dependent"],
      discovery:
        "已检索 system/xqled init、lib/xqled 的 sysfs/gpio 后端、uci-defaults 与厂商 Lua，未找到 system.led.sysfs 读取；xqled 的 led 字段不是该字段别名。",
      impact:
        "不能保证更改后控制指定 LED。厂商 sysfs 后端取 xqled 动作中的 led 字段，并检查 brightness 文件，不取本字段。",
      evidence: [
        {
          source: "lib/functions/uci-defaults.sh",
          line: 367,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "通用 LED helper 将第三个参数以 sysfs 名写入 JSON。",
          endLine: 376,
        },
        {
          source: "lib/xqled/xqled_common.sh",
          line: 3,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商灯脚本的 THIS_MODULE 是 xqled。",
          endLine: 5,
        },
        {
          source: "lib/xqled/xqled_sysfs.sh",
          line: 23,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商动作从 led 字段取设备，要求 /sys/class/leds/<led>/brightness 存在。",
          endLine: 38,
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 38,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 led_srv 的 -c 选择配置文件；这是 init 使用的厂商 JSON 输入，不证明 system.led 字段读取。",
          artifact: {
            source: "usr/sbin/led_srv",
            sha256:
              "4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733",
            offset: 18932,
          },
        },
      ],
      summary:
        "通用 LED 类设备名；原厂 xqled 用独立 led 字段，未证实此项接入。",
    },
    default: {
      description:
        "通用 LED 触发器接管前的默认亮灭状态；与触发器名称及厂商状态灯总开关不同。",
      dependencies: [
        "需要支持通用 system.led 的运行时加载器；触发器可能随后改变亮灭。",
      ],
      flags: ["hardware-dependent", "version-dependent"],
      discovery:
        "已检索 init、lib/functions/uci-defaults.sh、lib/xqled、led_ctl 与反编译 Lua，未找到 system.led.default 消费；helper 只写板级 JSON，BLUE_LED 是另一模块的开关。",
      impact:
        "不能保证保存后立即亮灯，或让启动阶段持续保持此状态。1.0.43 启动灯服务使用厂商 profile，尚未发现将此项写入 brightness 的路径。",
      evidence: [
        {
          source: "lib/functions/uci-defaults.sh",
          line: 379,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "通用 ucidef_set_led_default 将第四个参数写入 JSON default。",
          endLine: 384,
        },
        {
          source: "etc/init.d/xqled",
          line: 10,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 led_srv 读取 xqled JSON 和驱动 profile。",
          endLine: 16,
        },
        {
          source: "usr/sbin/led_ctl",
          line: 63,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂状态灯开关保存为 xiaoqiang.common.BLUE_LED，并调用 xqled 状态动作。",
          endLine: 69,
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 38,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 led_srv 的 -c 选择配置文件；这是 init 使用的厂商 JSON 输入，不证明 system.led 字段读取。",
          artifact: {
            source: "usr/sbin/led_srv",
            sha256:
              "4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733",
            offset: 18932,
          },
        },
      ],
      summary: "通用默认亮灭状态；原厂状态灯另由 xqled/BLUE_LED 控制。",
    },
    trigger: {
      description:
        "通用内核 LED 触发器名；netdev、timer 等依赖已安装驱动。厂商 xqled 的 trigger 是另一套 on/off/blink 状态。",
      dependencies: [
        "所选内核触发器须在对应 LED 的 trigger 属性中可用；原厂对本字段的支持未确认。",
      ],
      flags: ["hardware-dependent", "version-dependent"],
      discovery:
        "已检索 system/xqled init、lib/xqled、led_ctl、通用 LED helper 和反编译 Lua，未找到 system.led.trigger 读取；同名 xqled trigger 属于独立模块，不能证明通用字段生效。",
      impact:
        "不能保证此项驱动原厂状态灯。不要把 xqled 的 blink 动作或 profile 状态名当作 system.led 已支持的内核触发器。",
      evidence: [
        {
          source: "lib/functions/uci-defaults.sh",
          line: 496,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "通用 helper 把 trigger_name 写入板级 LED JSON 的 trigger。",
          endLine: 501,
        },
        {
          source: "lib/xqled/xqled_common.sh",
          line: 5,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "xqled 定义的触发状态为 blink、on、off。",
          endLine: 10,
        },
        {
          source: "lib/xqled/xqled_gpio.sh",
          line: 62,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "GPIO 后端读取 xqled 功能的 trigger，随后按 blink/on 分支控制 GPIO。",
          endLine: 64,
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 38,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 led_srv 的 -c 选择配置文件；这是 init 使用的厂商 JSON 输入，不证明 system.led 字段读取。",
          artifact: {
            source: "usr/sbin/led_srv",
            sha256:
              "4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733",
            offset: 18932,
          },
        },
      ],
      summary: "通用内核 LED 触发器；不能与厂商 xqled 的 on/off/blink 混用。",
    },
    dev: {
      description:
        "通用 netdev LED 触发器监控的底层网络设备名，例如 eth0；不是 network 中的逻辑接口名称。",
      dependencies: [
        "通常需 trigger=netdev、有效 sysfs LED 以及存在的底层网络设备。",
      ],
      flags: ["hardware-dependent", "version-dependent"],
      discovery:
        "已检索 init、lib/functions/uci-defaults.sh、lib/xqled 和反编译 Lua，未找到 system.led.dev 读取；helper 输出键是 device，也未找到将该 JSON 转成此 UCI 字段的消费者。",
      impact:
        "仅在 netdev 加载器支持时才会改变观察的网卡，不能修改网卡本身。1.0.43 未找到把此 UCI 字段绑定到 LED 设备的路径。",
      evidence: [
        {
          source: "lib/functions/uci-defaults.sh",
          line: 409,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "通用 netdev helper 取设备与模式，输出 type=netdev、device 和 mode。",
          endLine: 417,
        },
        {
          source: "etc/init.d/xqled",
          line: 14,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "实际原厂灯服务以 xqled JSON/profile 启动。",
          endLine: 16,
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 38,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 led_srv 的 -c 选择配置文件；这是 init 使用的厂商 JSON 输入，不证明 system.led 字段读取。",
          artifact: {
            source: "usr/sbin/led_srv",
            sha256:
              "4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733",
            offset: 18932,
          },
        },
      ],
      summary: "通用 netdev LED 监控设备；未证实原厂会从此字段绑定网卡。",
    },
    mode: {
      description:
        "通用 netdev 触发条件组合：link 表示链路状态，tx/rx 表示发送/接收活动。此组合不是网络接口工作模式。",
      range: "link、tx、rx 的原生组合（通用 netdev 约定）",
      dependencies: [
        "与 trigger=netdev、dev 和 sysfs 一起解释；依赖内核触发器支持。",
      ],
      flags: ["hardware-dependent", "version-dependent"],
      discovery:
        "已检索 init、lib/xqled、通用 netdev helper 和反编译 Lua，未找到 system.led.mode 的运行时消费。helper 的 link tx rx 回退只对板级函数参数有效，不作为本 UCI 项缺省。",
      impact:
        "若加载器支持，可选择常亮链路提示或流量闪烁；1.0.43 未证实原厂把此项写入 netdev LED 触发属性。",
      evidence: [
        {
          source: "lib/functions/uci-defaults.sh",
          line: 409,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netdev 板级 helper 的第五参数为空时采用 link tx rx，并写入 JSON mode。",
          endLine: 417,
        },
        {
          source: "etc/init.d/xqled",
          line: 10,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 led_srv 使用独立 xqled JSON 与 profile。",
          endLine: 16,
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 38,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 led_srv 的 -c 选择配置文件；这是 init 使用的厂商 JSON 输入，不证明 system.led 字段读取。",
          artifact: {
            source: "usr/sbin/led_srv",
            sha256:
              "4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733",
            offset: 18932,
          },
        },
      ],
      summary: "通用 link/tx/rx 条件组合；1.0.43 未证实原厂读取此 UCI 项。",
    },
    delayon: {
      description:
        "通用 timer/oneshot 触发器的点亮持续时间，以毫秒表示；不等同于厂商 xqled 的 msec_on。",
      unit: "毫秒（通用 timer 约定）",
      range: "非负整数；实际范围由 LED 触发器决定",
      dependencies: ["通常需 trigger=timer 或 oneshot，并与 delayoff 配合。"],
      flags: ["hardware-dependent", "version-dependent"],
      discovery:
        "已检索 init、lib/xqled、led_ctl、通用 timer helper 和反编译 Lua，未找到 system.led.delayon 读取；板级 JSON 同名键和 xqled.msec_on 都不证明此 UCI 项已接入。",
      impact:
        "仅在对应通用触发器加载器支持时改变亮灯阶段。厂商 GPIO 闪烁另外读 msec_on，不能把其回退 800 当成本字段缺省。",
      evidence: [
        {
          source: "lib/functions/uci-defaults.sh",
          line: 477,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "通用 timer helper 用第五参数写 JSON delayon、第六参数写 delayoff。",
          endLine: 486,
        },
        {
          source: "lib/functions/uci-defaults.sh",
          line: 492,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "timer helper 将触发器设为 timer。",
          endLine: 493,
        },
        {
          source: "lib/xqled/xqled_gpio.sh",
          line: 87,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商 GPIO 闪烁读 msec_on/msec_off，分别回退 800，并通过 MS2UNIT 转换。",
          endLine: 92,
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 38,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 led_srv 的 -c 选择配置文件；这是 init 使用的厂商 JSON 输入，不证明 system.led 字段读取。",
          artifact: {
            source: "usr/sbin/led_srv",
            sha256:
              "4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733",
            offset: 18932,
          },
        },
      ],
      summary: "通用 timer 点亮时长（毫秒）；原厂另读 msec_on，不是本项。",
    },
    delayoff: {
      description:
        "通用 timer/oneshot 触发器的熄灭持续时间，以毫秒表示；与 delayon 分别控制一个闪烁周期的两段。",
      unit: "毫秒（通用 timer 约定）",
      range: "非负整数；实际范围由 LED 触发器决定",
      dependencies: ["通常需 trigger=timer 或 oneshot，并与 delayon 配合。"],
      flags: ["hardware-dependent", "version-dependent"],
      discovery:
        "已检索 init、lib/xqled、led_ctl、通用 timer helper 和反编译 Lua，未找到 system.led.delayoff 读取；xqled.msec_off 是另一字段，不能据此指定本字段回退。",
      impact:
        "仅在对应通用触发器加载器支持时改变灭灯阶段。原厂 xqled 读取自己的 msec_off，本字段不保证覆盖厂商灯效。",
      evidence: [
        {
          source: "lib/functions/uci-defaults.sh",
          line: 477,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "通用 timer helper 以第六参数写 JSON delayoff。",
          endLine: 486,
        },
        {
          source: "lib/xqled/xqled_gpio.sh",
          line: 87,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商 blink 分支从功能配置读取 msec_off 并转换 GPIO 时间单位。",
          endLine: 92,
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 38,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 led_srv 的 -c 选择配置文件；这是 init 使用的厂商 JSON 输入，不证明 system.led 字段读取。",
          artifact: {
            source: "usr/sbin/led_srv",
            sha256:
              "4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733",
            offset: 18932,
          },
        },
      ],
      summary: "通用 timer 熄灭时长（毫秒）；原厂另读 msec_off，不是本项。",
    },
    interval: {
      description:
        "通用 LED 触发器的检查间隔，界面以毫秒表示；不是 timer 的亮灭持续时间。1.0.43 未确认其消费者与适用触发器。",
      unit: "毫秒（现有字段约定；原厂未确认）",
      dependencies: [
        "需要明确支持 interval 的触发器和对应加载器；不能假定所有触发器都使用它。",
      ],
      flags: ["hardware-dependent", "version-dependent"],
      discovery:
        "已检索 init、lib/functions/uci-defaults.sh、lib/xqled、led_ctl 及反编译 Lua，未找到 system.led.interval 读取，也未找到通用 netdev helper 输出 interval；没有可证实的缺省或范围。",
      impact:
        "不能保证改变后调整网络活动检测频率或厂商灯效。已检查的 netdev helper 仅输出设备与模式，原厂 xqled 服务也未建立此 UCI 项的关联。",
      evidence: [
        {
          source: "lib/functions/uci-defaults.sh",
          line: 409,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "通用 netdev helper 写 type、device、mode。",
          endLine: 417,
        },
        {
          source: "etc/init.d/xqled",
          line: 14,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂启动命令只使用 xqled JSON/profile 与日志级别参数。",
          endLine: 18,
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 38,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 led_srv 的 -c 选择配置文件；这是 init 使用的厂商 JSON 输入，不证明 system.led 字段读取。",
          artifact: {
            source: "usr/sbin/led_srv",
            sha256:
              "4b2018ab36d643cd80218a3bf8d04bc7cad7e9a8002fbfabbf3b959a317ed733",
            offset: 18932,
          },
        },
      ],
      summary: "通用触发器检查间隔；原厂消费者、适用触发器与单位未确认。",
    },
  },
};
