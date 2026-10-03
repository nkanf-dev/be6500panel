import type { ModuleFieldHelp } from "./types";

// Field-specific source tracing; unknown vendor consumers are recorded in discovery.
export const dropbearFieldHelp: ModuleFieldHelp = {
  dropbear: {
    Port: {
      description:
        "设置原厂 Dropbear SSH 的 TCP 监听端口，源码回退为 22。原厂支持端口列表，并为绑定接口的每个地址生成 -p 参数；独立救援通道使用自己的启动配置。",
      defaultValue: "22（原厂 init 校验回退）",
      unit: "TCP 端口",
      range: "1–65535（port 类型）；原厂校验允许列表",
      dependencies: [
        "Interface 非空时仅绑定其已获取的 IPv4/IPv6 地址。",
        "原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。",
        "be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。",
      ],
      flags: ["version-dependent"],
      impact:
        "重建原厂实例后，新连接改用相应端口；防火墙和客户端需匹配。此原厂实例配置不证明独立救援入口跟随改端口，应分别核对。",
      evidence: [
        {
          source: "etc/init.d/dropbear",
          line: 41,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "Port 以 list(port) 校验，回退 22。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 15,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "无绑定地址时生成 -p port，有地址时逐项生成 -p addr:port。",
          endLine: 27,
        },
        {
          source: "etc/init.d/dropbear",
          line: 82,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "实例把解析出的地址和 Port 传给 append_ports。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 132,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。",
          endLine: 137,
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/dropbear",
          line: 82,
          endLine: 82,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "1.0.64 原厂实例同样把 Port 和接口地址传给 append_ports。",
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 26,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "Dropbear 服务端帮助给出 -p [address:]port。",
          artifact: {
            source: "usr/sbin/dropbear",
            sha256:
              "f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2",
            offset: 153239,
          },
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 27,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "Dropbear 服务端帮助说明 -p 监听指定 TCP 端口及可选地址。",
          artifact: {
            source: "usr/sbin/dropbear",
            sha256:
              "f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2",
            offset: 153257,
          },
        },
      ],
    },
    Interface: {
      description:
        "监听的 network 逻辑接口名。原厂解析该接口的全部 IPv4/IPv6 地址；空值则不传监听地址。",
      dependencies: [
        "填写逻辑接口而不是 eth0 等底层设备名；接口需要已有地址。",
        "原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。",
        "be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。",
      ],
      flags: ["version-dependent"],
      impact:
        "修改会改变原厂实例的监听范围。指定接口但无可用地址时实例不启动；开机阶段先跳过有接口绑定的实例，随后由接口事件触发加载。独立救援绑定由其自己的启动配置决定。",
      evidence: [
        {
          source: "etc/init.d/dropbear",
          line: 35,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "Interface 校验为字符串，没有配置缺省接口。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 61,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "Interface 非空时 BOOT 阶段跳过，否则查询接口所有地址，查询失败则返回。",
          endLine: 68,
        },
        {
          source: "lib/functions/network.sh",
          line: 131,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "network_get_ipaddrs_all 合并 IPv4 与 IPv6 地址，均无地址时失败。",
          endLine: 143,
        },
        {
          source: "etc/init.d/dropbear",
          line: 152,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "配置变化触发 reload；已启用实例的 Interface 添加 interface.* 重载触发器。",
          endLine: 160,
        },
        {
          source: "etc/init.d/dropbear",
          line: 132,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。",
          endLine: 137,
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/dropbear",
          line: 61,
          endLine: 68,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "1.0.64 原厂实例同样解析 Interface 地址，启动阶段跳过绑定实例，无地址时返回失败。",
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 27,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "Dropbear 的 -p 参数支持指定监听地址，与 init 的 Interface 地址解析配合。",
          artifact: {
            source: "usr/sbin/dropbear",
            sha256:
              "f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2",
            offset: 153257,
          },
        },
      ],
    },
    PasswordAuth: {
      description:
        "允许整个实例的密码认证；为 0 时原厂给 Dropbear 加 -s。它不保存或修改用户密码。",
      defaultValue: "1（原厂 init 校验回退）",
      dependencies: [
        "root 密码登录还取决于 RootPasswordAuth、RootLogin 及有效账户凭据。",
        "原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。",
        "be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。",
      ],
      flags: ["version-dependent"],
      impact:
        "关闭后新登录不能使用密码认证，公钥认证不因此关闭。root 密码还需 RootPasswordAuth 和 RootLogin 允许；独立救援的认证策略不能由原厂 gate 推断。",
      evidence: [
        {
          source: "etc/init.d/dropbear",
          line: 33,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "PasswordAuth 布尔校验回退 1。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 76,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "PasswordAuth=0 时向命令追加 -s。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 78,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "root 密码和 root 全部登录限制分别映射 -g/-w。",
          endLine: 79,
        },
        {
          source: "etc/init.d/dropbear",
          line: 132,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。",
          endLine: 137,
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/dropbear",
          line: 76,
          endLine: 76,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "1.0.64 原厂实例同样在 PasswordAuth=0 时追加 -s。",
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 28,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 ELF 的 -s 帮助明确禁用密码登录。",
          artifact: {
            source: "usr/sbin/dropbear",
            sha256:
              "f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2",
            offset: 152962,
          },
        },
      ],
    },
    RootPasswordAuth: {
      description:
        "允许或禁止 root 使用密码登录 SSH。原厂在值为 0 时追加 -g；它不自行禁用 root 公钥认证，root 登录还受其他认证开关控制。",
      defaultValue: "1（原厂 init 校验回退）",
      dependencies: [
        "root 密码需要 PasswordAuth=1、RootLogin=1 和有效 root 密码。",
        "原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。",
        "be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。",
      ],
      flags: ["version-dependent"],
      impact:
        "关闭后 root 密码登录被拒绝。普通用户是否可用密码仍由 PasswordAuth 决定；RootLogin=0 会进一步拒绝 root 的全部认证方式。",
      evidence: [
        {
          source: "etc/init.d/dropbear",
          line: 37,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "RootPasswordAuth 布尔校验回退 1。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 76,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "PasswordAuth=0 加 -s，RootPasswordAuth=0 加 -g，RootLogin=0 加 -w。",
          endLine: 79,
        },
        {
          source: "etc/init.d/dropbear",
          line: 132,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。",
          endLine: 137,
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/dropbear",
          line: 78,
          endLine: 78,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "1.0.64 原厂实例同样在 RootPasswordAuth=0 时追加 -g。",
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 29,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 ELF 的 -g 帮助明确禁用 root 的密码登录。",
          artifact: {
            source: "usr/sbin/dropbear",
            sha256:
              "f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2",
            offset: 152990,
          },
        },
      ],
    },
    RootLogin: {
      description:
        "允许 root 通过此实例登录；为 0 时追加 -w，限制不只是密码认证。",
      defaultValue: "1（原厂 init 校验回退）",
      dependencies: [
        "若允许 root，认证方式还由 PasswordAuth、RootPasswordAuth 和账户公钥配置决定。",
        "原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。",
        "be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。",
      ],
      flags: ["version-dependent"],
      impact:
        "关闭后原厂实例拒绝 root 登录，包括公钥。它不删除 root 账户或已有密钥，也不代表独立救援实例按相同策略运行。",
      evidence: [
        {
          source: "etc/init.d/dropbear",
          line: 38,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "RootLogin 布尔校验回退 1。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 78,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "RootPasswordAuth=0 对应 -g；RootLogin=0 对应 -w，两者是独立参数。",
          endLine: 79,
        },
        {
          source: "etc/init.d/dropbear",
          line: 132,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。",
          endLine: 137,
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/dropbear",
          line: 79,
          endLine: 79,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "1.0.64 原厂实例同样在 RootLogin=0 时追加 -w。",
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 30,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 ELF 的 -w 帮助明确禁止 root 登录。",
          artifact: {
            source: "usr/sbin/dropbear",
            sha256:
              "f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2",
            offset: 152937,
          },
        },
      ],
    },
    GatewayPorts: {
      description:
        "允许 SSH 远程转发的监听接受非本机来源连接；原厂值为 1 时加 -a。这不是路由转发或网关的总开关。",
      defaultValue: "0（原厂 init 校验回退）",
      dependencies: [
        "需要 SSH 客户端发起远程端口转发；防火墙仍决定可达性。",
        "原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。",
        "be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。",
      ],
      flags: ["version-dependent"],
      impact:
        "可把远程转发端口暴露到其他可达设备，仍受客户端请求和防火墙约束。不改变 Dropbear 自身 Port/Interface，也不等于启用 WAN SSH。",
      evidence: [
        {
          source: "etc/init.d/dropbear",
          line: 36,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "GatewayPorts 布尔校验回退 0。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 77,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "GatewayPorts=1 时给 Dropbear 追加 -a。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 82,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "SSH 服务自身监听由 append_ports 单独设置。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 132,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。",
          endLine: 137,
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/dropbear",
          line: 77,
          endLine: 77,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "1.0.64 原厂实例同样在 GatewayPorts=1 时追加 -a。",
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 31,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 ELF 的 -a 帮助说明转发端口允许来自任意主机的连接。",
          artifact: {
            source: "usr/sbin/dropbear",
            sha256:
              "f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2",
            offset: 153184,
          },
        },
      ],
    },
    IdleTimeout: {
      description:
        "SSH 连接无传输活动的超时，单位秒；非零时原厂传 -I，0 时不传此参数。",
      defaultValue: "0（原厂 init 回退；不传 -I）",
      unit: "秒",
      range: "非负整数；0 不传超时参数",
      dependencies: [
        "与 SSHKeepAlive 分别控制空闲时长和保活发送。",
        "原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。",
        "be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。",
      ],
      flags: ["version-dependent"],
      impact:
        "超时可断开无传输活动的连接；与定期保活的 SSHKeepAlive 不同。ELF 声明显式 -I 0 不超时，但原厂 init 遇 0 时省略 -I，最终取其编译缺省，不能仅靠 UCI 的 0 推断。",
      evidence: [
        {
          source: "etc/init.d/dropbear",
          line: 43,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "IdleTimeout 以非负整数校验，回退 0。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 83,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "仅 IdleTimeout 非零时追加 -I。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 132,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。",
          endLine: 137,
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/dropbear",
          line: 83,
          endLine: 83,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "1.0.64 原厂实例同样只在 IdleTimeout 非零时追加 -I。",
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 32,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 ELF 的 -I 帮助声明空闲超时单位为秒、0 表示永不超时；编译缺省仍为格式占位符。",
          artifact: {
            source: "usr/sbin/dropbear",
            sha256:
              "f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2",
            offset: 153557,
          },
        },
      ],
      summary:
        "空闲超时（秒）；非零传 -I，0 不传参数，最终取 Dropbear 自身缺省。",
    },
    SSHKeepAlive: {
      description:
        "发送 SSH 保活消息的间隔，单位秒；非零时原厂传 -K，0 时省略该参数。",
      defaultValue: "300（原厂 init 校验回退）",
      unit: "秒",
      range: "非负整数；0 不传保活参数",
      dependencies: [
        "SSHKeepAlive 不是 IdleTimeout；保活不会替代监听地址与认证配置。",
        "原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。",
        "be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。",
      ],
      flags: ["version-dependent"],
      impact:
        "可用于发现失去响应的连接，与 TCP 系统级 keepalive 不同。ELF 声明显式 -K 0 不发送保活，但原厂 init 遇 0 时省略 -K，最终取其编译缺省；缩短间隔会增加保活消息。",
      evidence: [
        {
          source: "etc/init.d/dropbear",
          line: 42,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "SSHKeepAlive 以非负整数校验，回退 300。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 84,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "仅 SSHKeepAlive 非零时追加 -K。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 132,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。",
          endLine: 137,
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/dropbear",
          line: 84,
          endLine: 84,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "1.0.64 原厂实例同样只在 SSHKeepAlive 非零时追加 -K。",
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 33,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 ELF 的 -K 帮助声明保活间隔单位为秒、0 表示不发送；编译缺省仍为格式占位符。",
          artifact: {
            source: "usr/sbin/dropbear",
            sha256:
              "f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2",
            offset: 153504,
          },
        },
      ],
      summary:
        "SSH 保活间隔（秒）；非零传 -K，0 不传参数，最终取 Dropbear 自身缺省。",
    },
    MaxAuthTries: {
      description:
        "每个 SSH 连接的认证尝试上限；非零时传 -T，0 时留给 Dropbear 自身缺省。",
      defaultValue: "3（原厂 init 校验回退）",
      unit: "次/连接",
      range: "init 校验非负整数；0 不传 -T，实际非零上限由 Dropbear 构建决定",
      dependencies: [
        "客户端发送的多把公钥也可能消耗认证尝试；不是账号密码修改。",
        "原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。",
        "be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。",
      ],
      flags: ["version-dependent"],
      impact:
        "过小可能让带多个身份密钥的客户端在尝试正确凭据前就被断开。它不是全局失败次数或基于 IP 的封禁规则。",
      evidence: [
        {
          source: "etc/init.d/dropbear",
          line: 44,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "MaxAuthTries 以非负整数校验，回退 3。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 85,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "仅 MaxAuthTries 非零时追加 -T。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 132,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。",
          endLine: 137,
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/dropbear",
          line: 85,
          endLine: 85,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "1.0.64 原厂实例同样只在 MaxAuthTries 非零时追加 -T。",
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 34,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 ELF 的 -T 帮助声明每次连接的最大认证尝试；非零值起点为 1，上限/缺省在此为占位符。",
          artifact: {
            source: "usr/sbin/dropbear",
            sha256:
              "f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2",
            offset: 153059,
          },
        },
      ],
    },
    BannerFile: {
      description:
        "认证前发送给 SSH 客户端的提示文件路径；原厂校验 file，非空时传 -b。不是交互登录后的 shell 提示。",
      dependencies: [
        "路径指向路由器已有可读取的提示文件。",
        "原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。",
        "be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。",
      ],
      flags: ["version-dependent"],
      impact:
        "加载有效文件后，新登录能看到提示；文件需在实例启动时可读取。它不会改变认证方式，也不能在此字段填写文件内容。",
      evidence: [
        {
          source: "etc/init.d/dropbear",
          line: 40,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "BannerFile 校验为 file。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 81,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "非空 BannerFile 通过 -b 传给 Dropbear。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 132,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。",
          endLine: 137,
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/dropbear",
          line: 81,
          endLine: 81,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "1.0.64 原厂实例同样以 -b 传入非空 BannerFile。",
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 36,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 ELF 帮助说明提示文件内容在用户登录前展示。",
          artifact: {
            source: "usr/sbin/dropbear",
            sha256:
              "f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2",
            offset: 152688,
          },
        },
      ],
    },
    keyfile: {
      description:
        "SSH 服务器主机密钥文件路径，不是客户端登录公钥内容。1.0.43 原厂 init 读取的是 rsakeyfile，未读取目录中列出的 keyfile。",
      dependencies: [
        "文件需是有效主机私钥，勿在本字段粘贴私钥内容。",
        "keyfile 与 rsakeyfile 不是可证实的原厂别名；独立救援的使用需看其管理实现。",
        "be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。",
      ],
      flags: ["credential", "version-dependent"],
      discovery:
        "已检索 etc/init.d/dropbear、lib、原厂与反编译 Lua，未找到 dropbear.keyfile 的读取；原厂校验/实例变量/命令生成均用 rsakeyfile。ELF 支持 -r keyfile 仅证明命令行能力，不证明 UCI keyfile 接线。",
      impact:
        "保存 keyfile 不保证原厂切换主机身份；独立救援可按自己的管理实现使用密钥路径。真正更换主机密钥时客户端可能报告指纹变化，此项仍可编辑。",
      evidence: [
        {
          source: "etc/init.d/dropbear",
          line: 39,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂声明的主机密钥字段是 rsakeyfile:file。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 80,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂把非空 rsakeyfile 传入 -r；紧邻的 BannerFile 则传入 -b。",
          endLine: 81,
        },
        {
          source: "etc/init.d/dropbear",
          line: 93,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 keygen 检查默认 RSA 主机密钥，缺失时在 /tmp 生成并移到 /etc/dropbear。",
          endLine: 113,
        },
        {
          source: "etc/init.d/dropbear",
          line: 132,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。",
          endLine: 137,
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/dropbear",
          line: 80,
          endLine: 80,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "1.0.64 原厂实例仍从 rsakeyfile 而不是 keyfile 生成 -r 参数。",
        },
        {
          source: "docs/field-help-system-dropbear-research.md",
          line: 37,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 ELF 支持可重复的 -r 主机密钥文件参数；这不证明 UCI keyfile 被 init 读取。",
          artifact: {
            source: "usr/sbin/dropbear",
            sha256:
              "f5e6838edc8b218f7f95a4f20e4d9e9050c8f9025f613d252d823ab17743c7d2",
            offset: 152773,
          },
        },
      ],
      summary:
        "主机密钥文件路径；原厂 init 读取 rsakeyfile，不证明 keyfile 生效；独立救援另行管理。",
    },
    enable: {
      description:
        "此 Dropbear 章节的启用开关；原厂值为 0 时不创建该实例。与 rc.common 的服务自启动 enable 命令不同。",
      defaultValue: "1（原厂 init 校验回退）",
      dependencies: [
        "原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。",
        "be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。",
      ],
      flags: ["version-dependent"],
      impact:
        "停用此项会使原厂加载时跳过本章节，其他章节不因此关闭。不能用它推断独立救援 SSH 的状态，也不是强制删除已有会话的 killclients 命令。",
      evidence: [
        {
          source: "etc/init.d/dropbear",
          line: 34,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "enable 布尔校验回退 1。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 70,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "enable=0 时从 dropbear_instance 返回，不创建实例。",
          endLine: 75,
        },
        {
          source: "etc/init.d/dropbear",
          line: 118,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "接口触发器仅收集 enable=1 的章节接口。",
          endLine: 121,
        },
        {
          source: "etc/init.d/dropbear",
          line: 132,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。",
          endLine: 137,
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/dropbear",
          line: 70,
          endLine: 70,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "1.0.64 原厂实例同样在 enable=0 时返回。",
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/dropbear",
          line: 132,
          endLine: 137,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "1.0.64 原厂脚本也在 ssh_en 不为 1 或 CHANNEL=release 时返回；只证明此 init 的门槛。",
        },
      ],
    },
    mdns: {
      description:
        "通过 procd 的 mDNS 集成公布 SSH TCP 服务。非零时注册 ssh/tcp，使用该实例的 Port。",
      defaultValue: "1（原厂 init 校验回退）",
      dependencies: [
        "需要已启动的 SSH 实例和可用 mDNS responder；公布端口取 Port。",
        "原厂 init 另要求 nvram ssh_en=1 且 CHANNEL 不是 release；这是启动门槛，不是字段权限限制。",
        "be6500panel 管理的独立救援 SSH 走单独启动路径；此原厂门槛不支配救援实例，字段仍可编辑。",
      ],
      flags: ["version-dependent"],
      impact:
        "有 mDNS responder 集成时，局域网客户端可发现 SSH 服务。此项不打开防火墙、不创建监听端口；缺少 mDNS responder 时公布行为取决于运行环境。",
      evidence: [
        {
          source: "etc/init.d/dropbear",
          line: 46,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "mdns 布尔校验回退 1。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 88,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "mdns 非零时调用 procd_add_mdns ssh tcp，传入 Port 与 daemon=dropbear。",
        },
        {
          source: "etc/init.d/dropbear",
          line: 132,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 start_service 在 ssh_en 不为 1 或 channel 为 release 时返回，不进入实例创建。",
          endLine: 137,
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/dropbear",
          line: 88,
          endLine: 88,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "1.0.64 原厂实例同样通过 procd_add_mdns 公布 ssh/tcp 服务。",
        },
      ],
    },
  },
};
