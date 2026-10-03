import type { ModuleFieldHelp } from "./types";

// RN02 1.0.43 static sources; parsed keys do not by themselves prove every runtime effect.
export const networkFieldHelp: ModuleFieldHelp = {
  interface: {
    proto: {
      description:
        "选择逻辑接口的地址获取或拨号协议。DHCP、DHCPv6、PPP 和隧道分别交给对应处理器；协议插件是否安装会影响可用项。",
      dependencies: ["与所选协议的地址、DNS、认证字段配合使用。"],
      flags: ["version-dependent"],
      impact: "更换协议会改变地址、路由与认证流程，可能中断此接口的连接。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 20,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 接口参数表声明 proto 的原生输入类型。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172632,
          },
        },
        {
          source: "lib/netifd/netifd-proto.sh",
          line: 489,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "add_protocol 只匹配请求的协议，再分派 setup、teardown 或 renew。",
          endLine: 495,
        },
        {
          source: "lib/netifd/proto/ppp.sh",
          line: 405,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "PPPoE/PPTP 的注册依赖对应 pppd 插件文件。",
          endLine: 409,
        },
        {
          source: "docs/field-help-network-research.md",
          line: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "接口创建解析真实参数表并消费 proto 槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 27008,
          },
        },
        {
          source:
            "live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh",
          line: 405,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "当前公开脚本与 1.0.43 逐字节相同；PPPoE/PPTP 的注册依赖对应 pppd 插件文件。",
          endLine: 409,
        },
      ],
    },
    device: {
      description:
        "设置逻辑接口绑定的底层设备。netifd 接口参数表明确接收 device 字符串；串口 PPP 也使用同名参数，但含义是 PPP 设备路径。串口 PPP 的同名 device 则由协议解释为 PPP 设备路径。",
      dependencies: [
        "普通接口引用已有设备；device 缺失时核心回退读取 ifname；串口 PPP 则由协议解释此项。",
      ],
      flags: ["hardware-dependent"],
      impact:
        "修改底层绑定会改变逻辑接口的承载设备；串口 PPP 与以太网设备名称不能混用。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 18,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 接口参数表将 device 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172616,
          },
        },
        {
          source: "lib/netifd/proto/ppp.sh",
          line: 197,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "串口 PPP 读取 device 并将其传给通用 PPP setup。",
          endLine: 209,
        },
        {
          source: "docs/field-help-network-research.md",
          line: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "接口创建解析真实参数表并消费 device 槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 27008,
          },
        },
        {
          source:
            "live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh",
          line: 197,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "当前公开脚本与 1.0.43 逐字节相同；串口 PPP 读取 device 并将其传给通用 PPP setup。",
          endLine: 209,
        },
        {
          source: "docs/field-help-network-research.md",
          line: 176,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心接口主设备绑定优先 device，缺省再读 ifname。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 24404,
          },
        },
      ],
      summary:
        "绑定底层网卡或桥，优先于 ifname；串口 PPP 则表示 PPP 设备路径。",
    },
    ifname: {
      description:
        "绑定已有网卡或网卡列表。厂商端口映射会重建 LAN 的 ifname；网桥接口在状态查询中可被替换为 br-接口名或实际三层设备。",
      flags: ["legacy", "generated", "hardware-dependent"],
      dependencies: [
        "type=bridge 时可有多个成员；厂商端口分配操作可能重写此项。",
      ],
      impact:
        "修改成员或网卡会改变 LAN/WAN 的物理承载，可能让管理连接转到另一端口。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 19,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 接口参数表声明 ifname 的原生输入类型。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172624,
          },
        },
        {
          source: "lib/network/config.sh",
          line: 42,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "fixup_interface 读取 ifname，桥接时使用 br-章节名，有 l3_device 时再覆盖。",
          endLine: 49,
        },
        {
          source: "sbin/port_map",
          line: 148,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "端口映射收集 LAN 端口的 ifname，并写回 network.lan.ifname 后 reload。",
          endLine: 168,
        },
        {
          source: "docs/field-help-network-research.md",
          line: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "接口创建解析真实参数表并消费 ifname 槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 27008,
          },
        },
      ],
    },
    type: {
      description:
        "旧式 interface 章节中，bridge 表示将成员接口组成网桥。访客网络创建脚本仍使用这一结构。",
      dependencies: ["与 ifname 成员及该逻辑接口的 IP 配置配合。"],
      flags: ["legacy"],
      impact:
        "桥接会把成员接入同一二层网络；改变 LAN 桥结构可能中断本机管理与终端互通。",
      evidence: [
        {
          source: "lib/network/config.sh",
          line: 42,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "type 为 bridge 时，脚本采用 br-章节名作为接口名。",
          endLine: 44,
        },
        {
          source: "usr/sbin/guestwifi.sh",
          line: 87,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "访客网络以 interface 章节创建，并设置 type=bridge、proto=static。",
          endLine: 94,
        },
      ],
    },
    ipaddr: {
      description:
        "static 模式下设置 IPv4 本机地址；dhcp 模式下同名参数是向 DHCP 服务器请求的地址，不是强制使用的固定地址。",
      range: "IPv4 地址；静态网段还需正确的掩码。",
      dependencies: [
        "解释取决于 proto；LAN 地址还需与 DHCP 地址池和防火墙网络一致。",
      ],
      impact:
        "静态地址改变后，管理入口和本地网段会改变；DHCP 请求地址仍由服务器决定。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 65,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd static 地址参数表接收 ipaddr。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173208,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/admin_network/proto_static.lua",
          line: 21,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "静态协议表单将 ipaddr 标记为 IPv4 address，并要求 ip4addr。",
          endLine: 27,
        },
        {
          source: "lib/netifd/proto/dhcp.sh",
          line: 102,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "DHCP 处理器把非空 ipaddr 传给 udhcpc 的 -r 地址请求选项。",
        },
        {
          source:
            "live-inspection/field-help-live-1.0.64/lib/netifd/proto/dhcp.sh",
          line: 102,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "当前公开脚本与 1.0.43 逐字节相同；DHCP 处理器把非空 ipaddr 传给 udhcpc 的 -r 地址请求选项。",
        },
      ],
    },
    netmask: {
      description:
        "定义静态 IPv4 地址的网段掩码。Mesh 子节点的 DHCP 回调也会把租约掩码写回 LAN 配置。",
      dependencies: ["与 ipaddr 一起设置；Mesh RE 模式可能由租约回写。"],
      flags: ["generated"],
      impact:
        "掩码决定哪些地址视为本地直连；与 DHCP 地址池不一致时，终端可能无法访问网关。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 67,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd static 地址参数表接收 netmask。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173224,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/admin_network/proto_static.lua",
          line: 32,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "静态协议表单定义 netmask，标签为 IPv4 netmask。",
          endLine: 38,
        },
        {
          source: "lib/netifd/dhcp.script",
          line: 214,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "Mesh RE 租约回调把 netmask 写入 network.lan.netmask。",
          endLine: 223,
        },
      ],
    },
    gateway: {
      description:
        "设置静态 IPv4 接口的下一跳网关。Mesh RE 的 DHCP 回调会用租约的 router 更新 LAN gateway。",
      dependencies: ["用于静态地址；网关需能由承载接口到达。"],
      flags: ["generated"],
      impact: "影响接口离开本地子网的路径；错误网关可能使上网或远端管理失败。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 70,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd static 地址参数表接收 gateway。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173248,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/admin_network/proto_static.lua",
          line: 55,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "静态协议表单将 gateway 标记为 IPv4 gateway 并要求 ip4addr。",
          endLine: 61,
        },
        {
          source: "lib/netifd/dhcp.script",
          line: 220,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "Mesh RE 回调将租约 router 写为 network.lan.gateway。",
          endLine: 223,
        },
      ],
    },
    broadcast: {
      description:
        "static 模式下表示 IPv4 广播地址。dhcp 处理器却将同名字段作为布尔值，用于发送带广播标志的 DHCP 请求。",
      dependencies: ["必须先确认 proto；static 用地址，dhcp 用 0/1 原生值。"],
      impact:
        "静态广播地址影响本子网广播；DHCP 广播标志影响租约协商，不能把两种格式混用。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 68,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd static 地址参数表接收 broadcast。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173232,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/admin_network/proto_static.lua",
          line: 66,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "静态表单的 broadcast 是 IPv4 broadcast，datatype 为 ip4addr。",
          endLine: 72,
        },
        {
          source: "lib/netifd/proto/dhcp.sh",
          line: 17,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "DHCP 将 broadcast 声明为 bool。",
        },
        {
          source: "lib/netifd/proto/dhcp.sh",
          line: 57,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "broadcast=1 时给 udhcpc 添加 -B。",
        },
        {
          source:
            "live-inspection/field-help-live-1.0.64/lib/netifd/proto/dhcp.sh",
          line: 17,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "当前公开脚本与 1.0.43 逐字节相同；DHCP 将 broadcast 声明为 bool。",
        },
      ],
      summary:
        "static 用广播地址；dhcp 同名字段是 0/1 广播标志，格式不能混用。",
    },
    ip6addr: {
      description:
        "设置静态 IPv6 本机地址。PPP 自动 IPv6 的静态分支会读取 wan6 的地址并传给动态接口。",
      dependencies: [
        "静态 IPv6；PPP 自动创建路径还依赖 ipv6.<wan6>.mode=static。",
      ],
      range: "IPv6 地址及原生前缀格式。",
      impact:
        "改变接口的 IPv6 可达地址；地址与前缀不匹配会影响下游和上游通信。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 66,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd static 地址参数表接收 ip6addr。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173216,
          },
        },
        {
          source: "lib/netifd/ppp6-up",
          line: 33,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "静态 IPv6 分支要求 ip6addr、ip6gw、ip6prefix 都非空。",
          endLine: 37,
        },
        {
          source: "lib/netifd/ppp6-up",
          line: 45,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "静态动态接口更新提交 ip6addr 数组。",
          endLine: 49,
        },
      ],
    },
    ip6gw: {
      description:
        "设置静态 IPv6 的下一跳网关；PPP 自动 IPv6 的静态分支将此值传入 wan6 动态配置。",
      dependencies: ["与 ip6addr、ip6prefix 及静态 IPv6 模式配合。"],
      range: "IPv6 网关地址。",
      impact: "网关决定静态 IPv6 的上游出口；错误值会使远端 IPv6 目标不可达。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 71,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd static 地址参数表接收 ip6gw。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173256,
          },
        },
        {
          source: "lib/netifd/ppp6-up",
          line: 33,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "静态分支读取 network.<wan6>.ip6gw，并要求其非空。",
          endLine: 37,
        },
        {
          source: "lib/netifd/ppp6-up",
          line: 48,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "动态 static 接口配置提交 ip6gw。",
        },
      ],
    },
    ip6prefix: {
      description:
        "配置可供下游使用的 IPv6 前缀，不等同于本机 ip6addr。DHCPv6 处理器会将手动前缀导出为 USERPREFIX；PPP 静态分支也会转交该前缀。",
      dependencies: ["下游分配还需 ip6assign；上游需能够路由此前缀。"],
      range: "IPv6 前缀及前缀长度，可保留原生列表。",
      impact:
        "手动前缀会影响下游地址来源；错误前缀可能让 LAN IPv6 看似有地址却无法回程。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 72,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd static 地址参数表接收 ip6prefix。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173264,
          },
        },
        {
          source: "lib/netifd/proto/dhcpv6.sh",
          line: 22,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "DHCPv6 将 ip6prefix 声明为地址列表。",
        },
        {
          source: "lib/netifd/proto/dhcpv6.sh",
          line: 116,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "读取 ip6prefix 列表并累加至 ip6prefixes。",
        },
        {
          source: "lib/netifd/proto/dhcpv6.sh",
          line: 174,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "非空 ip6prefixes 导出为 USERPREFIX。",
        },
        {
          source: "lib/netifd/ppp6-up",
          line: 49,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "PPP 的静态 IPv6 动态配置转交 ip6prefix。",
        },
      ],
    },
    ip6assign: {
      description:
        "选择分配给下游的 IPv6 前缀长度。厂商 LAN IPv6 设置会比较旧值，变更后提交 network 并重新加载 IPv6 网络。",
      unit: "位",
      range: "IPv6 前缀长度为 0–128；上游前缀必须容纳所需子网。",
      dependencies: [
        "需要可用的上游委派前缀或本地前缀；与 LAN IPv6 服务模式相关。",
      ],
      impact: "长度改变会重划下游子网和 IPv6 地址；终端可能需要重新获取地址。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 31,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 接口参数表声明 ip6assign 的原生输入类型。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172720,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua",
          line: 8274,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商函数从 network.lan 读取 ip6assign 并转为数字。",
          endLine: 8281,
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua",
          line: 8431,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "ip6assign 改变后写入 LAN、提交 network，并执行 ipv6.sh reload_network all。",
          endLine: 8449,
        },
        {
          source: "docs/field-help-network-research.md",
          line: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "接口创建解析真实参数表并消费 ip6assign 槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 27008,
          },
        },
      ],
    },
    ip6hint: {
      description:
        "IPv6 子网提示，以十六进制子网编号选择可分配前缀内的子网。netifd 接口表接收字符串，不应改成十进制数值。",
      dependencies: ["面板语义要求配合 ip6assign 与可用前缀。"],
      flags: ["version-dependent"],
      impact:
        "改变希望分配的下游子网编号；超出分配空间的位会被掩码去掉，不保证上游一定提供所需前缀。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 32,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 接口参数表将 ip6hint 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172728,
          },
        },
        {
          source: "etc/init.d/network",
          line: 34,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网络服务启动 netifd；前缀分配实现不在该启动脚本中。",
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua",
          line: 8277,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "已检查的 LAN IPv6 消费函数读取 network.lan.ip6assign。",
          endLine: 8281,
        },
        {
          source: "docs/field-help-network-research.md",
          line: 168,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心将 ip6hint 按 base=16 转换，并按分配长度掩码保存。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 27432,
          },
        },
      ],
      summary: "十六进制子网编号提示，需能放入 ip6assign 对应的分配空间。",
    },
    ip6class: {
      description:
        "设置参与 IPv6 前缀分配的类别列表；netifd 接口表接收数组。厂商切换 WAN IPv6 模式会清空 LAN 的此项。",
      dependencies: ["需要可用前缀类别；可保留上游接口名称或原生列表。"],
      flags: ["generated", "version-dependent"],
      discovery:
        "已确认 interface.ip6class 的数组解析、核心逐项字符串保存及厂商清空路径；类别不匹配和前缀冲突时的最终分配顺序未在本轮定位。",
      impact:
        "若核心执行类别筛选，会影响 LAN 取得哪个上游前缀；厂商模式切换可能重置此选择。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 35,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 接口参数表将 ip6class 声明为 array。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172752,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua",
          line: 7150,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "WAN IPv6 模式配置函数调用 set 清空 ip6class。",
          endLine: 7154,
        },
        {
          source: "docs/field-help-network-research.md",
          line: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "接口创建解析真实参数表并消费 ip6class 槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 27008,
          },
        },
      ],
    },
    dns: {
      description:
        "设置手动 DNS 服务器列表。PPP 创建动态 IPv6 接口时，peerdns=0 才会读此列表；静态协议表单将其定义为自定义 DNS。",
      dependencies: ["DHCP/拨号场景与 peerdns 配合；地址需能经路由到达。"],
      impact:
        "影响路由器及其 DNS 转发的上游解析来源；错误地址会造成域名解析失败。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 26,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 接口参数表声明 dns 的原生输入类型。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172680,
          },
        },
        {
          source: "lib/netifd/ppp6-up",
          line: 80,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "peerdns=0 时读取 network.<接口>.dns 并加入动态 IPv6 DNS 更新。",
          endLine: 86,
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/admin_network/proto_static.lua",
          line: 77,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "静态协议表单使用 DynamicList 定义自定义 dns，datatype 为 ipaddr。",
          endLine: 84,
        },
        {
          source: "docs/field-help-network-research.md",
          line: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "接口创建解析真实参数表并消费 dns 槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 27008,
          },
        },
      ],
    },
    dns_search: {
      description:
        "设置 DNS 搜索域列表，供短主机名解析时展开。netifd 接口表直接接收 dns_search 数组；DHCP 租约中的 domain 也作为搜索域提交。",
      flags: ["version-dependent"],
      dependencies: ["需与 DNS 服务器及其可解析的域配合。"],
      impact:
        "改变短名称的域名展开，可能影响本地名称解析；不会更改 DNS 服务器地址。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 27,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 接口参数表将 dns_search 声明为 array。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172688,
          },
        },
        {
          source: "lib/netifd/dhcp.script",
          line: 81,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "DHCP 回调将租约 domain 逐项加入 DNS 搜索域。",
          endLine: 83,
        },
        {
          source: "lib/netifd/netifd-proto.sh",
          line: 349,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "协议更新把 PROTO_DNS_SEARCH 输出为 dns_search 数组。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 169,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "接口创建直接消费手动 dns_search 数组。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 27160,
          },
        },
      ],
      summary: "DNS 搜索域列表，用于展开短主机名；与 DNS 服务器地址分开设置。",
    },
    peerdns: {
      description:
        "允许使用上游协商得到的 DNS。核心接口创建在缺少此项时采用 1；PPP 自动 IPv6 路径同样回退为 1，设为 0 时读取手动 dns。",
      defaultValue:
        "1（核心接口创建读取该项时的缺省；协议/厂商模式仍可能另行控制）。",
      dependencies: ["主要用于 DHCP/拨号；关闭后需可用的手动 dns。"],
      impact:
        "决定动态 IPv6 接口使用上游还是手动 DNS；不影响 DHCP 服务器给终端分配地址。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 25,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 接口参数表声明 peerdns 的原生输入类型。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172672,
          },
        },
        {
          source: "lib/netifd/ppp6-up",
          line: 77,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "创建动态 IPv6 接口时 peerdns 为空回退为 1，并在值为 0 时加载手动 dns。",
          endLine: 86,
        },
        {
          source: "lib/netifd/netifd-proto.sh",
          line: 25,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "协议公共配置声明 peerdns 为布尔项。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "接口创建解析真实参数表并消费 peerdns 槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 27008,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 175,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "布尔项缺失时返回调用者提供的回退1，非空规范化为0/1。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 24084,
          },
        },
      ],
    },
    defaultroute: {
      description:
        "决定协议接口是否允许安装默认出口。公共协议配置声明此布尔项，随固件附带的 DHCP 表单说明关闭后不配置默认路由。",
      dependencies: ["与 proto、上游网关、metric 及其他出口配合。"],
      impact:
        "关闭可能让此 WAN 不再承担普通上网出口；已有静态或其他接口的路由仍可独立存在。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 24,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 接口参数表声明 defaultroute 的原生输入类型。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172664,
          },
        },
        {
          source: "lib/netifd/netifd-proto.sh",
          line: 23,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "公共协议配置声明 defaultroute、peerdns、metric。",
          endLine: 26,
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/admin_network/proto_dhcp.lua",
          line: 90,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "DHCP 表单将 defaultroute 标注为 Use default gateway，说明关闭则不配置默认路由。",
          endLine: 97,
        },
        {
          source: "docs/field-help-network-research.md",
          line: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "接口创建解析真实参数表并消费 defaultroute 槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 27008,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 175,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "布尔项缺失时返回调用者提供的回退1，非空规范化为0/1。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 24084,
          },
        },
      ],
      defaultValue:
        "1（核心接口创建读取该项时的缺省；协议/厂商模式仍可能另行控制）。",
    },
    delegate: {
      description:
        "允许将 IPv6 前缀交给下游分配。DHCP 的 6rd 和 DHCPv6 的 MAP/DS-Lite 子接口会在此值为 0 时传递禁用委派标志。",
      dependencies: ["需要上游委派或手动前缀；下游用 ip6assign 分配。"],
      impact:
        "关闭会影响这些动态子接口的下游前缀可用性，但不等同于停用 IPv6 地址或 DNS。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 36,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 接口参数表声明 delegate 的原生输入类型。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172760,
          },
        },
        {
          source: "lib/netifd/proto/dhcp.sh",
          line: 66,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "delegate=0 时导出 IFACE6RD_DELEGATE=0。",
        },
        {
          source: "lib/netifd/proto/dhcpv6.sh",
          line: 178,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "delegate=0 时导出 DS-Lite 与 MAP 的禁用委派标志。",
          endLine: 179,
        },
        {
          source: "lib/netifd/dhcpv6.script",
          line: 250,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "MAP 动态接口把 IFACE_MAP_DELEGATE 传为 delegate。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "接口创建解析真实参数表并消费 delegate 槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 27008,
          },
        },
        {
          source:
            "live-inspection/field-help-live-1.0.64/lib/netifd/proto/dhcp.sh",
          line: 66,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "当前公开脚本与 1.0.43 逐字节相同；delegate=0 时导出 IFACE6RD_DELEGATE=0。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 175,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "布尔项缺失时返回调用者提供的回退1，非空规范化为0/1。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 24084,
          },
        },
      ],
      defaultValue:
        "1（核心接口创建读取该项时的缺省；协议/厂商模式仍可能另行控制）。",
    },
    ipv6: {
      description:
        "此字段的含义取决于协议。RN02 的通用 PPP 处理器仅在值为 auto 且 force_disable_ipv6 不为 1 时打开 IPv6 并启用自动 wan6；L2TP 则以值 1 启用 IPv6。",
      dependencies: [
        "受 proto、network.<接口>.force_disable_ipv6 和 ipv6.<wan6>.mode 影响。",
      ],
      flags: ["version-dependent"],
      impact:
        "改变 PPP 的 IPv6 协商和动态子接口生成；不要认为 1 与 auto 在所有拨号协议中等价。",
      evidence: [
        {
          source: "lib/netifd/proto/ppp.sh",
          line: 101,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "通用 PPP 只在 ipv6=auto 且未 force_disable_ipv6 时设置 IPv6 和 AUTOIPV6。",
          endLine: 107,
        },
        {
          source: "lib/netifd/proto/l2tp.sh",
          line: 67,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "L2TP 仅保留值为 1 的 ipv6。",
        },
        {
          source: "lib/netifd/ppp6-up",
          line: 19,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "AUTOIPV6=1 时进入动态 wan6 创建逻辑。",
          endLine: 31,
        },
        {
          source:
            "live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh",
          line: 101,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "当前公开脚本与 1.0.43 逐字节相同；通用 PPP 只在 ipv6=auto 且未 force_disable_ipv6 时设置 IPv6 和 AUTOIPV6。",
          endLine: 107,
        },
      ],
      summary: "协议含义不同：通用 PPP 使用 auto；L2TP 使用 1，不能视为等价。",
    },
    auto: {
      description:
        "控制逻辑接口的自动启动；netifd 接口表接收布尔值。另有厂商 autovpn：PPTP 的 vpn.auto=1 且未连接时，会发起重连。",
      dependencies: [
        "PPTP 自动重连另需 user_option=1，且非 AP/Mesh 子节点模式。",
      ],
      impact:
        "关闭可阻止接口自动启动；PPTP VPN 的自动重连还受厂商模式和用户开关控制。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 21,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 接口参数表将 auto 声明为 bool/int8。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172640,
          },
        },
        {
          source: "etc/init.d/autovpn",
          line: 41,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "vpn.auto=1、proto=pptp、user_option=1 且状态未连接时执行 ifdown 与 vpn.lua up。",
          endLine: 50,
        },
        {
          source: "docs/field-help-network-research.md",
          line: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "接口创建解析真实参数表并消费 auto 槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 27008,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 175,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "布尔项缺失时返回调用者提供的回退1，非空规范化为0/1。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 24084,
          },
        },
      ],
      defaultValue:
        "1（核心接口创建读取该项时的缺省；协议/厂商模式仍可能另行控制）。",
    },
    force_link: {
      description:
        "控制接口是否忽略底层载波状态。核心读取布尔项，未设置时采用协议标志；厂商 LAN 链路聚合重建会写入 1。",
      flags: ["generated", "hardware-dependent"],
      dependencies: ["与底层链路或聚合设备相关。"],
      impact:
        "若核心接受此项，可在物理链路未就绪时仍处理接口；链路聚合重建可能覆盖手动设置。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 38,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 接口参数表将 force_link 声明为 bool/int8。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172776,
          },
        },
        {
          source: "sbin/port_map",
          line: 153,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "LAN 端口 service=lag 时向对应 network 章节写 force_link=1。",
          endLine: 158,
        },
        {
          source: "docs/field-help-network-research.md",
          line: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "接口创建解析真实参数表并消费 force_link 槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 27008,
          },
        },
      ],
    },
    disabled: {
      description:
        "标记逻辑接口停用。厂商网络诊断遇到 WAN disabled=1 会跳过该接口；IPv6 模式迁移也会生成此标记以避免重复 WAN6。",
      flags: ["generated"],
      dependencies: ["厂商 IPv6 模式可能管理 WAN6 的此标记。"],
      impact:
        "停用 WAN 可能移除该出口；停用 LAN 可能失去当前管理连接，诊断也不再检测停用的 WAN。",
      evidence: [
        {
          source: "usr/sbin/nettb2",
          line: 436,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "WAN 检查在 network.<wan>.disabled=1 时直接返回。",
          endLine: 440,
        },
        {
          source: "usr/sbin/ipv6.sh",
          line: 283,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "旧 IPv6 配置迁移在 PPPoE 时设 disabled=1。",
          endLine: 286,
        },
        {
          source: "usr/sbin/ipv6.sh",
          line: 301,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "迁移函数将 disabled 写入 wan6 接口。",
        },
      ],
    },
    mtu: {
      description:
        "设置接口最大传输单元。通用以太网和隧道不要混用：L2TP 虽读取此字段，随后仍改用当前 WAN 的 MTU/MRU 减 40；PPPoE 实际由 mru 控制。",
      unit: "字节",
      dependencies: ["取决于 proto 和承载链路；PPPoE 还需检查原生 mru。"],
      impact:
        "过大可能造成分片或路径 MTU 故障，过小增加开销；L2TP 中手动 mtu 可能被重算。",
      evidence: [
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/admin_network/proto_static.lua",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "静态协议表单将 mtu 标记为 Override MTU；1500 只是 placeholder。",
          endLine: 177,
        },
        {
          source: "lib/netifd/proto/l2tp.sh",
          line: 65,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "L2TP 读取 mtu。",
          endLine: 66,
        },
        {
          source: "lib/netifd/proto/l2tp.sh",
          line: 78,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "L2TP 根据 WAN 的 mtu/mru 及 VLAN 开销计算最终 MTU，再减 40。",
          endLine: 95,
        },
        {
          source: "lib/netifd/proto/ppp.sh",
          line: 165,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "通用 PPP 使用 mru 同时生成 pppd 的 mtu 和 mru。",
        },
        {
          source:
            "live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh",
          line: 165,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "当前公开脚本与 1.0.43 逐字节相同；通用 PPP 使用 mru 同时生成 pppd 的 mtu 和 mru。",
        },
      ],
      summary: "字节；L2TP 会按 WAN 重算，PPPoE 的实际参数由 mru 控制。",
    },
    metric: {
      description:
        "设置接口路由的度量值，较小值通常优先。公共协议配置声明为整数，DHCP 表单约束为非负整数；表单的 0 占位符不是已证明的运行缺省值。",
      range: "非负整数。",
      dependencies: ["与 defaultroute、同目标的其他路由及多 WAN 策略配合。"],
      impact: "与其他接口竞争相同目标路由时影响选择顺序；不直接限制带宽。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 29,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 接口参数表声明 metric 的原生输入类型。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172704,
          },
        },
        {
          source: "lib/netifd/netifd-proto.sh",
          line: 26,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "公共协议配置把 metric 声明为整数。",
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/admin_network/proto_dhcp.lua",
          line: 137,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "DHCP 表单将 metric 标注为 Use gateway metric，datatype 为 uinteger，0 为 placeholder。",
          endLine: 144,
        },
        {
          source: "docs/field-help-network-research.md",
          line: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "接口创建解析真实参数表并消费 metric 槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 27008,
          },
        },
      ],
    },
    macaddr: {
      description:
        "覆盖接口的 MAC 地址。启动初始化只对 LAN/WAN 系列且非 CPE 接口处理：缺少此项时，从 getmac 读取硬件地址并写入配置。",
      defaultValue:
        "LAN/WAN 非 CPE 且未配置时：getmac 返回的硬件地址（若非空）。",
      flags: ["generated", "hardware-dependent"],
      range: "原生 MAC 地址格式。",
      impact:
        "改动可能改变 DHCP 租约和运营商的设备识别；不可使同一二层网络出现重复 MAC。",
      evidence: [
        {
          source: "lib/miwifi/lib_network.sh",
          line: 34,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "MAC 初始化只处理 lan/wan 系列并跳过 wantype=cpe。",
          endLine: 38,
        },
        {
          source: "lib/miwifi/lib_network.sh",
          line: 40,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "macaddr 为空且硬件 getmac 非空时，写入 macaddr 并提交 network。",
          endLine: 45,
        },
      ],
    },
    username: {
      description:
        "设置 PPP 或 L2TP 认证用户名。通用 PPP 仅在用户名非空时，才同时向 pppd 提交 user 与 password。",
      dependencies: ["用于 PPP/PPPoE/PPTP/L2TP；与 password 配套。"],
      flags: ["credential"],
      impact: "错误用户名会导致认证失败；不会改变接口设备或 IP 网段。",
      evidence: [
        {
          source: "lib/netifd/proto/ppp.sh",
          line: 69,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "通用 PPP 声明 username 字符串。",
        },
        {
          source: "lib/netifd/proto/ppp.sh",
          line: 158,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "username 非空时向 pppd 传 user 与 password。",
        },
        {
          source: "lib/netifd/proto/l2tp.sh",
          line: 73,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "L2TP 在用户名非空时生成 user/password 认证参数。",
        },
        {
          source:
            "live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh",
          line: 69,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "当前公开脚本与 1.0.43 逐字节相同；通用 PPP 声明 username 字符串。",
        },
      ],
    },
    password: {
      description:
        "设置拨号或隧道的认证密码。它由 PPP/L2TP 处理器传给 pppd；不是面板登录密码，也不是 Wi-Fi 密钥。",
      dependencies: ["与 username、运营商或 VPN 账户配合。"],
      flags: ["credential"],
      impact:
        "错误密码会使拨号或隧道认证失败；保存后需等接口重新协商才可验证。",
      evidence: [
        {
          source: "lib/netifd/proto/ppp.sh",
          line: 70,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "通用 PPP 声明 password 字符串。",
        },
        {
          source: "lib/netifd/proto/ppp.sh",
          line: 158,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "用户名非空时连同 password 交给 pppd。",
        },
        {
          source: "lib/netifd/proto/l2tp.sh",
          line: 73,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "L2TP 将 password 加入认证参数。",
        },
        {
          source:
            "live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh",
          line: 70,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "当前公开脚本与 1.0.43 逐字节相同；通用 PPP 声明 password 字符串。",
        },
      ],
    },
    ac: {
      description:
        "筛选 PPPoE 接入集中器名称。值非空才会给 rp-pppoe 插件添加 rp_pppoe_ac；空值不会生成此筛选参数。",
      dependencies: ["仅 PPPoE，且需 rp-pppoe.so 插件。"],
      impact:
        "错误名称可能导致找不到接入集中器；不设置时保留插件自身的选择行为。",
      evidence: [
        {
          source: "lib/netifd/proto/ppp.sh",
          line: 238,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "PPPoE setup 从 JSON 读取 ac。",
        },
        {
          source: "lib/netifd/proto/ppp.sh",
          line: 252,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "ac 非空时添加 rp_pppoe_ac 参数。",
        },
        {
          source:
            "live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh",
          line: 238,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "当前公开脚本与 1.0.43 逐字节相同；PPPoE setup 从 JSON 读取 ac。",
        },
      ],
    },
    service: {
      description:
        "筛选 PPPoE 服务名。值非空时作为 rp_pppoe_service 交给插件，不能用它给接口命名。",
      dependencies: ["仅 PPPoE；名称由运营商服务决定。"],
      impact: "服务名不匹配可能使 PPPoE 发现或拨号失败。",
      evidence: [
        {
          source: "lib/netifd/proto/ppp.sh",
          line: 239,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "PPPoE setup 读取 service。",
        },
        {
          source: "lib/netifd/proto/ppp.sh",
          line: 253,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "非空 service 生成 rp_pppoe_service 参数。",
        },
        {
          source:
            "live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh",
          line: 239,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "当前公开脚本与 1.0.43 逐字节相同；PPPoE setup 读取 service。",
        },
      ],
    },
    keepalive: {
      description:
        "组合值包含 LCP 探测失败次数和间隔，可用空格或逗号分隔。通用 PPP 缺省为 5 20；只填次数时，间隔回退为 5。L2TP 不共享该整体缺省。",
      defaultValue: "通用 PPP：5 20；单值的间隔回退为 5。",
      unit: "失败次数 / 秒",
      dependencies: [
        "用于 PPP 系列；失败次数小于 1 时通用 PPP 不生成探测参数。",
      ],
      impact:
        "用于判断对端失联；次数小或间隔短会更快重连，也更容易在暂时丢包时断线。",
      evidence: [
        {
          source: "lib/netifd/proto/ppp.sh",
          line: 133,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "通用 PPP keepalive 为空回退为 5 20；分离失败次数与间隔，单值间隔回退为 5。",
          endLine: 140,
        },
        {
          source: "lib/netifd/proto/ppp.sh",
          line: 148,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "非空失败次数生成 lcp-echo-interval 与 lcp-echo-failure。",
        },
        {
          source: "lib/netifd/proto/l2tp.sh",
          line: 69,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "L2TP 分离间隔，单值回退为 5；仅非空 keepalive 生成 LCP 参数。",
          endLine: 72,
        },
        {
          source:
            "live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh",
          line: 133,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "当前公开脚本与 1.0.43 逐字节相同；通用 PPP keepalive 为空回退为 5 20；分离失败次数与间隔，单值间隔回退为 5。",
          endLine: 140,
        },
      ],
    },
    demand: {
      description:
        "设置通用 PPP 按需拨号的空闲时间。仅大于 0 时启用 demand、idle 与预编译活动流量过滤器；空值或 0 不生成按需参数。",
      defaultValue: "0（只证明不启用 demand；不承诺其他重连策略）。",
      unit: "秒",
      range: "非负整数；正值启用按需拨号。",
      dependencies: ["用于通用 PPP；persist、maxfail 等仍独立影响重连。"],
      impact:
        "正值允许空闲后断线，后续流量触发拨号；可能增加首次请求的等待时间。",
      evidence: [
        {
          source: "lib/netifd/proto/ppp.sh",
          line: 109,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "demand 为空按 0 判断；大于 0 时生成 demand idle 参数，否则清空。",
          endLine: 113,
        },
        {
          source: "lib/netifd/proto/ppp.sh",
          line: 156,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "生成的 demand 参数被传给 pppd。",
        },
        {
          source:
            "live-inspection/field-help-live-1.0.64/lib/netifd/proto/ppp.sh",
          line: 109,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "当前公开脚本与 1.0.43 逐字节相同；demand 为空按 0 判断；大于 0 时生成 demand idle 参数，否则清空。",
          endLine: 113,
        },
      ],
    },
    reqaddress: {
      description:
        "控制 DHCPv6 客户端的地址请求策略：try 尝试、force 要求、none 不请求。非空值转换为 odhcp6c 的 -N 参数。",
      range: "try / force / none。",
      dependencies: ["仅 dhcpv6；与 reqprefix 独立。"],
      impact:
        "影响是否请求 IPv6 地址；与前缀请求独立，不请求地址并不等于不请求委派前缀。",
      evidence: [
        {
          source: "lib/netifd/proto/dhcpv6.sh",
          line: 10,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "reqaddress 声明只允许 try、force、none。",
        },
        {
          source: "lib/netifd/proto/dhcpv6.sh",
          line: 132,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "非空 reqaddress 生成 -N 参数。",
        },
      ],
    },
    reqprefix: {
      description:
        "控制 DHCPv6 委派前缀请求。auto 或空值转为请求长度 0；no 不生成 -P 参数；数字长度依源声明为 0–64。",
      defaultValue: "auto 等效行为：请求长度 0。",
      unit: "位（数字值）",
      range: "auto / no / 0–64。",
      dependencies: ["仅 dhcpv6；下游还需 delegate、ip6assign 及 IPv6 服务。"],
      impact:
        "影响上游是否以及按什么长度提供下游前缀；上游仍可拒绝或分配不同长度。",
      evidence: [
        {
          source: "lib/netifd/proto/dhcpv6.sh",
          line: 11,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "reqprefix 声明允许 auto、no 或 0–64。",
        },
        {
          source: "lib/netifd/proto/dhcpv6.sh",
          line: 134,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "空值或 auto 转为 0，除 no 外生成 -P 前缀请求参数。",
          endLine: 135,
        },
      ],
    },
  },
  device: {
    name: {
      description:
        "设置设备名称。核心 UCI device 分支要求 name 非空，再按 type 选择实现，以该名称创建或更新设备对象；接口引用此名称取得承载设备。",
      dependencies: ["interface.device、bridge-vlan.device 等引用需同步。"],
      flags: ["hardware-dependent"],
      impact:
        "名称变更会改变接口或 VLAN 的引用目标；其他章节仍用旧名时可能失去承载设备。",
      evidence: [
        {
          source: "sbin/devstatus",
          line: 10,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "设备状态查询把命令参数作为 name 提交到 network.device status。",
          endLine: 12,
        },
        {
          source: "etc/init.d/network",
          line: 34,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "network 服务运行 netifd 来管理网络。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 183,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心 device 章节读取非空 name，并用于创建/更新设备对象。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 65172,
          },
        },
      ],
    },
    type: {
      description:
        "选择设备实现类型。netifd 设备参数表接收 type 字符串，二进制还包含 bridge、8021q、8021ad 设备实现对象；普通网卡与虚拟设备的创建条件不同。",
      dependencies: ["bridge 使用 ports；8021q/8021ad 使用 ifname 和 vid。"],
      flags: ["hardware-dependent"],
      impact: "改变设备类型会重建网桥或 VLAN，可能中断依赖它的接口。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 78,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "设备参数表将 type 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173584,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 143,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 二进制包含 8021q 类型名称。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 162245,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 144,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 二进制包含 8021ad 类型名称。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 162238,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 165,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "8021q 与 8021ad 对象绑定同一 VLAN 参数描述符和实现回调。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 177700,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 183,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "device 章节读取type并选择设备实现，缺少name则不继续。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 65172,
          },
        },
      ],
    },
    ports: {
      description:
        "设置 bridge 的成员设备列表。网桥参数表明确将 ports 解析为数组，与逻辑接口的 IP 地址列表无关。",
      dependencies: [
        "type=bridge；成员需是存在的设备，VLAN 成员标签另由 bridge-vlan 配置。",
      ],
      flags: ["hardware-dependent"],
      impact: "成员加入同一二层广播域；移除当前管理端口会断开连接。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 109,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥参数表将 ports 声明为 array。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173852,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 161,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥重配回调实际解析并保存 ports 参数。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 85052,
          },
        },
      ],
    },
    ifname: {
      description:
        "指定 802.1Q/802.1ad VLAN 的父设备。VLAN 参数表接受字符串，与旧式 interface.ifname 的逻辑接口成员列表不是同一层设置。",
      dependencies: ["type=8021q 或 8021ad，并配合 vid。"],
      flags: ["hardware-dependent"],
      impact:
        "VLAN 流量在所选父设备上传送；父设备改错会让该 VLAN 的接口失去连接。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 126,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "VLAN 参数表将 ifname 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 174208,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 165,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "8021q 与 8021ad 对象绑定同一 VLAN 参数描述符和实现回调。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 177700,
          },
        },
      ],
    },
    vid: {
      description:
        "设置 802.1Q/802.1ad VLAN 标签标识。源参数表以字符串解析 vid；面板数字输入只覆盖常规 VLAN 编号。",
      range: "常规可用 VLAN ID 为 1–4094；参数表未证明更窄的硬件范围。",
      dependencies: ["type=8021q/8021ad 与父设备 ifname；上游需支持相同标签。"],
      flags: ["hardware-dependent"],
      impact: "改变标签会让报文进入不同 VLAN；两端的标签约定必须一致。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 127,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "VLAN 参数表将 vid 声明为 string，而不是整数。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 174216,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 165,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "8021q 与 8021ad 对象绑定同一 VLAN 参数描述符和实现回调。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 177700,
          },
        },
      ],
    },
    mtu: {
      description:
        "设置设备层最大传输单元。netifd 设备表将 mtu 作为整数接收；这不等同于 PPP 协议内部的 mru。",
      unit: "字节",
      dependencies: ["受底层网卡、封装与上游链路限制；接口 mtu 另有独立配置。"],
      flags: ["hardware-dependent"],
      impact:
        "会影响所有使用该设备的逻辑接口；路径不支持时可能出现大包丢失或分片。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 79,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "设备参数表将 mtu 声明为 int32。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173592,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 164,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "通用设备参数消费读取并应用 mtu 对应槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 72092,
          },
        },
      ],
      range: "核心消费仅接受大于 67 的值；链路/硬件上限另行检查。",
    },
    macaddr: {
      description:
        "为设备层设置 MAC 地址覆盖。设备参数表接收字符串；厂商 LAN/WAN 接口的硬件地址回填不证明这里也有相同回退。",
      dependencies: ["设备驱动需允许地址修改；与接口层 macaddr 一起检查。"],
      flags: ["hardware-dependent"],
      range: "原生 MAC 地址格式。",
      impact:
        "影响引用此设备的二层身份；重复 MAC 可能造成交换表抖动和连接异常。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 81,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "设备参数表将 macaddr 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173608,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 164,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "通用设备参数消费读取并应用 macaddr 对应槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 72092,
          },
        },
      ],
    },
    ipv6: {
      description:
        "控制此设备的 IPv6 能力，设备参数表接收布尔值。netifd 包含 per-device disable_ipv6 内核开关路径；这是设备层设置，不是 PPP 的 auto 模式。",
      dependencies: ["内核 IPv6 支持及引用此设备的逻辑接口。"],
      impact: "停用会影响该设备上的 IPv6 地址与相关接口；不会直接关闭 IPv4。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 84,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "设备参数表将 ipv6 声明为 bool/int8。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173632,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 142,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 包含按设备写入 disable_ipv6 的 sysctl 路径。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 166234,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 164,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "通用设备参数消费读取并应用 ipv6 对应槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 72092,
          },
        },
      ],
    },
    stp: {
      description:
        "在 bridge 上控制生成树协议。网桥参数表接收布尔值，核心含 stp_state 写入目标；用于防止冗余二层链路形成环路。",
      dependencies: ["type=bridge；拓扑中的其他交换设备也需考虑 STP。"],
      impact:
        "启用可能改变端口转发状态及收敛时间；停用后不能依赖 STP 阻断环路。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 110,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥参数表将 stp 声明为 bool/int8。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173860,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 136,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 包含网桥 stp_state 内核属性路径。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 166274,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/network.lua",
          line: 612,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "LuCI 网络模型从 brctl show 读取 STP 状态并记录 yes/false。",
          endLine: 638,
        },
        {
          source: "docs/field-help-network-research.md",
          line: 161,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥重配回调实际解析并保存 stp 参数。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 85052,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 162,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥重配在覆盖参数前将 stp 初始化为 0。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 85116,
          },
        },
      ],
      defaultValue: "0（网桥重配初始化值，未提供该项时保留）。",
    },
    igmp_snooping: {
      description:
        "在网桥上根据 IGMP 成员关系限制 IPv4 组播转发。网桥表接受布尔值，核心含 multicast_snooping 属性路径。",
      dependencies: [
        "type=bridge；与网络中的 IGMP 查询器、multicast_querier 及 IPTV 拓扑相关。",
      ],
      impact:
        "可减少无关端口的组播流量；成员信息或查询器缺失时可能影响 IPTV/组播接收。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 113,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥参数表将 igmp_snooping 声明为 bool/int8。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173884,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 139,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 包含网桥 multicast_snooping 属性路径。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 166372,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 161,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥重配回调实际解析并保存 igmp_snooping 参数。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 85052,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 162,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥重配在覆盖参数前将 igmp_snooping 初始化为 0。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 85116,
          },
        },
      ],
      defaultValue: "0（网桥重配初始化值，未提供该项时保留）。",
    },
    multicast_querier: {
      description:
        "控制网桥是否主动发送组播成员查询。参数表将此项作为布尔值接收，核心含 multicast_querier 内核属性路径。",
      dependencies: ["type=bridge；主要与组播侦听、IGMP/MLD 查询有关。"],
      impact:
        "可维持组播成员信息；与外部查询器的角色应协调，避免误判组播服务问题。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 118,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥参数表将 multicast_querier 声明为 bool/int8。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173924,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 140,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 包含网桥 multicast_querier 属性路径。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 166426,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 161,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥重配回调实际解析并保存 multicast_querier 参数。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 85052,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 163,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "igmp_snooping 同时赋给 querier，后续 multicast_querier 显式值可覆盖。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 85252,
          },
        },
      ],
      defaultValue: "未单独设置时：跟随 igmp_snooping；两者都未设置则为 0。",
    },
    bridge_empty: {
      description:
        "允许无成员时保持网桥存在。网桥参数表接受布尔值；访客网络脚本也生成同名选项，但在旧式 interface 章节中。",
      dependencies: ["type=bridge；与 ports 或后续动态成员配合。"],
      impact:
        "即使没有有线成员，桥设备仍可作为后续无线成员的接入点；不会自动增加成员。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 117,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥参数表将 bridge_empty 声明为 bool/int8。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173916,
          },
        },
        {
          source: "usr/sbin/guestwifi.sh",
          line: 87,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商访客 network 创建旧式 interface 网桥，并设 bridge_empty=1。",
          endLine: 91,
        },
        {
          source: "docs/field-help-network-research.md",
          line: 161,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥重配回调实际解析并保存 bridge_empty 参数。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 85052,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 162,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥重配在覆盖参数前将 bridge_empty 初始化为 0。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 85116,
          },
        },
      ],
      defaultValue: "0（网桥重配初始化值，未提供该项时保留）。",
    },
    vlan_filtering: {
      description:
        "控制网桥是否按 VLAN 成员关系过滤报文。网桥参数表接受布尔值，核心含 vlan_filtering 内核属性路径。",
      dependencies: [
        "type=bridge；与 bridge-vlan 的 VLAN、端口标签和 local 配合。",
      ],
      impact:
        "启用后缺失的 VLAN/端口成员可能被隔离，包含管理流量；关闭则不能依赖此桥实施 VLAN 隔离。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 124,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥参数表将 vlan_filtering 声明为 bool/int8。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173972,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 141,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 包含网桥 vlan_filtering 属性路径。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 167086,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 161,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥重配回调实际解析并保存 vlan_filtering 参数。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 85052,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 162,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥重配在覆盖参数前将 vlan_filtering 初始化为 0。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 85116,
          },
        },
      ],
      defaultValue: "0（网桥重配初始化值，未提供该项时保留）。",
    },
    ageing_time: {
      description:
        "设置动态学习的网桥 MAC 转发表项老化时间。网桥参数表接受整数，核心含 ageing_time 属性目标；不会设置 ARP 或 DHCP 租约期限。",
      unit: "秒",
      dependencies: ["type=bridge；仅影响动态学习的 MAC 表项。"],
      impact:
        "值较短会更快忘记静默终端的端口并增加未知单播泛洪；较长保留旧端口映射更久。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 114,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥参数表将 ageing_time 声明为 int32。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173892,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 138,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 包含网桥 ageing_time 内核属性路径。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 167180,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 161,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥重配回调实际解析并保存 ageing_time 参数。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 85052,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 177,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "实际写入前将 ageing_time 乘100，输入为秒。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 141060,
          },
        },
      ],
    },
    priority: {
      description:
        "设置网桥的 STP 桥优先级，不是 IP 路由优先级。网桥参数表接受整数，核心含 bridge/priority 属性路径。",
      range: "桥优先级为 16 位值（0–65535）；硬件/拓扑规则另行检查。",
      dependencies: ["type=bridge；启用 stp 后才参与生成树选举。"],
      impact:
        "参与根桥选举；较小优先级可使该桥更容易成为根桥，可能改变二层路径。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 112,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥参数表将 priority 声明为 int32。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173876,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 137,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 包含网桥 priority 内核属性路径。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 167136,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 161,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网桥重配回调实际解析并保存 priority 参数。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 85052,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 162,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "priority 初始化指令写入16位32767，非运行配置采样。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 85116,
          },
        },
      ],
      defaultValue: "32767（网桥重配初始化的 16 位值）。",
    },
    disabled: {
      description:
        "面板把此项解释为停用设备，但 RN02 设备参数表使用 enabled 布尔项，没有在该表找到 disabled。不能套用 interface 或无线 disabled 的效果。",
      flags: ["version-dependent"],
      summary:
        "设备表使用 enabled；此 disabled 字段的停用效果未在 1.0.43 证实。",
      discovery:
        "已检查 netifd 设备与网桥参数表、lib/network/config.sh、lib/miwifi 和反编译网络 Lua；未找到 device.disabled 的设备消费，已找到不同键 device.enabled。",
      impact:
        "保存会保留此原生字段；是否停用设备尚未证实，不能据此判断设备已停止。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 83,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "设备参数表的启用键为 enabled，类型为 bool/int8。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173624,
          },
        },
        {
          source: "etc/init.d/network",
          line: 34,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网络设备由 netifd 服务管理。",
        },
      ],
    },
  },
  "bridge-vlan": {
    device: {
      description:
        "指定此 VLAN 条目所属网桥。核心在 bridge-vlan 分支按 device 名查找设备对象，并检查其网桥状态后读取 VLAN 成员参数。",
      dependencies: [
        "应引用 type=bridge 的设备；桥需配置相应端口及 VLAN 过滤。",
      ],
      impact:
        "绑定到错误网桥会让 VLAN 规则不作用于预期端口；可能影响管理流量的 VLAN 归属。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 135,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 二进制包含 bridge-vlan 类型名称。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 159091,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 74,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "bridge-vlan 参数表接收 VLAN 编号。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173552,
          },
        },
        {
          source: "etc/init.d/network",
          line: 34,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "网络服务启动 netifd。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 178,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "bridge-vlan 消费按 device 名查找并要求网桥设备。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 67432,
          },
        },
      ],
    },
    vlan: {
      description:
        "设置桥接 VLAN 成员条目的 VLAN 编号。netifd 的 bridge-vlan 表以整数解析 vlan；它不是旧交换芯片的 VLAN 表索引。",
      range: "核心接受 1–4095；常规业务 VLAN ID 为 1–4094，4095 为协议保留值。",
      dependencies: [
        "关联 device 必须是桥；与 ports、local 及桥 vlan_filtering 配合。",
      ],
      impact: "相同编号的成员共享该 VLAN；改动会改变端口和本机收发的二层隔离。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 74,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "bridge-vlan 参数表将 vlan 声明为 int32。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173552,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 172,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实桥接 VLAN 消费解析 vlan。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 67564,
          },
        },
      ],
    },
    ports: {
      description:
        "设置桥接 VLAN 的端口列表及标签方式。核心按冒号拆标记：缺省去标签，t 改为带标签，* 设为端口主 VLAN；u 不额外改变缺省去标签状态。",
      dependencies: ["成员必须属于目标桥；与 VLAN 编号及对端标签方式匹配。"],
      flags: ["hardware-dependent"],
      impact:
        "错误标签或主 VLAN 会让终端进入错误网络，或让其无法接收预期报文。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 76,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "bridge-vlan 参数表将 ports 声明为 array。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173568,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 172,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实桥接 VLAN 消费解析 ports。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 67564,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 173,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心按冒号拆端口标记：缺省untagged，t设tagged，*设PVID。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 68020,
          },
        },
      ],
      defaultValue: "端口未写标签标记：去标签；没有自动成员端口回退。",
    },
    local: {
      description:
        "控制路由器本机是否参与此桥接 VLAN。bridge-vlan 参数表将 local 解析为布尔值，与端口间转发设置分开。",
      dependencies: [
        "与桥 VLAN 及本机 VLAN 子接口配合；不等同于防火墙 input 策略。",
      ],
      impact:
        "关闭本机参与可能让该 VLAN 的终端不能访问路由器服务；成员端口之间仍由桥规则决定转发。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 75,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "bridge-vlan 参数表将 local 声明为 bool/int8。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173560,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 172,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实桥接 VLAN 消费解析 local。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 67564,
          },
        },
      ],
      defaultValue: "1（创建 bridge-vlan 条目时未提供 local 的初始化值）。",
    },
  },
  switch: {
    name: {
      description:
        "选择 swconfig 要加载的交换芯片。setup_switch_dev 先读 name，空值使用章节名；若同名网卡存在，先将其启用再加载 network。",
      defaultValue: "未设置时：该 switch 章节名称。",
      dependencies: ["设备必须可由 swconfig 驱动识别。"],
      flags: ["legacy", "hardware-dependent"],
      impact: "选错芯片会把配置送到错误设备；此路径影响物理端口的转发。",
      evidence: [
        {
          source: "lib/network/switch.sh",
          line: 4,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "setup_switch_dev 从章节读 name，空值回退为章节名；先启用同名网卡再运行 swconfig dev name load network。",
          endLine: 9,
        },
      ],
    },
    reset: {
      description:
        "在加载旧式交换芯片配置时请求重置。RN02 的 network 校验将其定义为布尔值，实际选项由 swconfig 和芯片驱动应用。",
      dependencies: ["旧式 swconfig 交换芯片及其驱动提供 reset 选项。"],
      flags: ["legacy", "hardware-dependent"],
      impact:
        "重置可能短暂中断芯片端口的流量并清理运行状态；不等同于路由器恢复出厂设置。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 134,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "network switch 校验声明 reset 为 bool。",
        },
        {
          source: "lib/network/switch.sh",
          line: 9,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "交换芯片配置交给 swconfig dev name load network。",
        },
      ],
    },
    enable_vlan: {
      description:
        "请求交换芯片启用 VLAN 隔离。network 的 switch 校验接受布尔值，随后由 swconfig 加载整个 network 配置。",
      dependencies: ["swconfig 驱动需支持 enable_vlan；与 switch_vlan 配合。"],
      flags: ["legacy", "hardware-dependent"],
      impact:
        "影响物理端口隔离；切换后需让 switch_vlan 成员与 CPU 口正确匹配，否则会失去连接。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 133,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "network switch 校验声明 enable_vlan 为 bool。",
        },
        {
          source: "lib/network/switch.sh",
          line: 9,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "setup_switch_dev 运行 swconfig dev name load network。",
        },
      ],
    },
    enable_mirror_rx: {
      description:
        "面板将此项解释为接收流量镜像开关。交换芯片配置经 swconfig 转交驱动；RN02 的脚本校验和厂商端口配置未声明此镜像键。",
      dependencies: ["需芯片驱动支持，并配合镜像来源和监控端口。"],
      flags: ["legacy", "hardware-dependent", "version-dependent"],
      summary: "接收镜像请求；RN02 静态消费链未证实驱动支持此键。",
      discovery:
        "已搜索 etc/init.d/network、lib/network/switch.sh、lib/miwifi 的端口/芯片脚本及全部可读 Lua；未找到 switch.enable_mirror_rx 的读写。swconfig 动态驱动属性仍可能提供此键，未在线查询。",
      impact:
        "若驱动实现此键，会把接收报文复制到监控端口，增加端口负载并暴露流量副本；当前支持未证实。",
      evidence: [
        {
          source: "lib/network/switch.sh",
          line: 9,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "交换芯片选项经 swconfig load network 下发。",
        },
        {
          source: "etc/init.d/network",
          line: 130,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "switch 校验只显式声明 name、enable、enable_vlan、reset 及 MIB 选项。",
          endLine: 136,
        },
      ],
    },
    enable_mirror_tx: {
      description:
        "面板将此项解释为发送流量镜像开关。当前可见 swconfig 加载链未单独声明或读取此镜像键，效果由芯片驱动决定。",
      dependencies: [
        "需驱动支持，并配合 mirror_source_port、mirror_monitor_port。",
      ],
      flags: ["legacy", "hardware-dependent", "version-dependent"],
      summary: "发送镜像请求；此芯片驱动是否接受该键尚未静态证实。",
      discovery:
        "已搜索 network init、lib/network/switch.sh、厂商 lib/miwifi 端口脚本及反编译 Lua；未找到 switch.enable_mirror_tx 的具体消费。未运行 swconfig 或读取运行时属性。",
      impact:
        "若驱动支持，会复制发送流量到监控端口，可能占用监控链路带宽；未证明 RN02 会启用该功能。",
      evidence: [
        {
          source: "lib/network/switch.sh",
          line: 9,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "交换芯片选项由 swconfig load network 交给驱动。",
        },
        {
          source: "etc/init.d/network",
          line: 130,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "可见 switch 校验声明不包含镜像发送开关。",
          endLine: 136,
        },
      ],
    },
    mirror_source_port: {
      description:
        "面板语义为被镜像的芯片端口编号，不是 Linux 网卡序号。静态加载链未证明 RN02 驱动读取此字段。",
      dependencies: ["依赖芯片端口映射、镜像方向开关及驱动属性。"],
      flags: ["legacy", "hardware-dependent", "version-dependent"],
      summary: "芯片镜像来源端口；实际支持和端口编号范围未证实。",
      discovery:
        "已检查 switch 加载、network 校验、lib/miwifi/lib_port_map.sh 及架构端口脚本和反编译 Lua；未找到 switch.mirror_source_port 的解析或应用，不能提供硬件范围。",
      impact:
        "若驱动接受，会决定哪一端口的流量被复制；编号错误可能监控到其他端口或无数据。",
      evidence: [
        {
          source: "lib/network/switch.sh",
          line: 9,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "交换芯片配置通过 swconfig load network 应用。",
        },
        {
          source: "etc/init.d/network",
          line: 130,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "switch 的脚本校验声明未包含 mirror_source_port。",
          endLine: 136,
        },
      ],
    },
    mirror_monitor_port: {
      description:
        "面板语义为接收镜像副本的芯片监控端口。静态消费者未证明该键是否由 RN02 驱动提供，不能按普通 LAN 口序号推定。",
      dependencies: ["需镜像方向、来源端口及芯片驱动支持。"],
      flags: ["legacy", "hardware-dependent", "version-dependent"],
      summary: "芯片镜像监控端口；实际支持和硬件编号范围未证实。",
      discovery:
        "已搜索 lib/network/switch.sh、network init、lib/miwifi 端口脚本和全部可读 Lua；未定位 switch.mirror_monitor_port 的读取或硬件属性表，未在线探测端口。",
      impact:
        "若驱动支持，此端口会收到流量副本；监控口承载业务时可能出现额外负载或流量泄露。",
      evidence: [
        {
          source: "lib/network/switch.sh",
          line: 9,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "swconfig 接收整个 network 的交换芯片配置。",
        },
        {
          source: "etc/init.d/network",
          line: 130,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "switch 的脚本校验未列 mirror_monitor_port。",
          endLine: 136,
        },
      ],
    },
  },
  switch_vlan: {
    device: {
      description:
        "指定旧式 swconfig VLAN 条目所属交换芯片。厂商端口映射会生成 switch_vlan 章节并把芯片名称写入 device。",
      dependencies: ["名称需对应 switch 章节中的交换芯片。"],
      flags: ["legacy", "generated", "hardware-dependent"],
      impact: "条目送给所选芯片；名称错误会使成员划分不作用于预期端口。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 142,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "switch_vlan 校验将 device 声明为 string。",
        },
        {
          source: "lib/miwifi/lib_port_map.sh",
          line: 54,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商 VLAN 生成函数创建 switch_vlan 并写入 device、ports、vlan、vid。",
          endLine: 61,
        },
      ],
    },
    vlan: {
      description:
        "设置旧式交换芯片的 VLAN 表条目编号，不应直接当作 bridge-vlan 的标签号。厂商生成路径恰好把 vlan 和 vid 写为同一值，但二者仍是独立键。",
      range: "非负整数；芯片表容量范围未在该脚本声明。",
      dependencies: [
        "与 device、vid、ports 配合；由 swconfig 驱动解释表索引。",
      ],
      flags: ["legacy", "generated", "hardware-dependent"],
      impact:
        "决定加载哪条 VLAN 表记录；改动可能改变物理端口和 CPU 口间的连通性。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 143,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "switch_vlan 校验把 vlan 声明为 uinteger。",
        },
        {
          source: "lib/miwifi/lib_port_map.sh",
          line: 58,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商通用生成路径把 vlan 和 vid 同时设置为函数参数 vid。",
          endLine: 59,
        },
      ],
    },
    vid: {
      description:
        "设置交换芯片 VLAN 表记录的实际标签标识。厂商生成代码单独写入 vid；RN02 架构脚本还保留一个 vid=0 的内部条目，面板范围不是固件全部内部用途。",
      range: "常规业务 VLAN ID 为 1–4094；厂商内部脚本另使用 0。",
      dependencies: ["与 vlan 表索引、端口成员、CPU 口的网卡 VLAN 标签配合。"],
      flags: ["legacy", "generated", "hardware-dependent"],
      impact:
        "标签与父网卡 VLAN 子接口需一致；修改可能隔离物理端口或上游链路。",
      evidence: [
        {
          source: "lib/miwifi/lib_port_map.sh",
          line: 59,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商 switch_vlan 生成路径单独写入 vid。",
        },
        {
          source: "lib/miwifi/arch/lib_arch_port_map.sh",
          line: 48,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "架构脚本生成 vlan0，设置 vlan=0、vid=0 和内部成员端口。",
          endLine: 55,
        },
      ],
    },
    ports: {
      description:
        "设置旧式交换芯片的端口编号列表及标签标记。network 校验要求 list(ports)，厂商映射还会把 CPU 口加入成员列表。",
      dependencies: ["端口编号由芯片/板级映射决定；t 等原生标签格式需保留。"],
      flags: ["legacy", "generated", "hardware-dependent"],
      impact:
        "端口成员决定 VLAN 可达性；漏掉 CPU 口可能让终端不能访问路由器或上网。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 144,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "switch_vlan 校验把 ports 声明为 list(ports)。",
        },
        {
          source: "lib/miwifi/lib_port_map.sh",
          line: 143,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商 VLAN 生成收集成员端口，并追加 cpu_port 后配置 VLAN。",
          endLine: 148,
        },
      ],
    },
  },
  route: {
    interface: {
      description:
        "选择此 IPv4 静态路由的出口逻辑接口。netifd 路由表接收字符串；填写 network 中的接口名称而不是物理端口编号。",
      dependencies: ["逻辑接口必须存在；网关应由该接口可达。"],
      impact:
        "该接口提供 IPv4 路由的承载设备；接口不存在或未就绪时，目标路径可能不可用。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 78,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "route 校验将 interface 声明为 string。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 40,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 interface 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172852,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 interface 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
      ],
    },
    target: {
      description:
        "设置 IPv4 路由匹配的目标网络或主机前缀。前缀越长，匹配越具体；不是路由器本机地址。",
      range: "IPv4 地址或 CIDR；前缀长度 0–32。",
      dependencies: [
        "IPv4 分开填写地址时可配 netmask；IPv6 使用自身前缀长度。",
      ],
      impact:
        "改变哪些 IPv4 目标走此路径；覆盖当前管理目标的路由可能改变回程。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 79,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "route 的 target 校验要求 cidr4。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 41,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 target 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172860,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 target 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
      ],
    },
    gateway: {
      description:
        "设置 IPv4 静态路由的下一跳。核心参数表接收 gateway 字符串，源校验限制对应地址族。",
      dependencies: [
        "下一跳需由 interface 到达；特殊链路外网关可检查 onlink。",
      ],
      range: "IPv4 网关地址。",
      impact:
        "报文经该下一跳离开接口；错误或不可达的网关会使匹配的 IPv4 目的地失联。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 81,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "route gateway 校验要求 ip4addr。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 43,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 gateway 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172876,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 gateway 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
      ],
    },
    metric: {
      description:
        "为此 IPv4 路由设置非负度量。相同目标和前缀长度的多条路由可用度量区分优先级。",
      range: "非负整数。",
      dependencies: ["只有在目标/前缀等路由条件相当时比较；与路由表选择独立。"],
      impact: "影响同目标 IPv4 路由的选择；不会改变策略规则的查表优先级。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 82,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "route metric 校验要求 uinteger。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 44,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 metric 声明为 int32。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172884,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 metric 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
      ],
    },
    table: {
      description:
        "把此 IPv4 路由放入指定路由表。可用表编号或名称；路由表名称需在系统中能够解析。",
      range: "源校验：数字 0–65535 或表名称。",
      dependencies: ["与 rule.lookup、内核查表规则及其他表中的路由配合。"],
      impact:
        "不在当前查询链中的表不会自动成为 IPv4 默认出口；策略 rule/rule6 可通过 lookup 使用它。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 84,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "route table 校验允许 0–65535 的数字或字符串名称。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 47,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 table 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172908,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 table 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
      ],
    },
    type: {
      description:
        "选择此 IPv4 路由的动作类型。unicast 指向可转发路径；blackhole 静默丢弃、unreachable 返回不可达、prohibit 表示禁止；throw 让查询继续其他规则。",
      dependencies: [
        "type=local 表示本机目的地址，不是普通出口；网关参数只适用于相应路由类型。",
      ],
      range:
        "unicast / local / blackhole / unreachable / prohibit / throw（面板保留项）。",
      discovery:
        "已定位 route.type 的真实转换调用及多种特殊类型名称；本轮未逐项记录全部保留类型的数值和未设置回退，不提供统一缺省。",
      impact:
        "特殊类型可以有意阻断或跳过 IPv4 路径，不能都按普通网关路由理解。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 50,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 type 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172932,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 147,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 二进制包含 blackhole 路由/动作类型名称。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 165308,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 150,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 二进制包含 throw 路由/动作类型名称。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 165327,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 type 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 182,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "实际路由type通过核心类型转换函数，失败进入日志错误路径。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 43272,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 184,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由type转换函数比较特殊类型名称并转换为内核类型数值。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 126876,
          },
        },
      ],
    },
    source: {
      description:
        "设置此路由的来源地址或来源前缀参数。核心按路由地址族解析来源地址，并保存可选前缀长度；不能拿它替代策略规则的 src 条件。",
      dependencies: ["与接口已有地址及同族路由配合；不能替代 rule/rule6.src。"],
      discovery:
        "已定位 route.source 的地址及来源前缀解析；最终在 IPv4/IPv6 中映射为内核首选来源地址还是源相关路由的差异尚未逐项核对，因此不承诺两族等价。",
      impact:
        "可能影响 IPv4 源地址选择或源相关路由条件；错误源参数会破坏回程或查表匹配。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 48,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 source 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172916,
          },
        },
        {
          source: "lib/netifd/netifd-proto.sh",
          line: 313,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "协议路由更新在 source 非空时向核心提交 source 字符串。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 source 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 180,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "静态 route.source 地址按族解析，保存可选来源前缀长度；无/时IPv4为32、IPv6为128。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42936,
          },
        },
      ],
    },
    mtu: {
      description:
        "为此 IPv4 路由设置路径 MTU。源校验接受非负整数；这是路由层属性，不是网卡设备或 PPP 接收单元设置。",
      unit: "字节",
      dependencies: ["受底层设备及真实路径 MTU 限制。"],
      impact:
        "限制匹配此路由的 IPv4 报文尺寸；太小降低效率，太大可能导致路径 MTU 故障。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 83,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "route mtu 校验要求 uinteger。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 45,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 mtu 声明为 int32。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172892,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 mtu 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
      ],
    },
    onlink: {
      description:
        "把此 IPv4 下一跳视为直连，即使它不落在已配置的接口前缀内。核心路由表明确接收布尔参数 onlink。",
      dependencies: [
        "需要 gateway 和正确的 interface；链路层仍必须能到达下一跳。",
      ],
      impact:
        "允许特殊下一跳的路由安装，但不会让本来无法到达的 IPv4 网关自动可达。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 49,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 onlink 声明为 bool/int8。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172924,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 onlink 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 181,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "onlink 非零给实际路由对象设置独立标记。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42980,
          },
        },
      ],
    },
    disabled: {
      description:
        "标记此 IPv4 静态路由不使用。核心路由参数表单独声明 disabled 布尔项；不会停用其关联接口。",
      dependencies: ["仅作用于该路由；可与其他表或动态路由并存。"],
      impact:
        "停用后流量只能使用其他匹配路由；若无替代路径，相应 IPv4 目标会不可达。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 52,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 disabled 声明为 bool/int8。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172948,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 disabled 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
      ],
    },
    netmask: {
      description:
        "设置 IPv4 target 的掩码，用于确定静态路由匹配范围；如果 target 已用 CIDR，还应保持两者表达一致。",
      range: "IPv4 掩码，等价前缀长度为 0–32。",
      dependencies: ["与 IPv4 target 配合；不适用于 route6。"],
      impact: "掩码过宽可能截走其他网段的流量；过窄则仅匹配部分目标。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 80,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "route 的 netmask 校验要求 netmask4。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 42,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 netmask 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172868,
          },
        },
      ],
    },
  },
  route6: {
    interface: {
      description:
        "选择此 IPv6 静态路由的出口逻辑接口。netifd 路由表接收字符串；填写 network 中的接口名称而不是物理端口编号。",
      dependencies: ["逻辑接口必须存在；网关应由该接口可达。"],
      impact:
        "该接口提供 IPv6 路由的承载设备；接口不存在或未就绪时，目标路径可能不可用。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 90,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "route6 校验将 interface 声明为 string。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 40,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 interface 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172852,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 interface 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
      ],
    },
    target: {
      description:
        "设置 IPv6 路由匹配的目标网络或主机前缀。前缀越长，匹配越具体；不是路由器本机地址。",
      range: "IPv6 地址或 CIDR；前缀长度 0–128。",
      dependencies: ["IPv6 无单独 netmask 字段；前缀写入 target。"],
      impact:
        "改变哪些 IPv6 目标走此路径；覆盖当前管理目标的路由可能改变回程。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 91,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "route6 的 target 校验要求 cidr6。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 41,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 target 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172860,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 target 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
      ],
    },
    gateway: {
      description:
        "设置 IPv6 静态路由的下一跳。核心参数表接收 gateway 字符串，源校验限制对应地址族。",
      dependencies: [
        "下一跳需由 interface 到达；特殊链路外网关可检查 onlink。",
      ],
      range: "IPv6 网关地址。",
      impact:
        "报文经该下一跳离开接口；错误或不可达的网关会使匹配的 IPv6 目的地失联。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 92,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "route6 gateway 校验要求 ip6addr。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 43,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 gateway 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172876,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 gateway 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
      ],
    },
    metric: {
      description:
        "为此 IPv6 路由设置非负度量。相同目标和前缀长度的多条路由可用度量区分优先级。",
      range: "非负整数。",
      dependencies: ["只有在目标/前缀等路由条件相当时比较；与路由表选择独立。"],
      impact: "影响同目标 IPv6 路由的选择；不会改变策略规则的查表优先级。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 93,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "route6 metric 校验要求 uinteger。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 44,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 metric 声明为 int32。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172884,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 metric 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
      ],
    },
    table: {
      description:
        "把此 IPv6 路由放入指定路由表。可用表编号或名称；路由表名称需在系统中能够解析。",
      range: "源校验：数字 0–65535 或表名称。",
      dependencies: ["与 rule6.lookup、内核查表规则及其他表中的路由配合。"],
      impact:
        "不在当前查询链中的表不会自动成为 IPv6 默认出口；策略 rule/rule6 可通过 lookup 使用它。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 95,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "route6 table 校验允许 0–65535 的数字或字符串名称。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 47,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 table 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172908,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 table 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
      ],
    },
    type: {
      description:
        "选择此 IPv6 路由的动作类型。unicast 指向可转发路径；blackhole 静默丢弃、unreachable 返回不可达、prohibit 表示禁止；throw 让查询继续其他规则。",
      dependencies: [
        "type=local 表示本机目的地址，不是普通出口；网关参数只适用于相应路由类型。",
      ],
      range:
        "unicast / local / blackhole / unreachable / prohibit / throw（面板保留项）。",
      discovery:
        "已定位 route6.type 的真实转换调用及多种特殊类型名称；本轮未逐项记录全部保留类型的数值和未设置回退，不提供统一缺省。",
      impact:
        "特殊类型可以有意阻断或跳过 IPv6 路径，不能都按普通网关路由理解。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 50,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 type 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172932,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 147,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 二进制包含 blackhole 路由/动作类型名称。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 165308,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 150,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netifd 二进制包含 throw 路由/动作类型名称。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 165327,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 type 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 182,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "实际路由type通过核心类型转换函数，失败进入日志错误路径。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 43272,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 184,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由type转换函数比较特殊类型名称并转换为内核类型数值。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 126876,
          },
        },
      ],
    },
    source: {
      description:
        "设置此路由的来源地址或来源前缀参数。核心按路由地址族解析来源地址，并保存可选前缀长度；不能拿它替代策略规则的 src 条件。",
      dependencies: ["与接口已有地址及同族路由配合；不能替代 rule/rule6.src。"],
      discovery:
        "已定位 route6.source 的地址及来源前缀解析；最终在 IPv4/IPv6 中映射为内核首选来源地址还是源相关路由的差异尚未逐项核对，因此不承诺两族等价。",
      impact:
        "可能影响 IPv6 源地址选择或源相关路由条件；错误源参数会破坏回程或查表匹配。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 48,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 source 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172916,
          },
        },
        {
          source: "lib/netifd/netifd-proto.sh",
          line: 313,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "协议路由更新在 source 非空时向核心提交 source 字符串。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 source 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 180,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "静态 route.source 地址按族解析，保存可选来源前缀长度；无/时IPv4为32、IPv6为128。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42936,
          },
        },
      ],
    },
    mtu: {
      description:
        "为此 IPv6 路由设置路径 MTU。源校验接受非负整数；这是路由层属性，不是网卡设备或 PPP 接收单元设置。",
      unit: "字节",
      dependencies: [
        "受真实路径 MTU 限制；IPv6 链路正常使用还需考虑 1280 字节最低 MTU。",
      ],
      impact:
        "限制匹配此路由的 IPv6 报文尺寸；太小降低效率，太大可能导致路径 MTU 故障。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 94,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "route6 mtu 校验要求 uinteger。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 45,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 mtu 声明为 int32。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172892,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 mtu 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
      ],
    },
    onlink: {
      description:
        "把此 IPv6 下一跳视为直连，即使它不落在已配置的接口前缀内。核心路由表明确接收布尔参数 onlink。",
      dependencies: [
        "需要 gateway 和正确的 interface；链路层仍必须能到达下一跳。",
      ],
      impact:
        "允许特殊下一跳的路由安装，但不会让本来无法到达的 IPv6 网关自动可达。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 49,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 onlink 声明为 bool/int8。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172924,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 onlink 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 181,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "onlink 非零给实际路由对象设置独立标记。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42980,
          },
        },
      ],
    },
    disabled: {
      description:
        "标记此 IPv6 静态路由不使用。核心路由参数表单独声明 disabled 布尔项；不会停用其关联接口。",
      dependencies: ["仅作用于该路由；可与其他表或动态路由并存。"],
      impact:
        "停用后流量只能使用其他匹配路由；若无替代路径，相应 IPv6 目标会不可达。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 52,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心路由参数表将 disabled 声明为 bool/int8。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 172948,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 170,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实路由消费解析 disabled 参数槽。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 42256,
          },
        },
      ],
    },
  },
  rule: {
    in: {
      description:
        "按入站逻辑接口匹配此 IPv4 策略规则。源校验接收字符串；它筛选报文进入的接口，不是设置出口。",
      dependencies: [
        "需已有 network 逻辑接口；与 out、src/dest 等条件一起匹配。",
      ],
      impact:
        "只让来自该接口的 IPv4 流量命中此规则；名称错误可能使规则不匹配。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 101,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule 校验声明 in 为 string。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 53,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 in 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173028,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 in 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    out: {
      description:
        "按出站逻辑接口匹配此 IPv4 策略规则，不直接指定转发网卡。路由出口仍由 lookup 或 action 决定。",
      dependencies: ["需已有逻辑接口及对应方向的规则条件。"],
      impact:
        "限制此规则匹配的 IPv4 流量范围；不要用此项替代静态路由的 interface。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 102,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule 校验声明 out 为 string。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 54,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 out 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173036,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 out 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    src: {
      description:
        "按 IPv4 来源地址或前缀匹配规则。来源条件筛选发送者，与路由条目 source 的源地址选择参数不同。",
      range: "IPv4 CIDR 前缀；前缀长度 0–32。",
      dependencies: ["与 dest、in/out、mark 一起匹配；invert 可反转条件。"],
      impact:
        "决定哪些 IPv4 发送者使用本规则；过宽前缀可能把无关终端引向同一出口或阻断动作。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 103,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule src 校验要求 cidr4。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 56,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 src 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173052,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 src 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    dest: {
      description:
        "按 IPv4 目标地址或前缀匹配规则。它选择要处理的目标流量，不会创建该目标的路由。",
      range: "IPv4 CIDR 前缀。",
      dependencies: ["与 lookup 表内的路由、优先级及其他匹配条件配合。"],
      impact:
        "决定访问哪些 IPv4 目的地时触发此查表或动作；对应表仍需有可用路由。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 104,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule dest 校验要求 cidr4。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 57,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 dest 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173060,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 dest 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    priority: {
      description:
        "设置此 IPv4 策略规则的处理顺序，数值较小先处理。核心规则表接收整数；此项不同于路由 metric。",
      range: "非负整数；源参数表未声明硬件式上限。",
      dependencies: ["应与其他内核规则协调；goto 使用此优先级作为跳转目标。"],
      impact: "可以让该规则先于其他 IPv4 查表规则生效，改变出口或阻断结果。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 58,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 priority 声明为 int32。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173068,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 priority 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    lookup: {
      description:
        "匹配后查询指定 IPv4 路由表。可填写数字编号或名称；它不会自动把路由加入该表。",
      range: "源校验：数字 0–65535 或路由表名称。",
      dependencies: [
        "配合 route.table 和该表的有效路由；与特殊 action 的组合需核对。",
      ],
      impact:
        "使匹配的 IPv4 流量查不同的路由表；表内无有效路径时，后续结果取决于其他规则。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 108,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule lookup 校验允许 0–65535 或字符串。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 61,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 lookup 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173092,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 lookup 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    mark: {
      description:
        "按数据包标记匹配此 IPv4 规则。使用原生标记及可选掩码；本规则只匹配标记，不会主动给报文打标。",
      dependencies: [
        "需防火墙、QoS 或其他路径先设置 packet mark；保留原生值/掩码格式。",
      ],
      impact:
        "仅已被其他机制标记的 IPv4 流量会走此规则；错误掩码可能扩大或缩小匹配范围。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 106,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule mark 校验接收 string。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 60,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 mark 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173084,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 mark 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    invert: {
      description:
        "反转此 IPv4 策略规则的匹配结果。源校验与核心参数表均声明布尔值；它反转条件，不反转查表结果。",
      dependencies: ["需一起检查 src、dest、in、out、mark 等条件。"],
      impact:
        "原本不匹配的 IPv4 流量可能转为命中，尤其宽泛条件下会影响更多连接。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 107,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule invert 校验要求 bool。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 55,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 invert 声明为 bool/int8。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173044,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 invert 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    action: {
      description:
        "设置 IPv4 策略规则的特殊动作。基线校验明确列出 prohibit、unreachable、blackhole、throw；面板另保留 unicast，不能据此断定它是固件可直接使用的动作。",
      range: "基线校验：prohibit / unreachable / blackhole / throw。",
      dependencies: ["与 lookup 或 goto 的组合由策略规则实现决定。"],
      summary:
        "源校验支持禁止、不可达、黑洞与继续；普通查表用 lookup，unicast 未验证。",
      discovery:
        "已检查 rule.action 校验与核心字符串表；未在规则校验中找到 unicast，保留面板现有选项但不把它声明为已验证动作。",
      impact:
        "可拒绝、丢弃或继续 IPv4 查表；正常按路由表选路使用 lookup，而不是仅选 unicast。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 110,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule action 校验只显式允许 prohibit、unreachable、blackhole、throw。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 62,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 action 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173100,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 action 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    goto: {
      description:
        "匹配此 IPv4 规则后跳到指定优先级的规则。值是策略规则 priority，不是路由表编号。",
      range: "源校验：0–65535。",
      dependencies: ["需对应目标 priority；与 lookup/action 的组合应核对。"],
      impact:
        "改变 IPv4 规则链的执行位置；无有效跳转目标时不能保证得到预期路径。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 109,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule goto 校验限制 0–65535。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 63,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 goto 声明为 int32。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173108,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 goto 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    suppress_prefixlength: {
      description:
        "查询路由表时，抑制前缀长度不大于此阈值的 IPv4 路由结果。例如阈值 0 可排除该次查询的默认路由，保留更具体路径。",
      unit: "位",
      range: "地址族前缀长度：0–32；面板统一上限 128 不代表 IPv4 都有效。",
      dependencies: ["通常与 lookup 配合；只抑制此次规则的路由结果。"],
      impact:
        "使当前查表忽略较宽的 IPv4 路由；若后续没有替代规则，目标可能失去出口。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 64,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 suppress_prefixlength 声明为 int32。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173116,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 suppress_prefixlength 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
      summary:
        "抑制不长于阈值的 IPv4 路由；有效前缀长度为 0–32，面板上限保留。",
    },
    disabled: {
      description:
        "面板将此项解释为停用 IPv4 策略规则。与 route.disabled 不同，基线规则参数表及 init 校验未列 disabled，不能据此承诺规则会被停用。",
      flags: ["version-dependent"],
      summary: "IPv4 规则表未找到 disabled；此项的停用效果尚未证实。",
      discovery:
        "已检查 netifd 的完整 12 项 rule 参数表、etc/init.d/network 的 rule 校验，以及可读网络 shell/Lua；未找到 rule.disabled 的独立消费，不能套用 route.disabled 的支持。",
      impact:
        "保存可保留原生字段，但此 IPv4 规则是否退出查表链未证实；不要将它当作已验证的停用状态。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 58,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "已检查的规则参数表包含 priority。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173068,
          },
        },
        {
          source: "etc/init.d/network",
          line: 100,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule 的校验声明匹配、查表及动作字段。",
          endLine: 110,
        },
      ],
    },
  },
  rule6: {
    in: {
      description:
        "按入站逻辑接口匹配此 IPv6 策略规则。源校验接收字符串；它筛选报文进入的接口，不是设置出口。",
      dependencies: [
        "需已有 network 逻辑接口；与 out、src/dest 等条件一起匹配。",
      ],
      impact:
        "只让来自该接口的 IPv6 流量命中此规则；名称错误可能使规则不匹配。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 116,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule6 校验声明 in 为 string。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 53,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 in 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173028,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 in 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    out: {
      description:
        "按出站逻辑接口匹配此 IPv6 策略规则，不直接指定转发网卡。路由出口仍由 lookup 或 action 决定。",
      dependencies: ["需已有逻辑接口及对应方向的规则条件。"],
      impact:
        "限制此规则匹配的 IPv6 流量范围；不要用此项替代静态路由的 interface。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 117,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule6 校验声明 out 为 string。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 54,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 out 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173036,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 out 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    src: {
      description:
        "按 IPv6 来源地址或前缀匹配规则。来源条件筛选发送者，与路由条目 source 的源地址选择参数不同。",
      range: "IPv6 CIDR 前缀；前缀长度 0–128。",
      dependencies: ["与 dest、in/out、mark 一起匹配；invert 可反转条件。"],
      impact:
        "决定哪些 IPv6 发送者使用本规则；过宽前缀可能把无关终端引向同一出口或阻断动作。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 118,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule6 src 校验要求 cidr6。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 56,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 src 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173052,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 src 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    dest: {
      description:
        "按 IPv6 目标地址或前缀匹配规则。它选择要处理的目标流量，不会创建该目标的路由。",
      range: "IPv6 CIDR 前缀。",
      dependencies: ["与 lookup 表内的路由、优先级及其他匹配条件配合。"],
      impact:
        "决定访问哪些 IPv6 目的地时触发此查表或动作；对应表仍需有可用路由。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 119,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule6 dest 校验要求 cidr6。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 57,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 dest 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173060,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 dest 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    priority: {
      description:
        "设置此 IPv6 策略规则的处理顺序，数值较小先处理。核心规则表接收整数；此项不同于路由 metric。",
      range: "非负整数；源参数表未声明硬件式上限。",
      dependencies: ["应与其他内核规则协调；goto 使用此优先级作为跳转目标。"],
      impact: "可以让该规则先于其他 IPv6 查表规则生效，改变出口或阻断结果。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 58,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 priority 声明为 int32。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173068,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 priority 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    lookup: {
      description:
        "匹配后查询指定 IPv6 路由表。可填写数字编号或名称；它不会自动把路由加入该表。",
      range: "源校验：数字 0–65535 或路由表名称。",
      dependencies: [
        "配合 route6.table 和该表的有效路由；与特殊 action 的组合需核对。",
      ],
      impact:
        "使匹配的 IPv6 流量查不同的路由表；表内无有效路径时，后续结果取决于其他规则。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 123,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule6 lookup 校验允许 0–65535 或字符串。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 61,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 lookup 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173092,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 lookup 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    mark: {
      description:
        "按数据包标记匹配此 IPv6 规则。使用原生标记及可选掩码；本规则只匹配标记，不会主动给报文打标。",
      dependencies: [
        "需防火墙、QoS 或其他路径先设置 packet mark；保留原生值/掩码格式。",
      ],
      impact:
        "仅已被其他机制标记的 IPv6 流量会走此规则；错误掩码可能扩大或缩小匹配范围。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 121,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule6 mark 校验接收 string。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 60,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 mark 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173084,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 mark 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    invert: {
      description:
        "反转此 IPv6 策略规则的匹配结果。源校验与核心参数表均声明布尔值；它反转条件，不反转查表结果。",
      dependencies: ["需一起检查 src、dest、in、out、mark 等条件。"],
      impact:
        "原本不匹配的 IPv6 流量可能转为命中，尤其宽泛条件下会影响更多连接。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 122,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule6 invert 校验要求 bool。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 55,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 invert 声明为 bool/int8。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173044,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 invert 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    action: {
      description:
        "设置 IPv6 策略规则的特殊动作。基线校验明确列出 prohibit、unreachable、blackhole、throw；面板另保留 unicast，不能据此断定它是固件可直接使用的动作。",
      range: "基线校验：prohibit / unreachable / blackhole / throw。",
      dependencies: ["与 lookup 或 goto 的组合由策略规则实现决定。"],
      summary:
        "源校验支持禁止、不可达、黑洞与继续；普通查表用 lookup，unicast 未验证。",
      discovery:
        "已检查 rule6.action 校验与核心字符串表；未在规则校验中找到 unicast，保留面板现有选项但不把它声明为已验证动作。",
      impact:
        "可拒绝、丢弃或继续 IPv6 查表；正常按路由表选路使用 lookup，而不是仅选 unicast。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 125,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule6 action 校验只显式允许 prohibit、unreachable、blackhole、throw。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 62,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 action 声明为 string。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173100,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 action 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    goto: {
      description:
        "匹配此 IPv6 规则后跳到指定优先级的规则。值是策略规则 priority，不是路由表编号。",
      range: "源校验：0–65535。",
      dependencies: ["需对应目标 priority；与 lookup/action 的组合应核对。"],
      impact:
        "改变 IPv6 规则链的执行位置；无有效跳转目标时不能保证得到预期路径。",
      evidence: [
        {
          source: "etc/init.d/network",
          line: 124,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule6 goto 校验限制 0–65535。",
        },
        {
          source: "docs/field-help-network-research.md",
          line: 63,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 goto 声明为 int32。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173108,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 goto 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    suppress_prefixlength: {
      description:
        "查询路由表时，抑制前缀长度不大于此阈值的 IPv6 路由结果。例如阈值 0 可排除该次查询的默认路由，保留更具体路径。",
      unit: "位",
      range: "地址族前缀长度：0–128。",
      dependencies: ["通常与 lookup 配合；只抑制此次规则的路由结果。"],
      impact:
        "使当前查表忽略较宽的 IPv6 路由；若后续没有替代规则，目标可能失去出口。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 64,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "策略规则参数表将 suppress_prefixlength 声明为 int32。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173116,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 171,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "真实策略规则消费保存 suppress_prefixlength 槽及标志。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 50252,
          },
        },
      ],
    },
    disabled: {
      description:
        "面板将此项解释为停用 IPv6 策略规则。与 route.disabled 不同，基线规则参数表及 init 校验未列 disabled，不能据此承诺规则会被停用。",
      flags: ["version-dependent"],
      summary: "IPv6 规则表未找到 disabled；此项的停用效果尚未证实。",
      discovery:
        "已检查 netifd 的完整 12 项 rule 参数表、etc/init.d/network 的 rule6 校验，以及可读网络 shell/Lua；未找到 rule6.disabled 的独立消费，不能套用 route.disabled 的支持。",
      impact:
        "保存可保留原生字段，但此 IPv6 规则是否退出查表链未证实；不要将它当作已验证的停用状态。",
      evidence: [
        {
          source: "docs/field-help-network-research.md",
          line: 58,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "已检查的规则参数表包含 priority。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 173068,
          },
        },
        {
          source: "etc/init.d/network",
          line: 115,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule6 的校验声明匹配、查表及动作字段。",
          endLine: 125,
        },
      ],
    },
  },
  globals: {
    ula_prefix: {
      description:
        "设置网络的 IPv6 唯一本地地址前缀。核心拆分地址与前缀长度并建立全局前缀；dnsmasq 还用它筛选路由器名称的本地 IPv6 记录。",
      range: "核心前缀处理接受长度1–64；ULA通常取fd00::/8内的/48，非运行缺省。",
      dependencies: [
        "下游地址分配还需 ip6assign；DNS 主机名生成另受 dnsmasq 配置影响。",
      ],
      impact:
        "会影响本地 IPv6 地址规划及路由器名称的本地 DNS 记录；ULA 本身不提供公网路由。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 452,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dnsmasq 读取 network globals 的 ula_prefix。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 460,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "对匹配 ULA 前缀的 LAN IPv6 地址添加路由器主机名记录。",
          endLine: 465,
        },
        {
          source: "docs/field-help-network-research.md",
          line: 174,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心 globals 分支读取 ula_prefix 并交给前缀处理函数。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 67260,
          },
        },
        {
          source: "docs/field-help-network-research.md",
          line: 179,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "核心解析IPv6前缀并要求长度1–64，更新全局前缀对象。",
          artifact: {
            source: "sbin/netifd",
            sha256:
              "7ba2352bde8e6e7f878a68eb83084c3d4416fa5ecd8b2f8af700845d9610151d",
            offset: 44148,
          },
        },
      ],
    },
    packet_steering: {
      description:
        "面板将此项解释为 CPU 间网络处理分流：0 停用、1 启用、2 所有 CPU。基线文本消费者和 netifd 参数表未找到此键；RN02 另有 ECM/NSS 加速路径，不能视为同一开关。",
      flags: ["version-dependent", "hardware-dependent"],
      summary: "CPU 分流选项；1.0.43 未定位此键的消费，不等同于 ECM/NSS 加速。",
      discovery:
        "已搜索 etc/init.d、lib、sbin、usr/sbin、usr/share 的可读消费者和全部可读反编译 Lua，并检查 netifd 参数表；未找到 globals.packet_steering 的输入或应用。",
      impact:
        "是否改变 CPU 分流尚未证实；不能据此判断硬件加速已停用或启用，也不能保证吞吐变化。",
      evidence: [
        {
          source: "lib/miwifi/arch/lib_arch_network.sh",
          line: 10,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "架构初始化操作的是 ecm.global.acceleration_engine，不是 packet_steering。",
          endLine: 20,
        },
        {
          source: "etc/init.d/network",
          line: 34,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "network 启动 netifd。",
        },
      ],
    },
  },
};
