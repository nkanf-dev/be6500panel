import type { ModuleFieldHelp } from "./types";

// Offline Xiaomi RN02 1.0.43 evidence; 1.0.64 firewall consumers were not compared.
export const firewallFieldHelp: ModuleFieldHelp = {
  defaults: {
    input: {
      description:
        "未被更具体规则处理、发往路由器本机的数据包策略；ACCEPT 允许，REJECT 回应拒绝，DROP 静默丢弃。",
      impact:
        "重载 fw3 后改变本机服务的最后处理动作，可能中断管理页面、DNS 或 DHCP 访问。",
      defaultValue: "DROP（fw3 缺失或无效策略回退）",
      range: "ACCEPT / REJECT / DROP",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 25,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 defaults 选项表读取 input，使用动作枚举解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 105692,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 204,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "fw3 缺失或无效策略回退 DROP；LuCI defaults.input 也明确返回 DROP。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    output: {
      description:
        "路由器本机发出的流量在没有更具体允许规则时使用的策略，不是 LAN 客户端的转发策略。",
      impact: "重载后可能阻断路由器自己的 DNS 查询、时间同步和更新连接。",
      defaultValue: "DROP（fw3 缺失或无效策略回退）",
      range: "ACCEPT / REJECT / DROP",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 27,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 defaults 选项表读取 output，使用动作枚举解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 105724,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 204,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "defaults.output 缺失时 DROP；动作作用于 OUTPUT 策略。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    forward: {
      description:
        "经过路由器、但既不发往本机也不由本机生成的流量的全局后备策略。",
      impact:
        "重载后影响客户端跨网络通信；具体区域策略、forwarding 与 rule 仍参与处理。",
      defaultValue: "DROP（fw3 缺失或无效策略回退）",
      range: "ACCEPT / REJECT / DROP",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 26,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 defaults 选项表读取 forward，使用动作枚举解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 105708,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 204,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "fw3 策略校验回退 DROP，LuCI defaults.forward 缺失时也返回 DROP。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    synflood_protect: {
      description:
        "启用 fw3 的 TCP SYN 洪泛限速链。旧名 syn_flood 与本字段写入同一保护开关，不是两层独立防护。",
      impact:
        "重载后改变新建 TCP 连接的 SYN 包处理；正常连接突发也可能触及保护速率。",
      dependencies: [
        "实际限速由同段 synflood_rate、synflood_burst 决定；本目录未列出这两个参数。",
      ],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 32,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 defaults 选项表读取 synflood_protect，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 105804,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 205,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "syn_flood/synflood_protect 共用偏移；生成器使用 --syn 与 syn_flood 链。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    drop_invalid: {
      description:
        "对连接跟踪判定为 INVALID 的包执行丢弃，不是丢弃所有未建立的新连接。",
      impact:
        "重载后过滤异常状态的流量；非对称路由或失配的连接状态可能被丢弃。",
      dependencies: ["需要 conntrack 状态匹配；与 ACCEPT 已建立连接规则并存。"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 28,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 defaults 选项表读取 drop_invalid，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 105740,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 206,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "布尔 drop_invalid 对应 conntrack INVALID 生成分支。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    flow_offloading: {
      description:
        "让已建立的可卸载连接进入 fw3 FLOWOFFLOAD 软件加速分支；解析器支持该字段不等于所有协议都能卸载。",
      impact:
        "重载后可能降低转发 CPU 开销；被加速流量的逐包统计、限速或后续过滤可与普通路径不同。",
      dependencies: [
        "需要固件内核 FLOWOFFLOAD 模块；适用分支匹配 RELATED,ESTABLISHED。",
      ],
      flags: ["hardware-dependent"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 59,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 defaults 选项表读取 flow_offloading，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 106236,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 207,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 生成 FLOWOFFLOAD，并检查卸载模块；此处仅证明静态支持。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    flow_offloading_hw: {
      description:
        "在流量卸载规则上追加硬件卸载请求 --hw；不是小米所有硬件加速功能的总开关。",
      impact:
        "重载后可请求硬件转发；没有驱动或模块支持时，不能仅凭此值保证加速生效。",
      dependencies: ["依赖 flow_offloading 的卸载路径及内核、硬件驱动支持。"],
      flags: ["hardware-dependent"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 60,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 defaults 选项表读取 flow_offloading_hw，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 106252,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 207,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "fw3 硬件分支追加 --hw；未证明 RN02 当前硬件可执行此分支。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    disable_ipv6: {
      description:
        "禁止 fw3 安装 IPv6 规则；vendor IPv6 附加防火墙处理也会检查此值。它不关闭 IPv6 地址或网络服务。",
      impact:
        "重载后 IPv6 防护规则可能缺失，而 IPv6 连接仍能存在；不要当作关闭 IPv6 网络。",
      dependencies: ["vendor ipv6_doit/ipv6_doit_v2 仅在值为 1 时提前返回。"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 58,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 defaults 选项表读取 disable_ipv6，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 106220,
          },
        },
        {
          source: "usr/sbin/sysapi.firewall",
          line: 499,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "读取 firewall.@defaults[0].disable_ipv6；为 1 则停止该 IPv6 附加处理。",
          endLine: 500,
        },
        {
          source: "usr/sbin/sysapi.firewall",
          line: 528,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "新版 IPv6 附加处理同样读取 disable_ipv6 并在 1 时返回。",
          endLine: 529,
        },
      ],
    },
  },
  zone: {
    name: {
      description:
        "区域的引用名称，用于生成 zone_<名称> 链并供规则、转发和地址转换引用；它不是 UCI 段 ID。",
      impact:
        "改名后旧 src/dest 引用需同步更新，否则重载时引用的区域可能找不到。",
      range: "非空，fw3 最长 14 字节",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 176,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 zone 选项表读取 name，使用字符串解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108740,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/zone-details.lua",
          line: 157,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "区域 name 是必填 uciname；改名调用 rename_zone。",
          endLine: 177,
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 249,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 对空区域名及超过 14 字节的名称跳过该区域。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    network: {
      description:
        "把 /etc/config/network 的逻辑接口归入此区域，可为原生列表；不是直接填写物理网卡名。",
      impact:
        "重载与接口地址变更会重新解析区域设备；改变归属会同时改变过滤与 NAT 的适用范围。",
      dependencies: [
        "逻辑接口须能解析为运行设备；与 device/subnet 共同限定区域。",
      ],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 178,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 zone 选项表读取 network，使用区域/设备引用解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108772,
          },
        },
        {
          source: "etc/hotplug.d/iface/20-firewall",
          line: 9,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "接口 ifup/ifupdate 用 fw3 network 判断相关性，再 reload firewall。",
          endLine: 12,
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/zone-details.lua",
          line: 304,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "network 被称为 Covered networks，并逐项 add_network。",
          endLine: 310,
        },
      ],
    },
    device: {
      description:
        "直接按物理/运行设备名称匹配区域；区别于 network 的逻辑接口解析。fw3 将其读为列表。",
      impact:
        "重载后改变按入/出设备选择的区域；设备名称变更会使相关流量不再进入原区域。",
      dependencies: ["使用实际设备名称；可与 network、subnet 一起指定。"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 179,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 zone 选项表读取 device，使用区域/设备引用解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108788,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 208,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "zone.device 是引用列表，规则生成使用 -i/-o 设备模板。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    subnet: {
      description:
        "按地址或网络前缀补充区域匹配，fw3 逐项读取地址列表；不创建接口地址或静态路由。",
      impact: "重载后可缩小或扩大某区域策略覆盖的源/目标地址范围。",
      dependencies: ["地址族须与区域 family 一致；保留原生列表和否定语法。"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 180,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 zone 选项表读取 subnet，使用地址/网络解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108804,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 208,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "zone.subnet 用地址解析器列表，生成地址/掩码匹配。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    input: {
      description: "来自本区域、发往路由器本机的后备策略。",
      impact: "重载后影响区域内客户端访问路由器服务，可能中断管理访问。",
      defaultValue:
        "LuCI 模型缺失时继承 defaults 同名策略，仍缺失则 DROP；不表示当前区域策略",
      range: "ACCEPT / REJECT / DROP",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 181,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 zone 选项表读取 input，使用动作枚举解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108820,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/firewall.lua",
          line: 943,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "zone.input 的 LuCI getter 缺失时取 defaults.input，随后回退 DROP。",
          endLine: 955,
        },
      ],
    },
    output: {
      description:
        "路由器本机发往本区域的后备策略，不是此区域客户端访问外网的策略。",
      impact: "重载后影响路由器主动访问区域内主机及本机服务的出站流量。",
      defaultValue:
        "LuCI 模型缺失时继承 defaults 同名策略，仍缺失则 DROP；不表示当前区域策略",
      range: "ACCEPT / REJECT / DROP",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 183,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 zone 选项表读取 output，使用动作枚举解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108852,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/firewall.lua",
          line: 987,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "zone.output 的 LuCI getter 缺失时取 defaults.output，随后回退 DROP。",
          endLine: 999,
        },
      ],
    },
    forward: {
      description:
        "本区域转发流量的后备策略；区域间 forwarding 许可仍是独立配置。",
      impact: "重载后改变区域内及未被具体跨区域规则许可的转发流量处理。",
      defaultValue:
        "LuCI 模型缺失时继承 defaults 同名策略，仍缺失则 DROP；不表示当前区域策略",
      range: "ACCEPT / REJECT / DROP",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 182,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 zone 选项表读取 forward，使用动作枚举解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108836,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/firewall.lua",
          line: 965,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "zone.forward 的 LuCI getter 缺失时取 defaults.forward，随后回退 DROP。",
          endLine: 977,
        },
      ],
    },
    family: {
      description:
        "限定区域适用的 IPv4、IPv6 或两者；不是给逻辑接口分配地址族。",
      impact:
        "重载后其他地址族的流量不使用此区域规则，双栈接口的保护范围可能改变。",
      dependencies: ["IPv6 规则还受 defaults.disable_ipv6 控制。"],
      range: "any / ipv4 / ipv6",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 177,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 zone 选项表读取 family，使用地址族解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108756,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/zone-details.lua",
          line: 373,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "区域 family 的选项分别是双栈、ipv4 与 ipv6。",
          endLine: 398,
        },
      ],
    },
    masq: {
      description:
        "对离开此区域的 IPv4 包执行 MASQUERADE，以出接口地址作为来源地址。",
      impact:
        "重载后改变客户端外连的可见来源地址；已有连接的 NAT 状态不一定随字段立即更新。",
      dependencies: [
        "IPv4 区域；masq_src/masq_dest 进一步限制范围，无法解析会关闭伪装。",
      ],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 184,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 zone 选项表读取 masq，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108868,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 209,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 包含区域 MASQUERADE 分支；来源/目的地址限制无法解析会 disabling masq。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    masq6: {
      description:
        "通用 schema 中表示 IPv6 地址伪装；1.0.43 的 fw3 区域选项表没有 masq6，不能确认保存后生效。",
      impact:
        "仅凭本字段不能保证 IPv6 NAT；vendor 脚本另按 ipv6 配置生成 NAT6，不等于读取 masq6。",
      flags: ["version-dependent"],
      summary: "通用 IPv6 伪装字段；1.0.43 未找到 zone.masq6 消费。",
      discovery:
        "已搜索 fw3 完整 zone 表与全部字符串、firewall init/hotplug/lib/vendor shell 及既有 Lua 反编译；未找到精确字段 masq6。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 242,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "限定静态搜索范围未发现 masq6；其他 IPv6 NAT 分支不能充当此字段的证据。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
        {
          source: "usr/sbin/sysapi.firewall",
          line: 562,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "vendor 按 IPv6 prefix 创建 NAT6 MASQUERADE；此段没有读取 zone.masq6。",
          endLine: 567,
        },
      ],
    },
    masq_src: {
      description:
        "把区域 IPv4 伪装限定到指定来源网络；列表项可保留否定语法，以排除部分来源。",
      impact:
        "重载后只改变哪些来源会被 NAT，不改变路由或是否允许转发；无法解析的限制会使伪装关闭。",
      dependencies: ["仅在 masq 开启时有意义；适用于 IPv4。"],
      range: "IPv4 地址/网络、可解析引用；支持原生列表与否定",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 186,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 zone 选项表读取 masq_src，使用地址/网络解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108900,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/zone-details.lua",
          line: 405,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "masq_src 明确是来源子网限制，datatype 支持列表/否定。",
          endLine: 422,
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 209,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "未解析的 masq_src 会 disabling masq。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    masq_dest: {
      description:
        "把区域 IPv4 伪装限定到指定目标网络；可用否定项排除不应转换的目的地。",
      impact:
        "重载后改变哪些目的地使用 NAT；不是只允许这些目的地联网的访问控制名单。",
      dependencies: ["仅在 masq 开启时有意义；适用于 IPv4。"],
      range: "IPv4 地址/网络、可解析引用；支持原生列表与否定",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 187,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 zone 选项表读取 masq_dest，使用地址/网络解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108916,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/zone-details.lua",
          line: 428,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "masq_dest 明确是目的子网限制，datatype 支持列表/否定。",
          endLine: 445,
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 209,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "未解析的 masq_dest 会 disabling masq。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    mtu_fix: {
      description:
        "通过 TCPMSS --clamp-mss-to-pmtu 修正 TCP SYN 报文的 MSS，不直接改接口 MTU。",
      impact:
        "重载后可缓解 PPPoE、隧道等路径的 TCP 大包问题；不修复 UDP 或所有路径 MTU 故障。",
      dependencies: ["需要 TCPMSS 支持；作用于 TCP 握手而非所有数据包。"],
      unit: "开关（不是 MTU 字节数）",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 191,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 zone 选项表读取 mtu_fix，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108980,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 210,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂生成 TCPMSS 和 --clamp-mss-to-pmtu；LuCI 标注 MSS clamping。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    log: {
      description:
        "启用区域日志生成。LuCI 将它当作 0/1 开关，但 fw3 实际按数值读取，不是独立布尔选项。",
      impact: "重载后被选中分支的包会写内核日志；噪声多时增加日志与 CPU 开销。",
      dependencies: ["log_limit 限制日志消息，不是流量吞吐限速。"],
      summary: "区域日志开关；原厂 fw3 按数值解析，配合 log_limit 限速。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 193,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 zone 选项表读取 log，使用32 位数值解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 109012,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 211,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "zone.log 使用数值 parser；LuCI 开关写 1，生成分支使用 --log-prefix。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/zone-details.lua",
          line: 460,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "区域日志开关 enabled=1。",
          endLine: 467,
        },
      ],
    },
    log_limit: {
      description:
        "限制该区域日志匹配的平均速率，填写带时间单位的值；不是网速限制。",
      impact:
        "重载后可减少日志洪泛；改变日志数量不自动改变 ACCEPT、REJECT 或 DROP 策略。",
      dependencies: ["区域 log 开启时有意义。"],
      unit: "包/second、minute、hour 或 day",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 194,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 zone 选项表读取 log_limit，使用速率解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 109028,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/zone-details.lua",
          line: 472,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "log_limit 是 Limit log messages，依赖 log=1；10/minute 只是 placeholder。",
          endLine: 483,
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 220,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "速率解析和 --limit/--limit-burst 生成存在；此处不把占位提示当默认。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    enabled: {
      description: "让 fw3 装载或跳过整个区域，而不是删除其网络接口。",
      impact:
        "停用后依赖此区域的规则、转发和 NAT 可能失去有效区域；全局默认策略仍存在。",
      dependencies: ["src/dest 引用此区域的其他段需要重新检查。"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 175,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 zone 选项表读取 enabled，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108724,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 249,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "zone 加载器同段 0x150f4–0x1510c 在 enabled=0 时释放并跳过区域。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
  },
  forwarding: {
    src: {
      description:
        "允许转发的入站区域名；与 dest 构成单向许可，不是逻辑接口名。",
      impact:
        "重载后放行来自此区域、发往 dest 的流量；反方向不会因为本段自动放行。",
      dependencies: ["src 与 dest 都须是有效防火墙区域；enabled 不能为 0。"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 66,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 forwarding 选项表读取 src，使用区域/设备引用解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 106364,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 212,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂生成 src->dest forwarding 及 zone_dest_ACCEPT 跳转。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    dest: {
      description: "此单向转发许可的出站区域名；不是转发后的 IP 地址。",
      impact:
        "重载后改变被放行的出站区域；允许转发不等于自动开启该区域的 NAT。",
      dependencies: ["与 src 配对；区域过滤、路由与 masq 另行配置。"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 67,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 forwarding 选项表读取 dest，使用区域/设备引用解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 106380,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 212,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "forwarding 有独立 dest 引用，跳转对应区域 ACCEPT 链。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    enabled: {
      description:
        "让 fw3 装载或跳过这一条区域间转发许可；不删除 src/dest 区域。",
      impact:
        "停用后此许可不再放行跨区域转发，其他规则或区域默认 ACCEPT 仍可能允许。",
      dependencies: ["仅控制此 forwarding 段，不是全局 forward 策略。"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 63,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 forwarding 选项表读取 enabled，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 106316,
          },
        },
      ],
    },
  },
  rule: {
    name: {
      description:
        "此流量规则的识别名称，用于规则显示与注释；不参与地址、协议或端口匹配。",
      impact: "重载后只改变规则的识别信息，便于定位日志与规则顺序。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 117,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 name，使用字符串解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107764,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 213,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 name 由字符串解析器读取，规则显示使用 Rule/NAT/Redirect 名称格式；不是包匹配条件。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    src: {
      description:
        "匹配进入路由器的来源区域；无 dest 时可针对路由器本机的入站流量。",
      impact:
        "重载后改变规则所在的入站/转发路径，区域填错会使规则匹配范围改变。",
      dependencies: [
        "src/dest 决定入站、出站或转发路径；两者缺失按本机 OUTPUT 规则处理。",
      ],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 119,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 src，使用区域/设备引用解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107796,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 214,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂按 src/dest 区域生成 input/output/forward 规则；两者缺失时按 OUTPUT 处理。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    dest: {
      description: "匹配离开路由器的目标区域；与 src 同时指定时形成转发规则。",
      impact: "重载后改变规则处理的出站区域；仅 dest 通常针对本机出站流量。",
      dependencies: [
        "src/dest 决定入站、出站或转发路径；两者缺失按本机 OUTPUT 规则处理。",
      ],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 120,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 dest，使用区域/设备引用解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107812,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 214,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂按 src/dest 区域生成 input/output/forward 规则；两者缺失时按 OUTPUT 处理。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    src_ip: {
      description: "匹配数据包来源地址或网络前缀，可保留原生列表与否定语法。",
      impact:
        "重载后缩小或扩大适用来源；只匹配地址，不自动绑定 MAC 或 DHCP 租约。",
      dependencies: [
        "地址族与 family 和区域一致；按原生地址、CIDR、列表/否定语法填写。",
      ],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 127,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 src_ip，使用地址/网络解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107924,
          },
        },
      ],
    },
    dest_ip: {
      description:
        "匹配数据包目的地址或网络前缀；这里是过滤条件，不是重定向后的主机地址。",
      impact: "重载后改变哪些目的地受本规则动作处理，不产生地址转换。",
      dependencies: [
        "地址族与 family 和区域一致；按原生地址、CIDR、列表/否定语法填写。",
      ],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 130,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 dest_ip，使用地址/网络解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107972,
          },
        },
      ],
    },
    src_mac: {
      description:
        "匹配收到帧的来源 MAC，可保留原生列表；不能识别经过其他路由器后的远端设备 MAC。",
      impact:
        "重载后按二层来源选择规则；跨三层转发时可见的 MAC 与客户端本身可能不同。",
      dependencies: ["需可见的二层来源 MAC；不是远端设备身份认证。"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 128,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 src_mac，使用MAC 解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107940,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 217,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 MAC 解析和 --mac-source 匹配存在；redirect 的 SNAT 禁止 src_mac。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    src_port: {
      description:
        "匹配 TCP/UDP 等带端口协议的来源端口，可填写端口、范围及原生列表/否定项。",
      impact:
        "重载后改变客户端源端口的匹配范围；客户端临时端口通常不是服务端监听端口。",
      dependencies: ["需 proto 含带端口的协议，常见为 TCP/UDP。"],
      unit: "端口号",
      range: "协议端口 0–65535；可用原生范围/否定语法",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 129,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 src_port，使用端口/范围解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107956,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 216,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂端口解析识别范围与否定，并以 uint16 保存；生成 --sport/--dport。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    dest_port: {
      description:
        "匹配目的服务端口，范围与原生列表/否定语法按 fw3 端口解析器处理。",
      impact: "重载后改变命中的服务；不配置服务本身的监听端口。",
      dependencies: ["需 proto 含带端口的协议，常见为 TCP/UDP。"],
      unit: "端口号",
      range: "协议端口 0–65535；可用原生范围/否定语法",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 131,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 dest_port，使用端口/范围解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107988,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 216,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂端口解析识别范围与否定，并以 uint16 保存；生成 --sport/--dport。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    proto: {
      description: "匹配 IP 协议名称/编号；空值回退 TCP+UDP，不等于所有协议。",
      impact:
        "重载后改变哪些协议受动作处理；ICMP 与 TCP/UDP 端口条件不能互换。",
      defaultValue: "tcp udp（fw3 缺协议回退）",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 126,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 proto，使用协议解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107908,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 215,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 rule/redirect 缺协议回退 TCP+UDP；nat 缺协议回退 all。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    family: {
      description:
        "限定此过滤规则生成 IPv4、IPv6 或两族规则；地址条件必须兼容所选族。",
      impact: "重载后可只改变一个地址族的访问策略，另一族可能仍被允许。",
      dependencies: [
        "IPv6 生成仍受 defaults.disable_ipv6 与内核模块支持约束。",
      ],
      range: "any / ipv4 / ipv6",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 118,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 family，使用地址族解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107780,
          },
        },
      ],
    },
    enabled: {
      description: "让 fw3 装载或跳过这一条流量规则，字段内容仍保留。",
      impact:
        "停用后不再执行此规则动作；后续规则和区域/全局默认策略继续决定结果。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 116,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 enabled，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107748,
          },
        },
      ],
    },
    ipset: {
      description:
        "引用已存在的 IP 集合及原生匹配方向，对集合成员流量应用本规则动作。",
      impact:
        "重载后按集合筛选；集合未知或 ipset 支持关闭时，fw3 会跳过该规则。",
      dependencies: ["须存在有效 ipset，集合地址族与规则一致。"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 123,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 ipset，使用IP 集合引用解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107860,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 218,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂用 --match-set 匹配集合；ipset 支持关闭或集合未知会跳过相关规则。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    mark: {
      description: "按当前包标记及可选掩码匹配；不是给包写入新标记。",
      impact:
        "重载后改变按已有标记选择的流量；可能与策略路由、QoS 标记相互作用。",
      range: "32 位标记[/掩码]；省略掩码为 0xffffffff",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 143,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 mark，使用标记/掩码解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108180,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 219,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂解析标记/掩码并生成 --mark；省略掩码用 0xffffffff。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    limit: {
      description: "限制此规则匹配的平均包速率，须包含时间单位；不是带宽上限。",
      impact: "重载后只有符合限速匹配的包执行 target，超限包继续经过后续规则。",
      dependencies: ["与 limit_burst 配合；超过匹配速率不表示自动丢弃。"],
      unit: "包/second、minute、hour 或 day",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 134,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 limit，使用速率解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108036,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 220,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂速率单位表含 second/minute/hour/day，生成 --limit/--limit-burst。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    limit_burst: {
      description: "设置 limit 匹配允许的初始/突发包数量，不是每秒的持续速率。",
      impact: "重载后改变短时突发命中的包量；无 limit 时不能独立作为吞吐限制。",
      dependencies: ["仅配合本段 limit；不是秒数或字节数。"],
      unit: "包",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 135,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 limit_burst，使用32 位数值解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108052,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 220,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂速率单位表含 second/minute/hour/day，生成 --limit/--limit-burst。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    target: {
      description:
        "决定命中规则后的动作。MARK 还需要 set_mark/set_xmark；HELPER 和 DSCP 分别还需要 set_helper、set_dscp，不能只选择动作。",
      impact:
        "重载后会放行、拒绝、丢弃或改变流量处理；动作缺参数时 fw3 可能跳过规则而非执行。",
      dependencies: [
        "MARK 配合本段 set_mark/set_xmark；HELPER/DSCP 所需写入字段未列在当前目录。",
      ],
      defaultValue: "REJECT（fw3 缺失或无效动作回退）",
      range: "目录列出 ACCEPT / REJECT / DROP / MARK / NOTRACK / HELPER / DSCP",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 148,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 target，使用动作枚举解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108260,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 221,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "fw3 缺失/无效 target 回退 REJECT，MARK 要求写标记参数。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 223,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "helper 与 HELPER 所需 set_helper 是独立字段。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    icmp_type: {
      description:
        "限定 ICMP/ICMPv6 类型或类型/代码，不是 TCP/UDP 端口。原厂类型表包含 echo-request 等命名。",
      impact:
        "重载后只对选定 ICMP 类型执行动作；过窄配置可能影响 IPv6 邻居发现或路径 MTU 通知。",
      dependencies: [
        "proto 必须为相应 ICMP 协议，family 与类型编号/名称一致。",
      ],
      range: "ICMP 类型 0–255；可使用原厂命名或列表",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 132,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 icmp_type，使用ICMP 类型解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108004,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/rule-details.lua",
          line: 630,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "ICMP 类型下拉包含 protocol-unreachable 等类型。",
          endLine: 645,
        },
        {
          source: "usr/sbin/ipv6.sh",
          line: 406,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "vendor 创建 ICMP 规则，并添加 ICMPv6 类型名单、速率和 IPv6 family。",
          endLine: 415,
        },
      ],
    },
    set_mark: {
      description:
        "target=MARK 时写入数据包标记，可带位掩码；与只做匹配的 mark 不同。",
      impact:
        "重载后新标记可影响后续 QoS 或策略路由；掩码不当会覆盖其他功能占用的标记位。",
      dependencies: [
        "target=MARK；不能用否定值；省略掩码会覆盖完整 32 位标记。",
      ],
      range: "32 位值[/掩码]；省略掩码为 0xffffffff",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 144,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 set_mark，使用标记/掩码解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108196,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 222,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂生成 --set-mark；校验不允许否定的 set_mark。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 219,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "mark/mask 解析器省略掩码时写 0xffffffff。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    set_xmark: {
      description:
        "target=MARK 时生成 --set-xmark：先按掩码清位，再异或指定值；不是简单地匹配已有标记。",
      impact:
        "重载后可只修改选定位；会影响使用这些位的 QoS、加速与策略路由规则。",
      dependencies: ["target=MARK；不能用否定值；与 set_mark 是不同写入方式。"],
      range: "32 位值[/掩码]；省略掩码为 0xffffffff",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 145,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 set_xmark，使用标记/掩码解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108212,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 222,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 set_xmark 使用 mark/mask 解析器并生成 --set-xmark。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    helper: {
      description:
        "匹配已有连接跟踪助手名称。它与 HELPER 动作所需的 set_helper 是不同字段；填 helper 不会替代 set_helper。",
      impact:
        "重载后按助手筛选已有连接；助手名称、协议或内核模块不匹配时规则可能跳过。",
      dependencies: [
        "助手应在原厂 fw3_helper 定义中存在，并支持所选协议/地址族；相关模块须加载。",
      ],
      summary: "匹配连接助手；HELPER 动作还需要独立 set_helper。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 124,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 helper，使用连接助手引用解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107876,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 223,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rule.helper 与 set_helper 是独立表项；生成 --helper 做匹配，HELPER 无 set_helper 会报警。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    start_time: {
      description:
        "规定每日开始匹配的时间，接受 HH:MM 或 HH:MM:SS；仅对到达此规则的包做时间条件匹配。",
      impact:
        "重载后开始时刻前的包不命中此规则；需要其他规则承担该时段的允许/拒绝策略。",
      dependencies: [
        "utc_time 决定 UTC 或内核本地时区；weekdays 可再限制日期。",
      ],
      unit: "时:分[:秒]",
      range: "小时 0–23；分钟、秒 0–59",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 139,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 start_time，使用时间解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108116,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 224,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂时间解析器检查小时/分钟/秒范围，并生成 --timestart。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    stop_time: {
      description:
        "规定每日结束匹配的时间，接受 HH:MM 或 HH:MM:SS；不是连接空闲超时。",
      impact:
        "重载后结束时刻外的包不命中此规则；跨午夜及已建立连接的处理需要结合整个规则集。",
      dependencies: ["配合 start_time；utc_time 决定所用时区。"],
      unit: "时:分[:秒]",
      range: "小时 0–23；分钟、秒 0–59",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 140,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 stop_time，使用时间解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108132,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 224,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "同一时间解析器读取 stop_time，并生成 --timestop。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    weekdays: {
      description:
        "按星期限制此规则匹配的日期，原厂枚举 monday 到 sunday，可保留原生列表表达。",
      impact: "重载后其他星期不命中本规则；不是自动启停整个防火墙服务。",
      dependencies: ["与 start_time/stop_time、utc_time 合并作为条件。"],
      unit: "星期",
      range: "monday…sunday（原生列表/缩写兼容性依 fw3 解析）",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 141,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 weekdays，使用星期解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108148,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 225,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂星期枚举表与 --weekdays 生成引用存在。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    utc_time: {
      description:
        "按 UTC 解释规则的时间条件；关闭时 fw3 添加 --kerneltz，使用内核时区，不是浏览器时区。",
      impact:
        "重载后可能使规则的每日生效窗口相对本地时钟移动；不修改系统时钟。",
      dependencies: [
        "需要时间或日期匹配字段；系统时间与内核时区正确才有预期结果。",
      ],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 136,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 rule 选项表读取 utc_time，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108068,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 224,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "utc_time 为假时生成 --kerneltz，时间条件另外生成 --timestart/--timestop。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
  },
  redirect: {
    name: {
      description: "此地址转换规则的识别名称，不决定外部端口或内部主机。",
      impact: "重载后改变 NAT 规则的显示/注释，地址端口匹配仍由其他字段决定。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 88,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 name，使用字符串解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107284,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 213,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 name 由字符串解析器读取，规则显示使用 Rule/NAT/Redirect 名称格式；不是包匹配条件。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    src: {
      description: "DNAT 时表示流量进入的来源区域；不是要转换到的内部区域。",
      impact: "重载后改变接受外部流量的入口；来源区域不匹配时不会执行此转发。",
      dependencies: ["引用防火墙 zone.name，不是 network 逻辑接口。"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 90,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 src，使用区域/设备引用解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107316,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 226,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 redirect.target 缺失回退 DNAT；DNAT/SNAT 的匹配和改写地址字段含义不同。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    dest: {
      description: "DNAT 时表示转换后的内部目标区域；SNAT 时是出站目标区域。",
      impact:
        "重载后改变 NAT 规则使用的区域和允许转发链；不能只填区域而忽略真实路由。",
      dependencies: ["引用防火墙 zone.name，不是 network 逻辑接口。"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 91,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 dest，使用区域/设备引用解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107332,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 226,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 redirect.target 缺失回退 DNAT；DNAT/SNAT 的匹配和改写地址字段含义不同。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    src_ip: {
      description: "匹配转换前的数据包来源地址/网络，不是转换后的来源地址。",
      impact: "重载后限制哪些来源可使用此转发，不能替代转换后的 dest_ip。",
      dependencies: [
        "地址族与 family 和区域一致；原厂此表项为单值，地址/网络前缀与匹配条件的否定按具体转换动作处理。",
      ],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 95,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 src_ip，使用地址/网络解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107396,
          },
        },
      ],
    },
    dest_ip: {
      description: "DNAT 时填写转换后的内部地址；SNAT 时作为原始目的地址匹配。",
      impact:
        "重载后会改变流量实际发送的内部主机（DNAT），或缩小 SNAT 匹配目的地。",
      dependencies: [
        "地址族与 family 和区域一致；原厂此表项为单值，地址/网络前缀与匹配条件的否定按具体转换动作处理。",
      ],
      summary: "DNAT 时为内部转换目标；SNAT 时为原始目的匹配。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 100,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 dest_ip，使用地址/网络解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107476,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/forward-details.lua",
          line: 362,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "DNAT 表单将该字段说明为内部目标地址/端口。",
          endLine: 373,
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 226,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 redirect.target 缺失回退 DNAT；DNAT/SNAT 的匹配和改写地址字段含义不同。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    src_mac: {
      description:
        "限制 DNAT 的二层来源 MAC；原厂 fw3 明确不允许 SNAT 使用此条件。",
      impact:
        "重载后改变可使用 DNAT 的来源；若用于 SNAT，规则校验可能跳过而不是转换。",
      dependencies: ["需可见的二层来源 MAC；不是远端设备身份认证。"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 96,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 src_mac，使用MAC 解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107412,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 217,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 MAC 解析和 --mac-source 匹配存在；redirect 的 SNAT 禁止 src_mac。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    src_port: {
      description: "匹配转换前的来源端口；不是路由器上暴露的外部服务端口。",
      impact: "重载后限制客户端来源端口；外部目的端口由 src_dport 指定。",
      dependencies: [
        "需 proto 含带端口的协议，常见为 TCP/UDP。",
        "原厂此段端口表项为单值；可解析端口范围，未证明原生列表支持。",
      ],
      unit: "端口号",
      range: "协议端口 0–65535；可用原生范围/否定语法",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 97,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 src_port，使用端口/范围解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107428,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 216,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂端口解析识别范围与否定，并以 uint16 保存；生成 --sport/--dport。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
      summary: "原厂此段按单值端口/范围解析，列表支持未证明。",
    },
    dest_port: {
      description: "DNAT 时是内部目标服务端口；SNAT 时是原始目的端口匹配。",
      impact: "重载后改变 DNAT 到内部主机的端口，或限制 SNAT 的目的服务。",
      dependencies: [
        "需 proto 含带端口的协议，常见为 TCP/UDP。",
        "原厂此段端口表项为单值；可解析端口范围，未证明原生列表支持。",
      ],
      unit: "端口号",
      range: "协议端口 0–65535；可用原生范围/否定语法",
      summary: "DNAT 时为内部转换目标；SNAT 时为原始目的匹配。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 101,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 dest_port，使用端口/范围解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107492,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/forward-details.lua",
          line: 398,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "DNAT 表单将该字段说明为内部目标地址/端口。",
          endLine: 413,
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 226,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 redirect.target 缺失回退 DNAT；DNAT/SNAT 的匹配和改写地址字段含义不同。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    proto: {
      description: "选择参与 NAT 的 IP 协议；缺失时 fw3 回退 TCP+UDP。",
      impact:
        "重载后分别改变 TCP/UDP 转发；仅设置外部端口不能匹配未包含的协议。",
      defaultValue: "tcp udp（fw3 缺协议回退）",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 94,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 proto，使用协议解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107380,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 215,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 rule/redirect 缺协议回退 TCP+UDP；nat 缺协议回退 all。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    family: {
      description: "限制此转换规则的地址族，转换前后地址必须使用兼容地址族。",
      impact:
        "重载后决定生成哪一族规则；并不保证 IPv6 NAT 或回环在设备上可用。",
      dependencies: [
        "IPv6 生成仍受 defaults.disable_ipv6 与内核模块支持约束。",
      ],
      range: "any / ipv4 / ipv6",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 89,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 family，使用地址族解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107300,
          },
        },
      ],
    },
    enabled: {
      description:
        "让 fw3 装载或跳过这一条端口转发/地址转换，不删除内部服务器。",
      impact:
        "停用后新流量不再由此规则转换；旧连接的 conntrack NAT 状态可能尚存。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 87,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 enabled，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107268,
          },
        },
      ],
    },
    ipset: {
      description:
        "用已存在的 IP 集合筛选参与此地址转换的流量；保留原生集合方向语法。",
      impact:
        "重载后集合外流量不走此转换；集合未知或支持关闭时该 redirect 会被跳过。",
      dependencies: ["须存在有效 ipset，集合地址族与规则一致。"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 92,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 ipset，使用IP 集合引用解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107348,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 218,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂用 --match-set 匹配集合；ipset 支持关闭或集合未知会跳过相关规则。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    mark: {
      description:
        "按转换前已有的数据包标记及掩码筛选此 NAT 规则，不写新标记。",
      impact:
        "重载后只让指定标记流量使用转发；与 QoS/策略路由的标记分配需一致。",
      range: "32 位标记[/掩码]；省略掩码为 0xffffffff",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 112,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 mark，使用标记/掩码解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107668,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 219,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂解析标记/掩码并生成 --mark；省略掩码用 0xffffffff。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    limit: {
      description:
        "限制能够匹配此地址转换的平均包速率，不是转发服务的带宽限制。",
      impact: "重载后超限包不匹配这条转换，可能转由其他 NAT/过滤规则处理。",
      dependencies: ["与 limit_burst 配合；超过匹配速率不表示自动丢弃。"],
      unit: "包/second、minute、hour 或 day",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 103,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 limit，使用速率解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107524,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 220,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂速率单位表含 second/minute/hour/day，生成 --limit/--limit-burst。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    limit_burst: {
      description: "指定此转换的 limit 匹配允许的突发包数。",
      impact: "重载后影响转发首次或短时突发匹配；持续平均速率仍由 limit 决定。",
      dependencies: ["仅配合本段 limit；不是秒数或字节数。"],
      unit: "包",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 104,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 limit_burst，使用32 位数值解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107540,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 220,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂速率单位表含 second/minute/hour/day，生成 --limit/--limit-burst。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    target: {
      description:
        "DNAT 改写目的地址/端口；SNAT 改写来源地址/端口。同名地址字段的意义随此动作变化。",
      impact:
        "重载后改变流量的实际地址转换方向，错误动作可能转发到错误主机或以错误来源发送。",
      dependencies: [
        "DNAT 的转换目标为 dest_ip/dest_port；SNAT 的改写来源为 src_dip/src_dport。",
      ],
      defaultValue: "DNAT（fw3 缺失或无效动作回退）",
      range: "DNAT / SNAT",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 115,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 target，使用动作枚举解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107716,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 226,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 target 回退 DNAT，init validator 只列 DNAT/SNAT。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
        {
          source: "etc/init.d/firewall",
          line: 31,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "redirect 的 target 校验接受 SNAT 或 DNAT。",
        },
      ],
    },
    src_dip: {
      description:
        "DNAT 时匹配外部目的地址；SNAT 时填写改写后的来源地址。它不是两个动作都通用的外部匹配地址。",
      impact:
        "重载后可限制 DNAT 暴露的外部地址，或改变 SNAT 对外显示的来源地址。",
      dependencies: ["意义由 target 决定；地址族应与 family 一致。"],
      summary: "DNAT 的外部目的匹配；SNAT 的改写来源地址。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 98,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 src_dip，使用地址/网络解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107444,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/forward-details.lua",
          line: 296,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "DNAT 表单定义 External IP address，匹配原始目的 IP。",
          endLine: 301,
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/rule-details.lua",
          line: 389,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "SNAT 旧式 redirect 表单将 src_dip 定义为 SNAT IP address。",
          endLine: 397,
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 226,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "redirect 两种转换方向的字段含义不同。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    src_dport: {
      description:
        "DNAT 时匹配路由器收到的外部目的端口；SNAT 时指定改写后的来源端口。",
      impact:
        "重载后改变公开服务的入口端口（DNAT），或改变出站来源端口（SNAT）。",
      dependencies: [
        "target 决定含义；proto 应包含 TCP/UDP 等带端口协议。",
        "原厂此表项为单值，不是规则段的端口列表。",
      ],
      unit: "端口号",
      range: "协议端口 0–65535；支持原生范围",
      summary: "DNAT 的外部目的端口；SNAT 的改写来源端口。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 99,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 src_dport，使用端口/范围解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107460,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/forward-details.lua",
          line: 335,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "DNAT 表单将 src_dport 描述为外部目的端口/范围。",
          endLine: 346,
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/rule-details.lua",
          line: 441,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "SNAT 表单将 src_dport 作为改写后来源端口，空值不改端口。",
          endLine: 453,
        },
      ],
    },
    reflection: {
      description:
        "为适用的 DNAT 生成 NAT 回环，让内网客户端可经外部地址访问内部转发服务。",
      impact:
        "重载后可能增加内部回环的 DNAT/SNAT 规则；关闭后经外部地址访问服务可能失败，直接内网访问不受此开关控制。",
      dependencies: [
        "依赖有效 DNAT、来源区域地址与内部目标；reflection_src 选择回环来源地址。",
      ],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 113,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 reflection，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107684,
          },
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/luci/model/cbi/firewall/forward-details.lua",
          line: 417,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "reflection 标为 Enable NAT Loopback；cfgvalue 缺失时返回 1，仅这是 LuCI 显示回退。",
          endLine: 441,
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 227,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂反射开关与 internal/external 来源枚举存在。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    reflection_src: {
      description:
        "选择回环 SNAT 使用内部区域地址或外部区域地址；不是客户端原始来源地址过滤。",
      impact:
        "重载后改变内部服务看到的回环来源地址，可能影响服务器基于来源地址的访问控制。",
      dependencies: [
        "reflection 启用且 DNAT 回环能生成；相关区域必须有可用地址。",
      ],
      range: "internal / external",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 114,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 reflection_src，使用回环来源枚举解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107700,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 227,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 reflection_src enum 只有 internal 与 external。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    reflection_zone: {
      description:
        "通用 schema 中用于选择 NAT 回环区域的列表；1.0.43 的 redirect 选项表没有此名称。",
      impact:
        "不能保证按此列表选择回环区域；原厂已确认的开关是 reflection 和 reflection_src。",
      dependencies: [
        "原厂已解析 reflection/reflection_src；本字段支持未证实。",
      ],
      flags: ["version-dependent"],
      summary: "通用回环区域字段；1.0.43 未找到 reflection_zone 消费。",
      discovery:
        "已搜索完整 fw3 redirect 表、全部 fw3 NUL 字符串、firewall init/hotplug/lib/vendor shell 及全部既有 Lua 反编译；未找到精确字段 reflection_zone。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 243,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 redirect 完整选项表与限定搜索中未找到 reflection_zone。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 114,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 redirect 选项表读取 reflection_src，使用回环来源枚举解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107700,
          },
        },
      ],
    },
  },
  nat: {
    name: {
      description: "此源 NAT 规则的识别名称，用于显示与注释，不是来源地址。",
      impact: "重载后改变规则的识别信息，不直接改变 SNAT/MASQUERADE 匹配。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 150,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 nat 选项表读取 name，使用字符串解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108308,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 213,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 name 由字符串解析器读取，规则显示使用 Rule/NAT/Redirect 名称格式；不是包匹配条件。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    src: {
      description:
        "指定源 NAT 挂载的出站区域；原厂 nat 生成 zone_<区域>_postrouting 链，与 rule.src 的入站含义不同。",
      impact:
        "重载后改变哪些出接口流量使用此源 NAT；填错区域会转换错误出口或不命中。",
      dependencies: ["引用防火墙 zone.name，不是 network 逻辑接口。"],
      summary: "源 NAT 的出站区域；与流量规则 src 的入站含义不同。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 152,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 nat 选项表读取 src，使用区域/设备引用解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108340,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 228,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 nat.src 对应出站区域的 zone_<区域>_postrouting 链；不是 rule.src 的入站区域。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    dest: {
      description:
        "通用 schema 的目标区域字段；1.0.43 fw3 的 nat 表没有 dest，只找到 dest_ip/dest_port 匹配。",
      impact: "不能确认 nat.dest 会限制出口；原厂源 NAT 出站区域由 src 指定。",
      dependencies: ["引用防火墙 zone.name，不是 network 逻辑接口。"],
      flags: ["version-dependent"],
      summary: "通用字段；1.0.43 未找到 nat.dest 消费。",
      discovery:
        "搜索 fw3 完整 nat 选项表、firewall init/hotplug/lib/vendor shell 及全部既有 Lua 反编译，未发现 nat.dest 读取；其他段同名字段不等于本段支持。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 246,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "完整 nat 表中没有 dest，范围内 shell/Lua 消费搜索也未找到该段字段。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 152,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 nat 选项表读取 src，使用区域/设备引用解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108340,
          },
        },
      ],
    },
    src_ip: {
      description:
        "匹配源 NAT 转换前的来源地址或网络；转换后的地址另由 snat_ip 指定。",
      impact: "重载后改变哪些内部来源被 SNAT/伪装，不分配新的接口地址。",
      dependencies: [
        "地址族与 family 和区域一致；原厂此表项为单值，地址/网络前缀与匹配条件的否定按具体转换动作处理。",
      ],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 156,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 nat 选项表读取 src_ip，使用地址/网络解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108404,
          },
        },
      ],
    },
    dest_ip: {
      description: "匹配源 NAT 流量的目的地址/网络；不是改写后的地址。",
      impact: "重载后可按目的网络筛选源 NAT，不会把流量重定向到该地址。",
      dependencies: [
        "地址族与 family 和区域一致；原厂此表项为单值，地址/网络前缀与匹配条件的否定按具体转换动作处理。",
      ],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 160,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 nat 选项表读取 dest_ip，使用地址/网络解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108468,
          },
        },
      ],
    },
    src_mac: {
      description:
        "通用 schema 的来源 MAC 条件；原厂 1.0.43 nat 选项表没有 src_mac。",
      impact:
        "无法保证源 NAT 按 MAC 筛选；不要把其他 rule/redirect 的 MAC 支持视为 nat 支持。",
      dependencies: ["需可见的二层来源 MAC；不是远端设备身份认证。"],
      flags: ["version-dependent"],
      summary: "通用字段；1.0.43 未找到 nat.src_mac 消费。",
      discovery:
        "搜索 fw3 完整 nat 选项表、firewall init/hotplug/lib/vendor shell 及全部既有 Lua 反编译，未发现 nat.src_mac 读取；其他段同名字段不等于本段支持。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 247,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "完整 nat 表中没有 src_mac，范围内 shell/Lua 消费搜索也未找到该段字段。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 156,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 nat 选项表读取 src_ip，使用地址/网络解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108404,
          },
        },
      ],
    },
    src_port: {
      description:
        "匹配源 NAT 转换前的来源端口；转换后的端口由 snat_port 指定。",
      impact: "重载后缩小原始客户端端口的转换范围，不改变服务器目的端口。",
      dependencies: [
        "需 proto 含带端口的协议，常见为 TCP/UDP。",
        "原厂此段端口表项为单值；可解析端口范围，未证明原生列表支持。",
      ],
      unit: "端口号",
      range: "协议端口 0–65535；可用原生范围/否定语法",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 157,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 nat 选项表读取 src_port，使用端口/范围解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108420,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 216,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂端口解析识别范围与否定，并以 uint16 保存；生成 --sport/--dport。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
      summary: "原厂此段按单值端口/范围解析，列表支持未证明。",
    },
    dest_port: {
      description: "按原始目的服务端口筛选源 NAT；它不表示转换后的来源端口。",
      impact: "重载后只改变哪些目的服务流量使用本规则；服务监听端口不变。",
      dependencies: [
        "需 proto 含带端口的协议，常见为 TCP/UDP。",
        "原厂此段端口表项为单值；可解析端口范围，未证明原生列表支持。",
      ],
      unit: "端口号",
      range: "协议端口 0–65535；可用原生范围/否定语法",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 161,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 nat 选项表读取 dest_port，使用端口/范围解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108484,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 216,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂端口解析识别范围与否定，并以 uint16 保存；生成 --sport/--dport。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
      summary: "原厂此段按单值端口/范围解析，列表支持未证明。",
    },
    proto: {
      description:
        "选择参与源 NAT 的 IP 协议；原厂 nat 缺失时回退 all，区别于 rule/redirect 的 TCP+UDP。",
      impact: "重载后改变源 NAT 协议范围；端口条件还需要相应带端口协议。",
      defaultValue: "all（fw3 缺协议回退）",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 155,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 nat 选项表读取 proto，使用协议解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108388,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 215,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 rule/redirect 缺协议回退 TCP+UDP；nat 缺协议回退 all。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    family: {
      description:
        "限定此源 NAT 的 IPv4/IPv6 地址族；来源和 snat_ip 必须与所选族兼容。",
      impact:
        "重载后按地址族生成源 NAT；二进制解析支持不等于每族内核 NAT 可用。",
      dependencies: [
        "IPv6 生成仍受 defaults.disable_ipv6 与内核模块支持约束。",
      ],
      range: "any / ipv4 / ipv6",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 151,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 nat 选项表读取 family，使用地址族解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108324,
          },
        },
      ],
    },
    enabled: {
      description: "让 fw3 装载或跳过此源 NAT 段，不停用整个区域的 masq。",
      impact:
        "停用后流量可能回到区域伪装或其他源 NAT；旧连接 NAT 状态可能继续存在。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 149,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 nat 选项表读取 enabled，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108292,
          },
        },
      ],
    },
    ipset: {
      description: "引用已有集合，按集合成员及匹配方向筛选源 NAT。",
      impact: "重载后集合未知、族不一致或 ipset 支持关闭会使本规则被跳过。",
      dependencies: ["须存在有效 ipset，集合地址族与规则一致。"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 154,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 nat 选项表读取 ipset，使用IP 集合引用解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108372,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 218,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂用 --match-set 匹配集合；ipset 支持关闭或集合未知会跳过相关规则。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    mark: {
      description:
        "按已有的包标记和掩码筛选源 NAT；不会设置来源 IP 或写新标记。",
      impact: "重载后可把不同标记流量分配到不同源 NAT；需与上游标记规则配合。",
      range: "32 位标记[/掩码]；省略掩码为 0xffffffff",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 173,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 nat 选项表读取 mark，使用标记/掩码解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108676,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 219,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂解析标记/掩码并生成 --mark；省略掩码用 0xffffffff。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    limit: {
      description: "限制本源 NAT 规则的平均匹配包速率，不是出口带宽。",
      impact: "重载后超限包不匹配此源 NAT，可能由其他规则继续处理。",
      dependencies: ["与 limit_burst 配合；超过匹配速率不表示自动丢弃。"],
      unit: "包/second、minute、hour 或 day",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 163,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 nat 选项表读取 limit，使用速率解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108516,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 220,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂速率单位表含 second/minute/hour/day，生成 --limit/--limit-burst。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    limit_burst: {
      description: "设置源 NAT 的 limit 匹配允许的突发包数。",
      impact: "重载后改变短时突发命中的流量，不单独控制平均速度。",
      dependencies: ["仅配合本段 limit；不是秒数或字节数。"],
      unit: "包",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 164,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 nat 选项表读取 limit_burst，使用32 位数值解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108532,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 220,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂速率单位表含 second/minute/hour/day，生成 --limit/--limit-burst。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    target: {
      description:
        "SNAT 用指定 snat_ip/snat_port 改写来源；MASQUERADE 使用动态出口地址。它不是允许/拒绝动作。",
      impact:
        "重载后改变出站流量对外显示的来源；错误来源地址或端口可使回复无法返回。",
      dependencies: [
        "SNAT 至少提供 snat_ip 或 snat_port；MASQUERADE 不应带这两个改写值。",
      ],
      defaultValue: "MASQUERADE（fw3 缺失或无效动作回退）",
      range: "SNAT / MASQUERADE",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 174,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 nat 选项表读取 target，使用动作枚举解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108692,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 229,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂缺/无效 target 回退 MASQUERADE；SNAT 必须有来源 IP 或端口。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    snat_ip: {
      description:
        "target=SNAT 时指定改写后的来源 IP；原始来源匹配使用 src_ip。它不把此地址配置到接口上。",
      impact:
        "重载后服务端将看到此来源地址；需要真实可路由的回程，否则连接无法成功。",
      dependencies: [
        "target=SNAT；地址族须匹配 family；非 SNAT 使用 snat_ip 会被原厂校验拒绝。",
      ],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 158,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 nat 选项表读取 snat_ip，使用地址/网络解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108436,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 229,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "SNAT 可提供 snat_ip；非 SNAT 的 snat_ip 触发 must not use 报错。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    snat_port: {
      description:
        "target=SNAT 时改写来源端口或范围；不是原始来源/目的端口匹配。",
      impact:
        "重载后改变服务端看到的客户端端口，可能影响固定端口协议及并发映射。",
      dependencies: [
        "target=SNAT；proto 应含带端口协议；非 SNAT 使用该值会被校验拒绝。",
        "原厂此表项为单值，不是规则段的端口列表。",
      ],
      unit: "端口号",
      range: "协议端口 0–65535；支持原生范围",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 159,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 nat 选项表读取 snat_port，使用端口/范围解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 108452,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 229,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "SNAT 可提供 snat_port；非 SNAT 的 snat_port 触发 must not use 报错。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 216,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "snat_port 使用原厂端口/范围解析器，端点保存为 uint16。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
  },
  include: {
    path: {
      description:
        "指定已有附加文件的路径。fw3 可装载规则文件，或把脚本作为 shell 文件执行；vendor loader 可继续插入小米规则。",
      impact:
        "防火墙启动/重载后附加文件可改动实际规则与相关服务；改错路径会跳过，改错脚本可中断附加处理。",
      dependencies: [
        "配合 type；目标文件需要存在。原厂脚本包装不允许通过 config() 读取 UCI。",
      ],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 69,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 include 选项表读取 path，使用字符串解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 106948,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 231,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂脚本路径检查及 shell 包装存在，vendor loader 另调用 sysapi.firewall。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
        {
          source: "lib/firewall.sysapi.loader",
          line: 7,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "附加 loader 在 sysapi.firewall 可执行时调用它。",
          endLine: 9,
        },
      ],
    },
    type: {
      description:
        "选择已有附加文件格式。1.0.43 fw3 的实际枚举是 script/restore；目录中 nftables 属于通用 fw4 语义，未找到原厂支持。",
      impact:
        "重载时决定脚本执行或 iptables-restore 装载路径；nftables 不能当成此基线已支持的格式。",
      range: "原厂 fw3：script / restore；目录 nftables 为版本相关项",
      flags: ["version-dependent"],
      summary: "1.0.43 原厂类型为 script/restore；nftables 未验证。",
      discovery:
        "fw3 include.type 解析枚举表只含 script/restore；搜索同二进制及 firewall init/lib/vendor/Lua 消费者未找到 nftables。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 70,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 include 选项表读取 type，使用include 类型枚举解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 106964,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 230,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "include 类型表只有 script/restore，无 nftables。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 245,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "限定搜索未找到 nftables 消费；不否认其他固件可能支持。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    enabled: {
      description:
        "让 fw3 装载或跳过这一段附加文件，不能等同于附加脚本的内部服务开关。",
      impact:
        "停用后该段不再插入自定义规则；其他 include 或 vendor 服务仍可能插入相同功能规则。",
      dependencies: ["只作用于本 include 段；不自动删除其他服务持有的规则。"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 68,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 include 选项表读取 enabled，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 106932,
          },
        },
      ],
    },
    reload: {
      description:
        "指定防火墙 reload 时重新装载此附加配置；原厂静态装载分支会检查该开关。",
      impact:
        "开启可在重载后重新应用附加规则；脚本或规则文件需要能安全重复执行，字段本身不保证幂等。",
      dependencies: [
        "enabled 启用且 path/type 有效；不同附加类型的执行分支不同。",
      ],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 72,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 include 选项表读取 reload，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 106996,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 232,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "include.reload 保存于偏移 0x1c；reload 装载 restore 分支值为 0 时跳过。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    fw4_compatible: {
      description:
        "通用 fw4 兼容声明；1.0.43 使用 fw3，原厂 include 选项表没有此字段。",
      impact: "此声明不会把 fw3 变为 fw4，也不能使 nftables 附加文件自动生效。",
      flags: ["version-dependent"],
      summary: "通用 fw4 声明；1.0.43 fw3 未找到字段消费。",
      discovery:
        "搜索完整 fw3 include 表与全部 NUL 字符串、firewall init/hotplug/lib/vendor shell 及全部既有 Lua 反编译，未找到精确字段 fw4_compatible。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 244,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 include 完整表与限定消费者搜索未发现 fw4_compatible。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
        {
          source: "etc/init.d/firewall",
          line: 75,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 reload_service 调用 fw3 reload，而不是 fw4。",
          endLine: 77,
        },
      ],
    },
  },
  ipset: {
    name: {
      description:
        "集合的引用名称，由规则 ipset 字段指向；不是规则名称或 DNS 域名。",
      impact:
        "重载后以此名创建集合；改名后规则的引用需同步，否则相关规则可能跳过。",
      dependencies: ["必须非空，且 ipset 工具/内核类型支持可用。"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 74,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 ipset 选项表读取 name，使用字符串解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107044,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 236,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂按名称创建 ipset，并以 add 逐项添加成员。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 233,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂检查 IP 集合必须使用单一地址族，不能为 any。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    family: {
      description:
        "指定该集合的 IPv4 或 IPv6 类型；原厂明确不接受 family=any，不是双栈集合。",
      impact: "重载后创建单族集合；引用此集合的规则地址族不同会被跳过。",
      dependencies: ["集合成员与引用规则须使用同一地址族。"],
      range: "ipv4 / ipv6；原厂不接受 any",
      summary: "集合仅可选 IPv4 或 IPv6；原厂不接受 any 双栈。",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 75,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 ipset 选项表读取 family，使用地址族解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107060,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 233,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂校验报错 must not have family any。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    match: {
      description:
        "定义集合键的类型和方向，例如 src_net；由 src_/dst_/dest_ 加 ip、net、port、mac 或 set 等类型组合。",
      impact:
        "重载后决定成员如何解释与规则如何匹配；类型改变后旧 entry 可能不再合法。",
      dependencies: [
        "配合 storage 与 entry；不能只有集合名而没有 match 类型。",
      ],
      range: "1–3 个匹配类型；组合须受 storage 与 ipset 支持",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 77,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 ipset 选项表读取 match，使用集合匹配类型解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107092,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 234,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂识别方向前缀与类型表，并校验最多三个数据类型。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    storage: {
      description:
        "选择 bitmap、hash 或 list 集合存储方法；必须与 match 类型组合兼容。",
      impact:
        "重载后改变集合创建方式与内存/容量特征；无效组合会使集合无法建立。",
      dependencies: [
        "bitmap 可能还需 iprange/portrange，当前目录未列这些范围参数；缺值时 fw3 按 match 推定方法而非固定默认。",
      ],
      range: "bitmap / hash / list；实际支持组合依 match",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 76,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 ipset 选项表读取 storage，使用集合存储枚举解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107076,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 235,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂存储枚举为 bitmap/hash/list；缺失方法按 matches 推定。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    entry: {
      description:
        "集合的静态成员列表，内容必须符合 match 定义的地址、网段、端口或组合；每个原生列表项保持独立。",
      impact:
        "重载后成员逐项添加到集合，直接改变引用该集合的流量匹配；不创建防火墙允许动作。",
      dependencies: [
        "成员格式与 family、match、storage 一致；规则须另引用本集合。",
      ],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 85,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 ipset 选项表读取 entry，使用集合成员解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107220,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 236,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 entry 列表使用 add %s %s 生成成员添加。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    maxelem: {
      description:
        "控制支持此选项的集合最多容纳的成员数；原厂会为不适用的存储/类型组合忽略它。",
      impact:
        "重载后影响集合容量和内存；容量不足时后续成员可能添加失败，不是吞吐限速。",
      dependencies: [
        "需 storage/type 组合支持 maxelem；目录输入下限 1，不代表已证明原厂固定最大值。",
      ],
      unit: "成员",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 81,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 ipset 选项表读取 maxelem，使用32 位数值解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107156,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 237,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂创建 maxelem %u，部分组合明确 maxelem ignored。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    timeout: {
      description:
        "指定支持超时的集合成员生存期；按 ipset 协议为秒，0 表示不自动超时。不是 TCP 连接超时。",
      impact:
        "重载后集合成员可按超时消失，使引用规则不再命中；不会自动中断所有现有连接。",
      dependencies: [
        "所选 ipset 类型须支持 timeout；成员被重新加入时超时可重新计算。",
      ],
      unit: "秒",
      range: "非负秒数；0 不自动超时（ipset 协议语义）",
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 83,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 ipset 选项表读取 timeout，使用32 位数值解析器；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107188,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 238,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂向 ipset create 输出 timeout %u；秒/0 的解释是 ipset 协议语义，不是固件设置默认。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
    enabled: {
      description: "让 fw3 创建/装载或跳过此集合，成员配置仍保留。",
      impact:
        "停用后引用此集合的 rule/redirect/nat 可能因集合缺失而跳过；不是单独停用那些规则段。",
      dependencies: ["引用它的规则需与集合启用状态配合。"],
      evidence: [
        {
          source: "docs/field-help-firewall-research.md",
          line: 73,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 fw3 的 ipset 选项表读取 enabled，使用布尔解析器（true / yes / 1 为真）；表项不是当前配置值。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
            offset: 107028,
          },
        },
        {
          source: "docs/field-help-firewall-research.md",
          line: 218,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 rule/redirect/nat 遇未知集合会跳过。",
          artifact: {
            source: "sbin/fw3",
            sha256:
              "7e25ca821847e00bf042b20aacd81be61c6b0eed239f37ad5801e43d3bc8101a",
          },
        },
      ],
    },
  },
};
