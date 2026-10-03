import type { ModuleFieldHelp } from "./types";

// Consumer evidence is from RN02 1.0.43 unless explicitly labeled otherwise.
// Schema semantics are kept separate from bounded binary-only discoveries.
export const dhcpFieldHelp: ModuleFieldHelp = {
  dnsmasq: {
    domainneeded: {
      description: "只转发含域名部分的 DNS 查询；短主机名优先留在本地解析。",
      defaultValue: "0（未启用）",
      dependencies: ["作用于 dnsmasq 的 DNS 查询转发。"],
      impact: "会减少向上游泄露短主机名；依赖短名的外部查询可能不再得到答复。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1041,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "domainneeded 被转换为 --domain-needed。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 159,
          endLine: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 80,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明不转发没有域名部分的查询。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 207858,
          },
        },
      ],
    },
    boguspriv: {
      description:
        "通用 dnsmasq 用此项过滤未在本地找到的私网反向查询；本固件读取此项的代码已被注释，不能确认开关生效。",
      flags: ["version-dependent"],
      discovery:
        "已检索 1.0.43 dnsmasq 启动脚本、lib shell、厂商文本脚本与反编译 Lua；未找到仍执行的 boguspriv 配置读取。父级保存的 1.0.64 公共 dnsmasq 脚本也有相同注释。未覆盖额外配置文件或未反编译二进制。",
      impact:
        "仅修改此字段未必改变反向查询行为；不要据此判断私网 PTR 查询已经被拦截。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1252,
          endLine: 1255,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "boguspriv 的 config_get_bool 行被注释；后续仅检查同名变量后追加 --bogus-priv。",
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/dnsmasq",
          line: 1252,
          endLine: 1255,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "已保存的 1.0.64 公共脚本仍将 boguspriv 配置读取注释，只检查变量后追加 --bogus-priv。",
        },
      ],
      summary: "原厂读取代码已注释；不能确认修改此项会改变私网反向查询。",
    },
    filterwin2k: {
      description:
        "传入 --filterwin2k，过滤旧式 Windows 会触发的部分 DNS 查询。",
      defaultValue: "0（未启用）",
      flags: ["legacy"],
      impact: "可能减少无用上游请求，也可能影响仍依赖这些记录的旧客户端。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1042,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "filterwin2k 被转换为 --filterwin2k。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 159,
          endLine: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 81,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明不转发 Windows 主机的无用 DNS 请求。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 208005,
          },
        },
      ],
    },
    localise_queries: {
      description:
        "按接收查询的接口本地化 DNS 答复，适合多网段下有多个本地地址的主机记录。",
      defaultValue: "0（未启用）",
      dependencies: ["主要影响 dnsmasq 读取的本地主机记录。"],
      impact: "多网段内同一主机名可能得到不同地址；不用于替换上游 DNS 服务器。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1048,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "localise_queries 被转换为 --localise-queries。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 159,
          endLine: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 82,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明根据收到查询的接口回答 DNS。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 210858,
          },
        },
      ],
    },
    rebind_protection: {
      description:
        "启用 DNS 重绑定保护，过滤上游答复中的 RFC1918 私网地址。需要解析到内网的域名可用 rebind_domain 指定例外。",
      defaultValue: "1（启用）",
      dependencies: ["rebind_localhost 与 rebind_domain 仅在此项启用时读取。"],
      impact:
        "会拦截部分确实指向内网的域名；可信例外应通过 rebind_domain 单独设置。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1162,
          endLine: 1168,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rebind_protection 缺省为 1；启用后追加 --stop-dns-rebind，并记录丢弃上游 RFC1918 响应。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 102,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明解析时过滤私网地址以阻止 DNS 重绑定。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 212890,
          },
        },
      ],
    },
    rebind_localhost: {
      description: "允许 DNS 重绑定保护中的 127.0.0.0/8 回环地址例外。",
      defaultValue: "0（不放行）",
      dependencies: ["需启用 rebind_protection。"],
      impact:
        "开启后，外部 DNS 可返回回环地址；不会同时放行全部 RFC1918 私网地址。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1163,
          endLine: 1175,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rebind_localhost 位于重绑定保护分支内，缺省为 0；为真时追加 --rebind-localhost-ok，并说明允许 127.0.0.0/8。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 103,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助明确允许 127.0.0.0/8 重绑定例外。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 212951,
          },
        },
      ],
    },
    rebind_domain: {
      description: "按域名列表放行重绑定保护，否则可能被丢弃的私网 DNS 答复。",
      dependencies: [
        "需启用 rebind_protection。",
        "保留 dnsmasq 原生域名匹配语法和列表边界。",
      ],
      impact: "指定域名可解析到内网地址；例外范围过大会削弱重绑定保护。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1177,
          endLine: 1182,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "rebind_domain 的每项转换为 --rebind-domain-ok；日志说明允许该域名的 RFC1918 响应。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 104,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明对指定域名停用重绑定保护。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 213000,
          },
        },
      ],
    },
    local: {
      description:
        "传入 dnsmasq 的本地域匹配规则，例如 /lan/，该域不走普通上游转发。",
      dependencies: ["与本地 hosts、DHCP 名称和 server 的域名转发规则配合。"],
      impact:
        "本地域缺失的记录可能不会继续向公网查询；厂商 pdnsd 切换流程也会改动此项。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1087,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "local 的非空值传入 --local。",
        },
        {
          source: "usr/sbin/sysapi",
          line: 1008,
          endLine: 1010,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "sysapi 的 pdnsd 关闭分支删除 local 并写入 resolvfile。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 105,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明永不向上游转发指定域名查询。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 209990,
          },
        },
      ],
    },
    domain: {
      description:
        "设置 DHCP 与本地主机使用的域名后缀，并可用于路由器自身的搜索域。",
      dependencies: [
        "expandhosts 控制 hosts 短名的扩展。",
        "静态租约 dns=1 时也使用此后缀。",
      ],
      impact: "会改变 DHCP 客户端域名、本地记录的完整名称及路由器搜索路径。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1086,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "domain 传入 --domain。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 372,
          endLine: 375,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "静态租约生成 hosts 记录时将 DOMAIN 追加到名称。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 1369,
          endLine: 1373,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "localuse 与 ADD_LOCAL_DOMAIN 开启且 DOMAIN 非空时，在 resolv.conf 写入 search DOMAIN。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 106,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明指定 DHCP 租约分配的域名。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 210053,
          },
        },
      ],
    },
    expandhosts: {
      description: "给 hosts 文件中的短名称追加 domain 指定的域名后缀。",
      defaultValue: "0（未启用）",
      dependencies: ["需设置适合本网络的 domain。"],
      impact:
        "同一 hosts 地址可用完整本地域名查询；domain 不正确会产生错误名称。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1060,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "expandhosts 被转换为 --expand-hosts。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 159,
          endLine: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 83,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明给 /etc/hosts 的简单名称追加域后缀。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 207951,
          },
        },
      ],
    },
    authoritative: {
      description: "将 dnsmasq 声明为其 DHCPv4 网络的权威地址服务器。",
      defaultValue: "0（未启用）",
      dependencies: [
        "需由 dnsmasq 实际提供 DHCP；maindhcp 可能让地址服务交给 odhcpd。",
      ],
      impact:
        "可更快处理旧地址请求；同网段还有其他 DHCP 服务器时可能出现错误拒绝或地址冲突。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1039,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "authoritative 被转换为 --dhcp-authoritative。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 159,
          endLine: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 84,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明假设本机是本地网络唯一 DHCP 服务器。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 209042,
          },
        },
      ],
    },
    readethers: {
      description:
        "读取 /etc/ethers 的 MAC 与地址映射，作为静态 DHCP 分配依据。",
      defaultValue: "0（未启用）",
      dependencies: ["需有可读取且格式正确的 /etc/ethers。"],
      impact:
        "ethers 内容会影响匹配客户端的固定地址；与 host 租约配置应保持一致。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1049,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "readethers 被转换为 --read-ethers。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 1396,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dnsmasq 的 jail 挂载包括 /etc/ethers。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 159,
          endLine: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 85,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明读取文件中的 DHCP 静态主机信息；路径占位符没有作为默认值推断。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 211100,
          },
        },
      ],
    },
    leasefile: {
      description:
        "设置 dnsmasq 保存动态租约的文件，启动时会创建缺失文件并允许服务写入。",
      defaultValue: "/tmp/dhcp.leases",
      dependencies: ["文件所在路径须可写；/tmp 下的租约不跨重启持久保存。"],
      impact:
        "会改变租约保存和读取位置；不可写路径会影响租约持久记录。厂商设备列表也读取此路径。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1115,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "leasefile 传入 --dhcp-leasefile，明确缺省为 /tmp/dhcp.leases。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 1144,
          endLine: 1145,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "缺失的租约文件通过 touch 创建。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 1397,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "leasefile 被加入 jail 可写挂载。",
        },
      ],
    },
    resolvfile: {
      description:
        "通用配置指定上游 DNS 文件；本固件启动时不读取这个值，而固定使用 /tmp/resolv.conf.auto。停止流程仍会读取此项。",
      flags: ["version-dependent"],
      dependencies: ["noresolv=1 时不使用启动脚本的上游解析文件。"],
      discovery:
        "已核对 1.0.43 dnsmasq 启动、停止与 sysapi pdnsd 流程；resolvfile 有写入和停止读取，但启动读取被注释。父级保存的 1.0.64 公共 dnsmasq 脚本此分支相同；未检查 1.0.64 私有配置或实际解析效果。",
      impact:
        "自定义路径不会按通用 OpenWrt 预期改变启动时的上游 DNS；也可能改变停止服务后恢复 resolv.conf 的判定。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1148,
          endLine: 1154,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "noresolv 未启用时，读取 resolvfile 的代码被注释，脚本固定使用 /tmp/resolv.conf.auto。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 1407,
          endLine: 1412,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "停止流程读取 resolvfile，并在其为 /tmp/resolv.conf.auto 且 noresolv=0 时默认恢复系统 resolv.conf。",
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/dnsmasq",
          line: 1148,
          endLine: 1154,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "已保存的 1.0.64 公共脚本仍固定使用 /tmp/resolv.conf.auto，未恢复 resolvfile 的启动配置读取。",
        },
      ],
      summary:
        "原厂启动时固定使用 /tmp/resolv.conf.auto；此字段仍参与停止处理。",
    },
    noresolv: {
      description: "不读取上游解析文件；仍可使用 server 等显式 DNS 转发规则。",
      defaultValue: "0（读取解析文件）",
      dependencies: ["启用时应检查 server 或其他显式上游来源。"],
      impact: "如果没有可用的显式上游，非本地域名可能无法解析。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1047,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "noresolv 被转换为 --no-resolv。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 1148,
          endLine: 1156,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "noresolv 缺省为 0；仅在它不是 1 时添加固定上游解析文件。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 86,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明不读取 resolv.conf。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 209714,
          },
        },
      ],
    },
    nohosts: {
      description: "不读取系统 /etc/hosts；与额外 addnhosts 文件的开关分开。",
      defaultValue: "0（读取系统 hosts）",
      impact:
        "系统 hosts 中的本地名称可能不再解析；不会自动停用脚本生成的额外主机记录。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1043,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "nohosts 被转换为 --no-hosts。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 1105,
          endLine: 1113,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "启动脚本仍独立添加生成的 HOSTFILE 及 addnhosts。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 159,
          endLine: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。",
        },
      ],
    },
    nonwildcard: {
      description:
        "启用 --bind-dynamic，只绑定当前允许的接口地址，并跟随地址变化。",
      defaultValue: "1（启用）",
      dependencies: [
        "监听范围还受 interface、notinterface、listen_address 影响。",
      ],
      impact:
        "有助于避免与其他 DNS 实例争用监听地址；接口选择不当会让客户端失去 DNS 服务。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1064,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "nonwildcard 转换为 --bind-dynamic，明确缺省为 1。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 108,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明绑定已使用的接口并检查新接口。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 214766,
          },
        },
      ],
    },
    localservice: {
      description: "限制 DNS 查询来源为本机所连接的本地子网。",
      defaultValue: "0（未启用）",
      dependencies: [
        "dnsmasq 的显式监听接口或地址设置可能影响该开关的适用范围。",
      ],
      impact:
        "跨路由远程客户端可能被拒绝；这是 DNS 来源限制，不等于 DHCP 地址池 ignore。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1067,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "localservice 被转换为 --local-service。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 159,
          endLine: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 87,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明只接受直接连接网络的查询。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 216010,
          },
        },
      ],
    },
    strictorder: {
      description: "要求 dnsmasq 按上游列表顺序尝试 DNS 服务器。",
      defaultValue: "0（未启用）",
      dependencies: ["检查 server 列表和自动上游 DNS 的顺序。"],
      impact:
        "第一台上游较慢或故障时可能增加等待；与 allservers 的并发意图不同。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1045,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "strictorder 被转换为 --strict-order。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 159,
          endLine: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 88,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明严格按给定顺序使用名称服务器。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 209368,
          },
        },
      ],
    },
    allservers: {
      description: "向所有可用上游 DNS 服务器并发发送查询，采用先返回的答复。",
      defaultValue: "0（未启用）",
      dependencies: ["与 strictorder 的顺序查询策略一并检查。"],
      impact:
        "增加上游查询量和隐私暴露范围；不同上游回答不一致时结果可能变化。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1071,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "allservers 被转换为 --all-servers。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 159,
          endLine: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 89,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明每次都向全部服务器发送 DNS 查询。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 213046,
          },
        },
      ],
    },
    logqueries: {
      description: "记录 DNS 查询，并使用 extra 格式携带更详细的查询上下文。",
      defaultValue: "0（未启用）",
      impact: "日志会增长，并可包含客户端地址和查询域名；适合短期排错。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1046,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "logqueries 被转换为 --log-queries=extra。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 159,
          endLine: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 90,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明记录 DNS 查询。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 209644,
          },
        },
      ],
    },
    logdhcp: {
      description: "记录 DHCP 分配与处理的详细信息。",
      defaultValue: "0（未启用）",
      dependencies: ["仅对 dnsmasq 实际处理的 DHCP 有作用。"],
      impact: "增加日志量，并可记录客户端标识与租约信息；不会直接扩大地址池。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1068,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "logdhcp 被转换为 --log-dhcp。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 159,
          endLine: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 91,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明额外 DHCP 日志。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 212801,
          },
        },
      ],
    },
    port: {
      description:
        "设置 dnsmasq 的 DNS 监听端口；0 表示关闭 DNS 功能，不等于停用 DHCP。",
      unit: "端口号",
      range: "0–65535；0 关闭 DNS",
      dependencies: ["检查防火墙放行规则及其他 DNS 服务的端口占用。"],
      impact:
        "客户端通常访问 53 端口；改为其他端口需要配套转发或客户端设置，DNS 功能关闭会影响普通解析。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1080,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "port 的非空值传入 --port；此处未指定数值缺省。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 92,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助明确 DNS 监听端口默认是 53。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 209520,
          },
        },
      ],
      defaultValue: "53（dnsmasq 内置帮助明确说明）",
    },
    queryport: {
      summary: "0 开启单端口复用，不等于留空时的默认随机端口策略。",
      description:
        "设置上游 DNS 查询的来源端口。0 启用单端口复用模式，由系统或端口范围策略选取端口；不等于留空时的随机查询端口策略。",
      unit: "端口号",
      range: "0–65535",
      dependencies: [
        "影响 DNS 上游请求；与监听 port 无关。",
        "0 值选取端口时仍受 minport/maxport 范围策略影响；socket 按来源地址及接口条件匹配复用。",
      ],
      impact:
        "显式端口或 0 的复用模式会减少上游查询来源端口的变化，可能降低 DNS 抗伪造能力；复用受来源地址及接口条件限制，不代表所有接口共用一个端口。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1083,
          endLine: 1085,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "queryport 传入 --query-port，与 minport/maxport 分别配置；此处没有数值缺省。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 93,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明强制指定上游 DNS 查询的来源端口。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 209661,
          },
        },
        {
          source: "docs/field-help-queryport-review.md",
          line: 35,
          endLine: 50,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "query-port=0 分支将共享查询 socket 模式标志设为 1，不是省略选项或恢复默认。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 89592,
          },
        },
        {
          source: "docs/field-help-queryport-review.md",
          line: 54,
          endLine: 85,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "共享查询 socket 列表按来源地址及接口/名称条件匹配，匹配时复用已有 socket；不是每次查询重新分配端口。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 112160,
          },
        },
        {
          source: "docs/field-help-queryport-review.md",
          line: 89,
          endLine: 104,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "新建 socket 的来源端口为 0 时，有端口范围则在范围内选取；没有范围时由系统选取，之后可复用该 socket。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 107156,
          },
        },
      ],
    },
    cachesize: {
      description: "设置 DNS 缓存条目上限；0 关闭常规 DNS 缓存。",
      unit: "条",
      range: "非负整数",
      impact:
        "较大缓存会占用更多内存；较小或关闭缓存会增加重复查询及上游延迟。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1078,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "cachesize 的非空值传入 --cache-size；此处没有数值缺省。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 94,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明缓存大小按条目计数；默认数值仅是未解析的占位符。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 207693,
          },
        },
      ],
    },
    dnsforwardmax: {
      description: "限制同时等待上游答复的 DNS 转发请求数量。",
      unit: "个并发请求",
      range: "非负整数；0 的具体处理未在启动脚本中说明。",
      impact:
        "上限过低时忙碌网络的 DNS 请求可能排队或失败；增大上限会增加资源占用。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1079,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dnsforwardmax 的非空值传入 --dns-forward-max；此处没有数值缺省。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 95,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明控制最大并发 DNS 查询数，默认数值未在该字符串解析。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 212009,
          },
        },
      ],
    },
    dhcpleasemax: {
      description:
        "设置 dnsmasq 同时记录的 DHCP 租约数量上限，不是每个地址池的地址数。",
      unit: "条租约",
      range: "非负整数；0 的具体处理未在启动脚本中说明。",
      dependencies: ["仅用于 dnsmasq 提供的 DHCP 租约。"],
      impact:
        "租约表耗尽时新客户端可能无法获得地址；应与各池 limit 和内存容量协调。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1082,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dhcpleasemax 的非空值传入 --dhcp-lease-max；此处没有数值缺省。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 96,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明控制最大 DHCP 租约数，默认数值未在该字符串解析。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 210800,
          },
        },
      ],
    },
    ednspacket_max: {
      description: "设置 dnsmasq 允许的 EDNS UDP DNS 数据包最大大小。",
      unit: "字节",
      range: "面板范围 512–65535；固件启动脚本未校验数值。",
      impact:
        "较大的 UDP 数据包可能被路径 MTU 或防火墙限制；过小可能增加截断与 TCP 重试。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1081,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "ednspacket_max 的非空值传入 --edns-packet-max；此处没有数值缺省。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 97,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明最大支持的 EDNS.0 UDP 包大小，默认数值未在该字符串解析。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 209581,
          },
        },
      ],
    },
    server: {
      description:
        "指定上游 DNS 服务器，保留 /域名/服务器#端口 等 dnsmasq 原生匹配语法及每个列表项。",
      dependencies: [
        "noresolv=1 时尤其需要可用的显式上游。",
        "allservers 和 strictorder 影响上游选择策略。",
      ],
      impact:
        "可把特定域名送往不同上游；错误地址、端口或匹配规则会造成解析失败。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 181,
          endLine: 183,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "append_server 将每项原样追加为 --server。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 1089,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "server 通过 config_list_foreach 逐项调用 append_server。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 98,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明配置可带域名匹配条件的上游服务器地址。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 209827,
          },
        },
      ],
    },
    address: {
      description:
        "按原生 /域名/IP地址 规则直接回答 DNS 查询，不需要访问上游。",
      dependencies: ["区分固定 address 答复与 server 上游转发规则。"],
      impact:
        "匹配域名及其范围内的查询会被重写；范围过大会让无关站点指向错误地址。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 189,
          endLine: 191,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "append_address 将每项原样追加为 --address。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 1091,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "address 通过 config_list_foreach 逐项转换。",
        },
      ],
    },
    interface: {
      description:
        "按列表选择服务接口；先将逻辑网络名转换为设备名，无法转换时保留原值。",
      dependencies: [
        "与 notinterface、listen_address、nonwildcard 一并检查。",
        "boot 阶段的生成路径不会读取此列表。",
      ],
      impact:
        "会改变 DNS/DHCP 可服务的网卡范围；漏选 LAN 可能让本地客户端失去服务。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 201,
          endLine: 204,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "append_interface 使用 network_get_device；失败时沿用输入，再生成 --interface。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 1101,
          endLine: 1104,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "非 BOOT 分支读取 interface 与 notinterface 列表。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 99,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明选择监听接口。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 208551,
          },
        },
      ],
    },
    notinterface: {
      description:
        "按列表排除服务接口；逻辑网络名称会转换成设备名后传给 --except-interface。",
      dependencies: [
        "应与 interface 的允许列表核对；boot 生成路径不读取此列表。",
      ],
      impact:
        "可避免在 WAN 等接口暴露服务；排除客户端所在接口会造成 DNS 或 DHCP 不可达。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 210,
          endLine: 213,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "append_notinterface 将逻辑网络转换为设备，失败时沿用输入，并生成 --except-interface。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 1101,
          endLine: 1104,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "非 BOOT 分支逐项读取 notinterface。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 100,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明选择不监听的接口。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 208586,
          },
        },
      ],
    },
    listen_address: {
      description: "按列表指定本机 DNS 监听 IP 地址，值原样传给 dnsmasq。",
      dependencies: [
        "与 interface、notinterface 和 nonwildcard 共同决定监听范围。",
      ],
      impact:
        "地址不属于本机或缺少客户端可达地址时，DNS 可能无法启动或无法访问。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 206,
          endLine: 208,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "append_listenaddress 为每项生成 --listen-address。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 1088,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "listen_address 通过 config_list_foreach 读取。",
        },
      ],
    },
    addnhosts: {
      description:
        "指定额外 hosts 文件；启动脚本把路径加入服务挂载并逐项传给 dnsmasq。",
      dependencies: ["每项是可读取的 hosts 文件路径，不是域名列表。"],
      impact:
        "额外文件中的名称可覆盖正常上游解析预期；缺失或不可读文件会缺少相应记录。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 229,
          endLine: 232,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "append_addnhosts 将路径加入 EXTRA_MOUNT，并追加 --addn-hosts。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 1113,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "addnhosts 列表逐项调用 append_addnhosts。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 101,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明额外读取 hosts 文件。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 208465,
          },
        },
      ],
    },
    dhcp_option: {
      description:
        "为此 dnsmasq 实例添加全局 DHCP 选项；保留编号、值、逗号及独立列表项。",
      dependencies: ["接口段 dhcp_option 有网络标签；与全局选项共同检查。"],
      impact:
        "可改变所有匹配客户端的 DNS、网关等网络参数；错误选项会让已获地址的客户端仍无法联网。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1204,
          endLine: 1205,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dnsmasq 段调用 dhcp_option_add，网络标签为空；普通和强制选项各调用一次。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 769,
          endLine: 791,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dhcp_option_add 优先读取原生列表，兼容但警告旧式 option 字符串，并逐项生成 DHCP 选项。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 107,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明配置发给 DHCP 客户端的选项。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 209419,
          },
        },
      ],
    },
  },
  dhcp: {
    interface: {
      description:
        "关联 /etc/config/network 的逻辑网络；dnsmasq 必须找到其设备与子网后才生成地址池。",
      dependencies: [
        "关联网络必须存在并处于可用状态。",
        "ignore、force 与 DHCP 服务归属共同决定是否实际分配。",
      ],
      impact:
        "改错网络名会使地址池被跳过；除厂商 LAN AP 别名分支外，dnsmasq 仅处理 static 接口。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 574,
          endLine: 587,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "interface 必须非空并可通过 network_get_device 获得设备；有别名时尝试对应 _alias 网络。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 599,
          endLine: 606,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "LAN AP 别名分支使用别名子网，否则要求网络协议为 static。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 15,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 interface 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 67984,
          },
        },
      ],
    },
    start: {
      description: "地址池起点相对于接口子网的偏移量，不是完整 IPv4 地址。",
      defaultValue: "100",
      unit: "个地址偏移",
      range: "须落在关联 IPv4 子网的可用地址范围；脚本未设置固定 65535 上限。",
      dependencies: [
        "通常与 limit、接口 IPv4 地址和 netmask 一起计算。",
        "CPE 桥接模式可使用上游生成的固定地址范围。",
      ],
      impact:
        "会移动可分配地址范围；若同时存在厂商 startip/endip，偏移量路径会被绕过。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 640,
          endLine: 645,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "start 的明确回退为 100；脚本也读取厂商 startip/endip。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 664,
          endLine: 672,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "没有完整 startip/endip 时，将 start 经 dhcp_calc 转换并传给 ipcalc.sh；有完整起止地址时直接使用它们。",
        },
      ],
    },
    limit: {
      description: "偏移量地址池最多包含的 IPv4 地址数量。",
      defaultValue: "150",
      unit: "个地址",
      range: "非负整数；需适配子网，0 不走通常的减 1 路径。",
      dependencies: [
        "与 start 和 netmask 共同计算范围；dynamicdhcp=0 将范围改为静态分配。",
      ],
      impact:
        "普通偏移池的末地址超过子网广播地址减 1 时会被裁剪，实际数量可能小于 limit；仍需避免覆盖已有静态地址。完整 startip/endip 走另一路径，不使用 limit，也不经过此裁剪。0 不能当作停用池。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 641,
          endLine: 643,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "limit 明确回退为 150。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 664,
          endLine: 667,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "普通偏移池仅在 limit 大于 0 时减 1，随后将 start 和 limit 传给 ipcalc.sh 计算地址范围。",
        },
        {
          source: "bin/ipcalc.sh",
          line: 49,
          endLine: 55,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "起点不低于 network+1；末地址按 start 加数量偏移计算，超过广播地址减 1 时被裁到该值。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 668,
          endLine: 672,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "完整 startip/endip 路径直接赋值 START/END，不使用 limit，也不调用普通偏移池的 ipcalc.sh。",
        },
      ],
    },
    leasetime: {
      description:
        "设置客户端可使用租用地址的时间。12h 表示 12 小时；原厂将此原生字符串传给 dnsmasq，特定客户端可另设 host.leasetime。",
      defaultValue: "12h（普通地址池回退）",
      unit: "时间字符串：s、m、h 等单位或 infinite",
      dependencies: [
        "host.leasetime 可为特定客户端另设租期。",
        "厂商 CPE 桥接路径使用 120 秒，不采用普通池回退。",
      ],
      impact:
        "更短的租约会增加续租请求；更长的租约会让旧地址占用和配置切换持续更久。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 640,
          endLine: 643,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "普通地址池路径中 leasetime 明确回退为 12h。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 628,
          endLine: 636,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "CPE 桥接且获得上游地址时，将 leasetime 设置为 120。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 682,
          endLine: 683,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "leasetime 被追加到 --dhcp-range。",
        },
      ],
    },
    ignore: {
      description:
        "让 dnsmasq 在关联设备上禁用 DHCP，并立即跳过该地址池。LAN 此值还会被原厂启动前处理按运行模式重写。",
      defaultValue: "0（不忽略）",
      dependencies: [
        "interface 指定被忽略的接口；不用此项判断 odhcpd 的 IPv6 模式。",
        "LAN 启动前重写受 misc.features.SkipForceDhcp、厂商运行模式及 miwifi_force_ignore 影响。",
      ],
      impact:
        "客户端将不能从此池自动获址；DNS 监听不因此自动关闭。手动修改 LAN ignore 可能在启动前被 AP/中继模式及 miwifi_force_ignore 的判定覆盖。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 593,
          endLine: 597,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "ignore 通过 append_bool 生成 --no-dhcp-interface，随后提前返回。",
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua",
          line: 6445,
          endLine: 6451,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商 IPv6 上游配置写入 dhcp 段 ignore=1。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 159,
          endLine: 167,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "append_bool 未给出默认值时使用 0；仅为真时追加 dnsmasq 开关。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 1442,
          endLine: 1457,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "除 SkipForceDhcp=1 外，启动前按 AP/中继模式和 miwifi_force_ignore 重写并提交 dhcp.lan.ignore。",
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/dnsmasq",
          line: 1450,
          endLine: 1458,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "1.0.64 仍重写 LAN ignore，并新增 NETMODE 为空、product=ap 且 ft_mode=0 时忽略 DHCP 的分支。",
        },
      ],
      flags: ["generated", "version-dependent"],
      summary: "停用地址池；LAN 值可能被原厂启动前处理按 AP/中继模式重写。",
    },
    force: {
      description: "跳过同接口已有 DHCP 服务器的检测，仍生成本机地址池。",
      defaultValue: "0（保留检测）",
      dependencies: ["仍需 interface 可用、ignore=0 且服务提供 DHCP。"],
      impact:
        "同一网段多个服务器可能竞争分配，造成网关或地址冲突；并非强制发送 DHCP 选项的开关。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 609,
          endLine: 616,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "force 明确回退为 0；大于 0 时跳过 dhcp_check，否则发现其他服务器就返回。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 125,
          endLine: 140,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dhcp_check 检查设备状态并通过 udhcpc 发起一次 DHCP 检测，将结果保存到标记文件。",
        },
      ],
    },
    dynamicdhcp: {
      description:
        "允许向没有固定租约的客户端动态分配地址。关闭后地址池被设为 static。",
      defaultValue: "1（动态分配）",
      dependencies: ["静态分配需有相应 host 租约及可用地址。"],
      impact:
        "关闭后未匹配静态租约的客户端可能无法自动联网；不会删除已有 host 配置。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 618,
          endLine: 619,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dynamicdhcp 明确回退为 1。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 674,
          endLine: 679,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "关闭 dynamicdhcp 时 IPv4 END 设为 static、IPv6 范围设为 ::,static；开启时建立动态 IPv6 范围。",
        },
      ],
    },
    netmask: {
      description:
        "覆盖 dnsmasq 地址池计算使用的 IPv4 子网掩码；未设置时取接口子网的掩码。",
      defaultValue: "关联接口子网掩码",
      range:
        "普通偏移池保留 ipcalc.sh 接受的掩码格式；厂商完整 startip/endip 路径要求前缀长度 8–32。",
      dependencies: ["与 start、limit 及厂商 startip/endip 路径相关。"],
      impact:
        "与接口实际子网不一致会造成错误地址池或客户端路由；不会直接修改 network 中的接口掩码。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 639,
          endLine: 640,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "netmask 缺省取 subnet 的斜线后部分。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 664,
          endLine: 671,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "偏移地址池把 netmask 交给 ipcalc.sh；完整起止地址路径可调用 dhcp_calc_netmask。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 551,
          endLine: 560,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dhcp_calc_netmask 仅接受 8–32 的前缀位数，否则使用 24，再转换为点分掩码。",
        },
      ],
    },
    dhcpv4: {
      description:
        "选择是否生成本接口的 IPv4 地址池；只有 disabled 会阻止 dnsmasq 输出 DHCPv4 范围。",
      range:
        "面板模式：disabled / server；dnsmasq 判断仅区分 disabled 与其他值。",
      dependencies: [
        "dnsmasq 实际处理 DHCP 的服务归属由 odhcpd.maindhcp 等决定。",
        "ignore=1 时在读取 dhcpv4 前已返回。",
      ],
      impact: "关闭后客户端不会从该池获得 IPv4 地址；IPv6 模式是另设字段。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 621,
          endLine: 622,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "脚本读取 dhcpv4 与 dhcpv6。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 682,
          endLine: 684,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "只要 dhcpv4 不等于 disabled，就追加 IPv4 --dhcp-range。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 25,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 dhcpv4 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68064,
          },
        },
      ],
    },
    dhcpv6: {
      description:
        "配置 DHCPv6 的服务器、中继或混合模式；厂商 IPv6 流程会按网络模式重写此项。",
      range: "disabled / server / relay / hybrid",
      dependencies: [
        "odhcpd 需启动且厂商 IPv6 模式不是 off/passthrough；AP/中继运行模式可能不启动它。",
        "与 ra、master、接口 IPv6 前缀配合。",
      ],
      flags: ["version-dependent"],
      impact:
        "会改变 IPv6 地址及选项获取方式；中继还需可用上游。dnsmasq 后备路径只单独处理 disabled，不能代替 odhcpd 的完整中继行为。",
      evidence: [
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua",
          line: 6460,
          endLine: 6465,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商上游配置写入 dhcpv6=relay。",
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua",
          line: 8400,
          endLine: 8404,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "LAN IPv6 模式流程将计算后的模式写入 dhcpv6。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 705,
          endLine: 706,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dnsmasq 的 IPv6 RA 分支在 dhcpv6=disabled 时改为纯 SLAAC 模式。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 26,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 dhcpv6 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68072,
          },
        },
        {
          source: "etc/init.d/odhcpd",
          line: 8,
          endLine: 24,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。",
        },
      ],
    },
    ra: {
      description:
        "配置 IPv6 路由器通告模式；厂商 LAN IPv6 流程可设置 server 或 hybrid，上游配置可设置 relay。",
      range: "disabled / server / relay / hybrid",
      dependencies: [
        "由 odhcpd 正常处理；dnsmasq 接管时的支持范围不同。",
        "厂商 IPv6 设置可能重写此值。",
      ],
      impact:
        "改变客户端的默认 IPv6 路由与前缀获取；错误通告可导致 IPv6 断网，即使 DHCPv4 仍正常。",
      evidence: [
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua",
          line: 6452,
          endLine: 6458,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商上游写入 ra=relay。",
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua",
          line: 8380,
          endLine: 8383,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "LAN IPv6 模式将所选 ra 写入 dhcp 配置。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 687,
          endLine: 724,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dnsmasq 仅在其 DHCPv6 路径且 ra=server 时生成 RA 参数和相关 IPv6 范围。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 24,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 ra 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68056,
          },
        },
        {
          source: "etc/init.d/odhcpd",
          line: 8,
          endLine: 24,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。",
        },
      ],
    },
    ndp: {
      description:
        "配置 odhcpd 的 IPv6 邻居发现代理模式；厂商 IPv6 中继流程会写入 relay 或 hybrid。",
      range:
        "disabled / server / relay / hybrid（面板模式；server 行为未在文本消费者中证明）",
      dependencies: [
        "由 odhcpd 处理，不是 dnsmasq 的 DHCPv4 功能。",
        "中继应与 master 上游和 ra/dhcpv6 模式一致。",
      ],
      discovery:
        "检索 dnsmasq/odhcpd init、lib shell、厂商脚本与反编译 Lua：确认 ndp 的厂商写入及 odhcpd 二进制名称表；未获得 odhcpd NDP 分支的可读实现，不能仅凭面板选项确认 server 模式。",
      impact:
        "可改变跨接口 IPv6 邻居可达性；错误代理模式会造成地址可见但流量不可达。",
      evidence: [
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua",
          line: 6466,
          endLine: 6472,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商上游写入 ndp=relay。",
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua",
          line: 8384,
          endLine: 8390,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "LAN IPv6 流程写入所选 ndp 模式。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 27,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 ndp 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68080,
          },
        },
        {
          source: "etc/init.d/odhcpd",
          line: 8,
          endLine: 24,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。",
        },
      ],
      summary: "已确认原厂中继写入；server 模式未取得可读实现证明。",
    },
    master: {
      description:
        "将此接口标记为 IPv6 中继上游；厂商生成上游 DHCP 段时同时写入 master=1 与 ignore=1。",
      dependencies: ["与 ra、dhcpv6、ndp 的中继配置及对应上游逻辑网络配合。"],
      flags: ["generated"],
      discovery:
        "确认厂商 Lua 的 master 上游标记写入及 odhcpd 二进制布尔类型表；dnsmasq 文本消费者不读取 master，未取得 odhcpd 内部中继选择实现。",
      impact:
        "选错上游会让 IPv6 中继找不到正确服务；不是把此接口设为 DHCPv4 主服务器。",
      evidence: [
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua",
          line: 6440,
          endLine: 6451,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商上游配置连续写入 master=1、ignore=1。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 22,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 master 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68040,
          },
        },
        {
          source: "etc/init.d/odhcpd",
          line: 8,
          endLine: 24,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。",
        },
      ],
      summary: "原厂 IPv6 中继上游标记，不是 DHCPv4 主服务器开关。",
    },
    ra_management: {
      description:
        "旧式 RA/DHCPv6 联动模式：0 生成无状态 DHCPv6，2 生成有状态分配，其余常规值生成 SLAAC 与 DHCPv6 混合。",
      range: "面板 0 / 1 / 2；内部另用 3 表示仅 SLAAC。",
      flags: ["legacy"],
      dependencies: [
        "dnsmasq 接管时需 ra=server 且启用 DHCPv6 能力；odhcpd 也有同名表项，内部兼容细节未验证。",
      ],
      impact:
        "仅应在旧配置兼容或 dnsmasq 接管 IPv6 时理解此项；不要把它与厂商主要写入的 ra_flags 当作等价字段。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 687,
          endLine: 709,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "ra_management 在 dnsmasq DHCPv6 且 ra=server 的分支内使用；dhcpv6=disabled 时强制为 3。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 709,
          endLine: 725,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "0 对应 ra-stateless，2 对应地址 DHCP，3 对应 ra-only，其余对应 slaac。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 40,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 ra_management 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68184,
          },
        },
      ],
      summary:
        "旧式 IPv6 联动项；dnsmasq 后备路径使用，原厂主流程写 ra_flags。",
    },
    ra_default: {
      description:
        "选择默认路由通告策略。此项不能绕过原厂 WAN6 连通状态检查；检查未发现可用上联时，有效 Router Lifetime 会归零。",
      range:
        "0 / 1 / 2；参与不同策略条件，最终有效期仍可被厂商 WAN6 检查覆盖。",
      dependencies: [
        "需提供 RA 并有适合的 IPv6 路由；AP/off/passthrough 模式可能不启动 odhcpd。",
      ],
      flags: ["version-dependent"],
      discovery:
        "已取得 ra_default 的解析及策略分支、最终 Router Lifetime 的 WAN6 门控；未将面板的 2 标签推广为始终通告有效默认路由的保证。",
      impact:
        "会改变客户端是否将本机作为 IPv6 默认路由；2 不保证始终提供有效默认路由。有效期归零不等于关闭 RA 中的前缀和其他选项。",
      evidence: [
        {
          source: "docs/field-help-dhcp-research.md",
          line: 39,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 ra_default 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68176,
          },
        },
        {
          source: "etc/init.d/odhcpd",
          line: 32,
          endLine: 35,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd init 直接启动 /usr/sbin/odhcpd，没有在此处解释 RA 策略。",
        },
        {
          source: "etc/init.d/odhcpd",
          line: 8,
          endLine: 24,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。",
        },
        {
          source: "docs/field-help-queryport-review.md",
          line: 119,
          endLine: 140,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "ra_default 解析到接口策略字段；0、1、2 参与不同默认路由/前缀判定条件。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 18548,
          },
        },
        {
          source: "docs/field-help-queryport-review.md",
          line: 143,
          endLine: 161,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "生成 RA 后执行 wan6_link_check.sh；检查返回 0 时强制把 Router Lifetime 写为 0。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 25920,
          },
        },
        {
          source: "usr/sbin/wan6_link_check.sh",
          line: 6,
          endLine: 23,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "WAN6 检查只在非 dedicated、接口已 up 且有 IPv6 网关时返回 1，否则返回 0。",
        },
      ],
      summary:
        "默认路由通告仍受原厂 WAN6 检查限制；2 不保证始终提供有效默认路由。",
    },
    ra_flags: {
      description:
        "设置 RA 标志列表：managed-config、other-config、home-agent 或 none。厂商 LAN IPv6 模式会重建列表。",
      range: "独立列表项：managed-config / other-config / home-agent / none",
      dependencies: [
        "主要用于 odhcpd 的 RA 模式；dnsmasq 后备代码读取的是 ra_management。",
      ],
      flags: ["generated", "version-dependent"],
      impact:
        "可引导客户端使用 DHCPv6 获取地址或其他配置；手动值可能在更改厂商 IPv6 模式后被覆盖。",
      evidence: [
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua",
          line: 8303,
          endLine: 8339,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商 IPv6 模式按分支选择 managed-config、other-config 或 none。",
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQLanWanUtil.lua",
          line: 8412,
          endLine: 8429,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商通过 set_list 写入 ra_flags、提交 dhcp 并重启 odhcpd。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 41,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 ra_flags 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68192,
          },
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 67,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 managed-config 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68360,
          },
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 68,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 other-config 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68368,
          },
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 69,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 home-agent 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68376,
          },
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 70,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 none 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68384,
          },
        },
        {
          source: "etc/init.d/odhcpd",
          line: 8,
          endLine: 24,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。",
        },
      ],
    },
    ra_slaac: {
      description:
        "控制 RA 前缀的 Autonomous 位，让客户端可用 SLAAC 自动生成 IPv6 地址；原厂只对长度不大于 64 的前缀设置此位。",
      dependencies: ["需有 RA 通告与可用 IPv6 前缀；需另看 dhcpv6 模式。"],
      flags: ["version-dependent"],
      discovery:
        "已取得 ra_slaac 的布尔解析及前缀 Autonomous 位消费者；未追踪其他 DHCPv6 分配行为，不由此推断开关缺省值。",
      impact:
        "关闭可能使依赖 SLAAC 的客户端无法自动生成 IPv6 地址；开启也不能代替合适的前缀或默认路由。",
      evidence: [
        {
          source: "docs/field-help-dhcp-research.md",
          line: 42,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 ra_slaac 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68200,
          },
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 709,
          endLine: 725,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dnsmasq 后备路径通过 ra_management 选择 slaac/ra-only 等模式，并非读取 ra_slaac。",
        },
        {
          source: "etc/init.d/odhcpd",
          line: 8,
          endLine: 24,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。",
        },
        {
          source: "docs/field-help-queryport-review.md",
          line: 173,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "ra_slaac 解析为布尔字段；为真且前缀长度不大于 64 时设置 Autonomous 位 0x40。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 25776,
          },
        },
      ],
      summary:
        "有合适前缀时设置 RA Autonomous 位；前缀长度大于 64 时不会设置。",
    },
    ra_mininterval: {
      description:
        "设置非请求式 RA 的最短间隔。原厂在值不大于 2 时改为 3，过大时结合有效最长间隔重新计算，不是原值直接发送。",
      unit: "秒",
      range:
        "值不大于 2 时调整为 3；过大时重新计算，需结合有效 ra_maxinterval。",
      dependencies: ["由 odhcpd 的 RA 模式使用；与 ra_maxinterval 配合。"],
      flags: ["version-dependent"],
      discovery:
        "已取得 ra_mininterval 的解析及定时调整分支；本次没有推断字段缺省值或完整随机通告分布。",
      impact:
        "影响通告频率与网络开销；实际间隔可能不同于输入值，应同时检查最长间隔及默认路由有效期。",
      evidence: [
        {
          source: "docs/field-help-dhcp-research.md",
          line: 47,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 ra_mininterval 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68240,
          },
        },
        {
          source: "etc/init.d/odhcpd",
          line: 26,
          endLine: 30,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd init 只对 LAN ra_maxinterval 缺失补值，没有在此处设置 ra_mininterval。",
        },
        {
          source: "etc/init.d/odhcpd",
          line: 8,
          endLine: 24,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。",
        },
        {
          source: "docs/field-help-queryport-review.md",
          line: 175,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "ra_mininterval 解析后参与定时生成；不大于 2 时改为 3，过大时重新计算。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 24368,
          },
        },
      ],
      summary: "RA 最短间隔会由原厂调整；不大于 2 时改为 3，过大时重新计算。",
    },
    ra_maxinterval: {
      description:
        "设置 RA 通告的最长间隔。LAN 此项为空时原厂启动脚本写入 20；odhcpd 还会结合有效期调整，将有效最长间隔控制在 4–1800 秒。",
      defaultValue: "20（仅 LAN 缺失时由启动脚本写入）",
      unit: "秒",
      dependencies: [
        "需实际启动 odhcpd；AP 模式、IPv6 off/passthrough 可提前返回。",
        "应与 ra_mininterval 一起检查。",
      ],
      flags: ["generated"],
      discovery:
        "已确认 LAN 的 ra_maxinterval 启动补值及 odhcpd 定时夹限分支；未从此推断其他接口缺省或完整随机通告分布。",
      impact:
        "影响客户端发现路由器及刷新状态的速度；配置值会被调整，且 LAN 启动补值不代表所有接口都默认 20。",
      evidence: [
        {
          source: "etc/init.d/odhcpd",
          line: 26,
          endLine: 30,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd 启动脚本检查 dhcp.lan.ra_maxinterval，缺失时设为 20 并提交 dhcp。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 48,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 ra_maxinterval 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68248,
          },
        },
        {
          source: "etc/init.d/odhcpd",
          line: 8,
          endLine: 24,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。",
        },
        {
          source: "docs/field-help-queryport-review.md",
          line: 175,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "ra_maxinterval 参与定时生成；先结合有效期收窄，再将有效最长间隔控制在 4–1800 秒。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 24368,
          },
        },
      ],
      summary:
        "LAN 缺失时启动脚本写 20；有效最长间隔还会结合有效期调整至 4–1800 秒。",
      range: "有效最长间隔为 4–1800 秒，先结合有效期收窄；不是输入值原样发送。",
    },
    ra_lifetime: {
      description:
        "设置 RA 中默认路由的有效时长，不是地址租期。原厂保留 0；正值至少为有效最长通告间隔，并限制到 9000，之后仍受 WAN6 检查。",
      unit: "秒",
      dependencies: [
        "主要由 odhcpd 处理；dnsmasq 接管 IPv6 时这里的代码采用固定 7200。",
        "结合 ra_default、RA 间隔和实际上游路由检查。",
      ],
      flags: ["version-dependent"],
      discovery:
        "已取得 ra_lifetime 的非负值夹限及最终 WAN6 门控；负值路径按有效最长间隔计算，没有据此推广为固定缺省值。",
      impact:
        "影响客户端保留本机默认 IPv6 路由的时间；可被有效间隔夹限或 WAN6 状态覆盖，不能据此保证始终有默认路由。",
      evidence: [
        {
          source: "docs/field-help-dhcp-research.md",
          line: 49,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 ra_lifetime 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68256,
          },
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 693,
          endLine: 701,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dnsmasq 后备路径把 RA route lifetime 写死为 7200，并注释尚未转换灵活租期；不读取 ra_lifetime。",
        },
        {
          source: "etc/init.d/odhcpd",
          line: 8,
          endLine: 24,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。",
        },
        {
          source: "docs/field-help-queryport-review.md",
          line: 176,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "ra_lifetime 正值小于有效最长间隔时提升，较大值限制到 9000，0 保持为 0；负值另按最长间隔计算。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 24588,
          },
        },
        {
          source: "docs/field-help-queryport-review.md",
          line: 143,
          endLine: 161,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "最终 Router Lifetime 在 WAN6 检查返回 0 时被归零。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 25920,
          },
        },
        {
          source: "usr/sbin/wan6_link_check.sh",
          line: 6,
          endLine: 23,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "WAN6 检查根据上游专用标记、接口 up 状态及 IPv6 网关返回连通标记。",
        },
      ],
      summary: "有效期会结合 RA 最长间隔夹限；原厂 WAN6 检查还可将它归零。",
      range:
        "0 保持为 0；正值至少为有效 ra_maxinterval，且不超过 9000。WAN6 检查可最终归零。",
    },
    ra_mtu: {
      description:
        "设置 RA 中告知客户端的链路 MTU，不直接修改网卡 MTU。原厂在值为 0 时读取接口 MTU，并将实际发送值提升到至少 1280。",
      unit: "字节",
      range:
        "发送值至少 1280；0 读取接口 MTU。面板范围 1280–65535，二进制此分支未证明 65535 上限。",
      dependencies: ["需有 RA；应与接口实际 MTU 及上游路径相容。"],
      flags: ["version-dependent"],
      discovery:
        "已取得 ra_mtu 的配置解析、0 值读取接口 MTU 及发送值下限处理；没有从该片段证明 65535 上限或字段缺省值。",
      impact:
        "通告 MTU 与实际链路不符可能造成 IPv6 丢包或吞吐下降；二进制已证明下限处理，但不能从该分支推断 65535 上限。",
      evidence: [
        {
          source: "docs/field-help-dhcp-research.md",
          line: 54,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 ra_mtu 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68296,
          },
        },
        {
          source: "etc/init.d/odhcpd",
          line: 32,
          endLine: 35,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd init 直接启动服务二进制，不在脚本中配置 RA MTU。",
        },
        {
          source: "etc/init.d/odhcpd",
          line: 8,
          endLine: 24,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。",
        },
        {
          source: "docs/field-help-queryport-review.md",
          line: 174,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "ra_mtu 解析到接口字段；值为 0 时查询接口 MTU，最终发送值至少为 1280。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 24104,
          },
        },
      ],
      summary:
        "设置 RA 通告 MTU；0 取接口 MTU，发送值至少 1280，不直接改网卡。",
    },
    dns: {
      description:
        "向 IPv6 客户端提供 DNS 地址列表；odhcpd 二进制中存在同名列表表项。",
      dependencies: [
        "需有可用的 RA 或 DHCPv6 服务；不是 dnsmasq.server 的上游 DNS 设置。",
      ],
      flags: ["version-dependent"],
      discovery:
        "检索 dnsmasq/odhcpd init、lib shell、厂商脚本与反编译 Lua，未证明接口段 dns 的 odhcpd 发送分支；dnsmasq dhcp_add 虽引用 dns 变量却未读取此字段。ELF 列表项仅证明名称/类型。",
      impact:
        "会改变客户端 IPv6 DNS 选择；不可达地址可让 IPv6 网络看似连通但无法解析名称。",
      evidence: [
        {
          source: "docs/field-help-dhcp-research.md",
          line: 29,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 dns 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68096,
          },
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 728,
          endLine: 735,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dnsmasq 后备 IPv6 分支有生成 option6:dns-server 的代码，但该函数内未见 config_get dns。",
        },
        {
          source: "etc/init.d/odhcpd",
          line: 8,
          endLine: 24,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。",
        },
      ],
      summary: "接口 IPv6 DNS 列表；odhcpd 发送分支未取得可读实现。",
    },
    domain: {
      description:
        "按接口提供 IPv6 DNS 搜索域列表；与 dnsmasq 全局本地域名分开保存。",
      dependencies: ["由 odhcpd 的 IPv6 配置处理；需配合可达 DNS 服务。"],
      flags: ["version-dependent"],
      discovery:
        "检索接口 dhcp_add、odhcpd init、lib shell、厂商文本脚本和反编译 Lua，未找到 dhcp 接口段 domain 的文本读取；odhcpd 有 domain 列表表项，但搜索域发送实现未取得。",
      impact:
        "会影响客户端对短名称的补全；不匹配本地 DNS 记录时可能产生无效查询。",
      evidence: [
        {
          source: "docs/field-help-dhcp-research.md",
          line: 31,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 domain 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68112,
          },
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 1086,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dnsmasq 的 --domain 读取发生在 dnsmasq 全局段，而不是接口 dhcp_add。",
        },
        {
          source: "etc/init.d/odhcpd",
          line: 8,
          endLine: 24,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd 在 wifiapmode、lanapmode、whc_re 时直接返回；仅存在非 off/passthrough 的 WAN IPv6 模式时继续启动。",
        },
      ],
      summary: "接口 IPv6 搜索域列表；odhcpd 发送分支未取得可读实现。",
    },
    dhcp_option: {
      description:
        "为此接口添加带网络标签的 DHCP 选项，可用编号和值或 option6: 等原生语法。",
      dependencies: [
        "由 dnsmasq 实际服务此地址池；ignore 时不会进入选项生成。",
        "与 dnsmasq 全局 dhcp_option 及自动生成的 DNS/网关选项一起检查。",
      ],
      impact:
        "只针对该池匹配客户端修改 DNS、网关等配置；不正确的选项可导致已获地址但不能联网。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 577,
          endLine: 578,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "networkid 未设置时使用 interface 名称作为网络标签。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 738,
          endLine: 740,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "地址池分别调用普通与强制选项生成器。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 769,
          endLine: 791,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dhcp_option_add 优先保留原生列表，兼容旧式字符串并按标签生成选项。",
        },
      ],
    },
    dhcp_option_force: {
      description:
        "即使客户端没有请求，也发送此接口的 DHCP 选项。厂商还会自动附加型号、版本及初始化状态选项。",
      dependencies: ["仅影响 dnsmasq 处理的 DHCP；保留原生逗号与列表语法。"],
      flags: ["version-dependent"],
      impact:
        "比普通选项覆盖面更强；错误的 DNS 或网关可持续发给客户端，且原厂附加选项仍可能存在。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 525,
          endLine: 533,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dhcp_option_force_add 读取该字段并生成 --dhcp-option-force。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 535,
          endLine: 548,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商在同一函数继续加入由型号、ROM 版本、颜色及初始化状态生成的强制选项。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 738,
          endLine: 740,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "地址池调用 dhcp_option_add 强制路径及 dhcp_option_force_add。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 109,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "内置帮助说明即使客户端没有请求，也发送此 DHCP 选项。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 209463,
          },
        },
      ],
    },
  },
  host: {
    name: {
      description:
        "设置静态客户端主机名；没有 MAC/DUID 时，dnsmasq 还可用此名称作为客户端匹配条件。",
      dependencies: [
        "dns=1 且有 ip 才额外写本地 hosts 记录；全局 domain 可追加后缀。",
      ],
      impact:
        "名字影响租约识别和本地解析；没有名称、IPv4 或 hostid 的空记录会被跳过。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 366,
          endLine: 370,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "读取 name/ip/hostid；三者全空时忽略记录。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 393,
          endLine: 398,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "没有 MAC 或 DUID 时使用 name 作为客户端标识，然后清空输出名称字段。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 413,
          endLine: 420,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "有独立名称时将其追加到 --dhcp-host。",
        },
      ],
    },
    mac: {
      description:
        "用一个或多个 MAC 地址匹配同一静态租约；脚本按空白拆分后用逗号拼接。",
      dependencies: ["用于 IPv4 静态匹配；缺少 MAC/DUID 时可退回名称匹配。"],
      range: "原生 MAC 地址格式；多个地址保持列表或空白分隔形式。",
      impact:
        "匹配客户端会得到对应固定地址；重复或错误 MAC 会让目标客户端无法匹配，或导致地址冲突。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 377,
          endLine: 385,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "读取 mac；支持多个 MAC 并将它们以逗号拼接。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 418,
          endLine: 420,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "拼接后的 MAC 标识进入 --dhcp-host。",
        },
      ],
    },
    ip: {
      description:
        "给匹配客户端指定固定 IPv4 地址，或使用 dnsmasq 原生 ignore 标记忽略该客户端。",
      dependencies: [
        "需有效的 MAC、DUID 或名称匹配条件。",
        "dns=1 且 name 非空时该字符串也会进入 hosts 文件，应避免组合使用 ignore 与生成 DNS 记录。",
      ],
      impact:
        "固定地址应与对应 LAN 子网一致且不能与其他分配冲突；ignore 会让匹配客户端无法从此服务获址。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 366,
          endLine: 370,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "ip 参与静态租约是否有效的判断。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 416,
          endLine: 420,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "ip 原样进入 --dhcp-host，IPv4 路径不转换该值。",
        },
      ],
    },
    duid: {
      description:
        "设置 DHCPv6 客户端标识；dnsmasq 只在其 DHCPv6 服务路径中生成 id: 标识。",
      range: "DHCPv6 原生十六进制 DUID；保留格式。",
      dependencies: [
        "当前负责 DHCPv6 的服务必须支持相应静态租约；IPv4 dnsmasq 路径忽略 DUID。",
      ],
      impact:
        "只改 DUID 不会改变普通 DHCPv4 的 MAC 匹配；标识不对应目标客户端时固定 IPv6 分配失效。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 377,
          endLine: 390,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "duid 被读取，但仅 DNSMASQ_DHCP_VER=6 且非空时拼接 id: 标识。",
        },
      ],
    },
    hostid: {
      description:
        "设置 IPv6 固定地址的十六进制主机部分；dnsmasq 验证十六进制格式后把低 32 位放入 [::主机部分]。",
      range: "十六进制主机标识，可带 0x；dnsmasq 转换器仅保留低 32 位。",
      dependencies: [
        "dnsmasq 接管 DHCPv6 时才在此代码中输出；odhcpd 需核对其同名静态租约支持。",
      ],
      impact:
        "不会设置完整 IPv6 前缀；实际地址还依赖接口可用前缀及 DHCPv6 服务。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 368,
          endLine: 370,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "hostid 参与静态记录是否有效的判断。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 400,
          endLine: 402,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "hostid 非空时调用 hex_to_hostid 转换。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 416,
          endLine: 420,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "仅 IPv6 dnsmasq 路径在 --dhcp-host 地址中追加 [::hostid]。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 93,
          endLine: 107,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "hex_to_hostid 去除可选 0x 前缀、拒绝非十六进制字符，再将低 32 位分成两个 16 位 IPv6 分组。",
        },
      ],
    },
    leasetime: {
      description: "为这个静态客户端单独设置租约时长；空值不会输出专用租期。",
      unit: "dnsmasq 时间字符串，例如 12h 或 infinite",
      dependencies: [
        "无专用租期时应检查接口地址池 leasetime，不把示例 12h 当作此字段默认。",
      ],
      impact:
        "可让固定设备使用不同于地址池的租期；短租期增加续租，长租期延后配置更新。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 408,
          endLine: 414,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "读取 host.leasetime，无显式回退；只在非空时加入 nametime。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 418,
          endLine: 420,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "nametime 追加到此条 --dhcp-host。",
        },
      ],
    },
    dns: {
      description: "为含 IPv4 地址与名称的静态租约额外生成本地 hosts 记录。",
      defaultValue: "0（不额外生成）",
      dependencies: [
        "需要 ip 和 name 非空；全局 domain 会追加后缀。",
        "由 dnsmasq 的额外 hosts 文件加载，不等于发送 IPv6 DNS 地址。",
      ],
      impact:
        "使静态客户端名称可在取得租约前供本地 DNS 使用；缺少 ip 或 name 时即使启用也不生成。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 372,
          endLine: 375,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dns 缺省为 0；只有 dns=1 且 ip、name 均非空时写入 HOSTFILE_TMP，并可追加 DOMAIN。",
        },
      ],
    },
    broadcast: {
      description:
        "给此客户端附加 needs-broadcast 标签，请求使用广播 DHCP 应答。",
      defaultValue: "0（不加广播标签）",
      flags: ["version-dependent"],
      summary: "原厂只追加 needs-broadcast 标签；显式广播开关的生成行已注释。",
      discovery:
        "确认静态租约广播标签的生成，但检索 dnsmasq init、厂商脚本与反编译 Lua，未找到仍执行的 --dhcp-broadcast=tag:needs-broadcast 生成；其他配置文件是否定义该标签未验证。",
      impact:
        "可帮助部分收不到单播应答的客户端；但本固件直接启用该标签的 dhcp-broadcast 行已被注释，不能保证单改此项会触发广播。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 408,
          endLine: 413,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "broadcast 缺省为 0；非零时加入 set:needs-broadcast 标签。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 1207,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "生成 --dhcp-broadcast=tag:needs-broadcast 的行被注释。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 114,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dnsmasq 内置帮助说明可按标签强制广播应答；原厂脚本是否生成相应全局选项需独立核对。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 208935,
          },
        },
      ],
    },
    tag: {
      description:
        "为匹配客户端附加一个或多个 dnsmasq set: 标签，以关联选项或策略。",
      dependencies: ["应与已有标签匹配规则配套；保留每个标签边界。"],
      impact: "标签本身不分配 IP；关联的选项规则会改变该客户端的 DHCP 参数。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 379,
          endLine: 405,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "读取 tag；按空白拆分为逗号分隔标签。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 413,
          endLine: 420,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "标签带 set: 前缀进入 --dhcp-host。",
        },
      ],
    },
  },
  domain: {
    name: {
      description:
        "给本地 DNS 记录指定一个或多个主机名，脚本按空白拆分后写入生成的 hosts 文件。",
      dependencies: [
        "需要同一记录 ip 非空；由 dnsmasq 的额外 hosts 文件加载。",
      ],
      impact:
        "这些名称会由本机直接解析；错误名称可能覆盖客户端期望的公开域名。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 800,
          endLine: 810,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "name 必须非空；循环组合名称后，与 ip 一起写入 HOSTFILE_TMP。",
        },
      ],
    },
    ip: {
      description:
        "指定本地 hosts 记录答复的 IP 地址；该路径直接写入 hosts，不进行 CIDR 展开。",
      range: "hosts 地址字段：单个 IPv4 或 IPv6 地址，不应填写 CIDR 网段。",
      dependencies: ["同一记录 name 必须非空。"],
      summary: "填写单个 IPv4 或 IPv6 地址；原厂直接写 hosts，不展开 CIDR。",
      impact:
        "影响该名称的本地解析地址；填入网段或不合法地址可能让记录无法解析。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 803,
          endLine: 810,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "ip 必须非空，并与记录名称直接写入 HOSTFILE_TMP。",
        },
      ],
    },
  },
  odhcpd: {
    maindhcp: {
      description:
        "通知 dnsmasq 是否把全部 DHCP 地址服务交给 odhcpd。原厂安装的是 ipv6only 变体，不可把此开关视为已验证的 DHCPv4 接管能力。",
      defaultValue: "0（dnsmasq 判断的回退）",
      dependencies: ["受 odhcpd init 是否启用及 dnsmasq DHCPv6 编译能力影响。"],
      flags: ["version-dependent"],
      summary:
        "会让 dnsmasq 退出 DHCP；原厂 ipv6only 变体的 IPv4 接管能力未证明。",
      impact:
        "启用后 dnsmasq 可停止提供地址池；若 odhcpd 没有 DHCPv4 能力，IPv4 客户端会失去地址分配。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 1005,
          endLine: 1019,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd 存在时，maindhcp 缺省为 0；若其大于 0 且未优先走 dnsmasq IPv6 后备分支，则 DNSMASQ_DHCP_VER=0。",
        },
        {
          source: "usr/lib/opkg/info/odhcpd-ipv6only.control",
          line: 1,
          endLine: 16,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "安装包为 odhcpd-ipv6only，说明列出 RA、DHCPv6、前缀委派及 RA/DHCPv6/NDP 中继服务。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 63,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 maindhcp 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68408,
          },
        },
      ],
    },
    leasefile: {
      description:
        "按 odhcpd 字段语义指定 IPv6 租约/主机记录文件；不是 dnsmasq 的 DHCPv4 租约路径。",
      dependencies: [
        "路径及更新用途由 odhcpd 二进制处理；leasetrigger 可关联后续更新。",
      ],
      flags: ["version-dependent"],
      summary:
        "odhcpd 的文件路径；原厂二进制有此项，格式与默认路径未取得证明。",
      discovery:
        "检索 odhcpd init/update、lib shell、厂商文本脚本及反编译 Lua，未发现 odhcpd.leasefile 的文本读取；ELF 字符串表项仅证明该配置名及类型，不证明采样配置即默认。",
      impact:
        "错误路径可能使 IPv6 租约记录或下游 DNS 更新失效；可读脚本没有证明文件格式及缺省路径。",
      evidence: [
        {
          source: "docs/field-help-dhcp-research.md",
          line: 64,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 leasefile 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68416,
          },
        },
        {
          source: "usr/sbin/odhcpd-update",
          line: 1,
          endLine: 6,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd-update 的注释说明租约更新通知让 dnsmasq 重读 hosts，并向 dnsmasq 发送信号。",
        },
      ],
    },
    leasetrigger: {
      description:
        "按 odhcpd 字段语义指定租约变更后的通知路径；原厂 odhcpd-update 脚本会通知 dnsmasq 重读 hosts。",
      dependencies: [
        "通知文件需存在并能运行；不能从 update 脚本本身推断它一定是缺省 trigger。",
      ],
      flags: ["version-dependent"],
      summary:
        "IPv6 租约通知路径；已找到 update 脚本，实际 trigger 调用分支未取得。",
      discovery:
        "检索 odhcpd init/update、lib shell、厂商文本脚本与反编译 Lua，未找到 leasetrigger 文本消费者；odhcpd ELF 有字符串类型表项，但本次未取得调用时机、参数或默认路径。",
      impact:
        "路径失效可能使租约变化不能及时反映到本地 DNS；更改已有执行路径前应核对该通知链。",
      evidence: [
        {
          source: "docs/field-help-dhcp-research.md",
          line: 65,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 leasetrigger 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68424,
          },
        },
        {
          source: "usr/sbin/odhcpd-update",
          line: 1,
          endLine: 6,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd-update 导入 procd 函数并向 dnsmasq 发送重读 hosts 的信号。",
        },
      ],
    },
    loglevel: {
      description:
        "按 odhcpd 字段语义设置 syslog 级别阈值；面板编号 0 最紧急，7 最详细。",
      range: "0–7（syslog/schema 级别；内部校验未取得）",
      flags: ["version-dependent"],
      summary: "syslog 级别 0–7；原厂二进制含此项，默认值与过滤分支未验证。",
      discovery:
        "检索 odhcpd init、lib shell、厂商文本脚本及反编译 Lua，未找到 odhcpd.loglevel 文本读取；只确认 odhcpd 的同名整数表项。",
      impact:
        "较详细日志会增加资源占用并包含客户端信息；实际过滤及默认级别尚未在可读固件实现中证明。",
      evidence: [
        {
          source: "docs/field-help-dhcp-research.md",
          line: 66,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原始 odhcpd ELF 表含 loglevel 名称及原始类型/数值；此表本身不证明缺省值或运行行为。",
          artifact: {
            source: "usr/sbin/odhcpd",
            sha256:
              "afdbd367187bfa35d3f6868c313744e100bd392d81d7fd3b76d021252830f798",
            offset: 68432,
          },
        },
        {
          source: "etc/init.d/odhcpd",
          line: 32,
          endLine: 35,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "odhcpd init 未传入日志级别参数，直接启动二进制。",
        },
      ],
    },
  },
  cname: {
    cname: {
      description: "创建本地 DNS 别名，脚本将其与 target 拼成 --cname。",
      dependencies: ["target 必须是 DHCP 或本地 hosts 已知名称。"],
      impact:
        "匹配名称会转到本机可解析的目标；此功能不是把任意外部域名作为 CNAME 目标。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 854,
          endLine: 860,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "cname 与 target 都必须非空，输出 --cname=cname,target。",
        },
        {
          source: "etc/dnsmasq.conf",
          line: 33,
          endLine: 37,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 dnsmasq.conf 注释说明 CNAME 仅适用于 DHCP 或 /etc/hosts 中的本地目标。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 110,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dnsmasq 内置帮助说明是 LOCAL DNS 名称的别名。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 213714,
          },
        },
      ],
    },
    target: {
      description:
        "设置别名指向的本地主机名；原厂注释限定目标来自 DHCP 或 /etc/hosts。",
      dependencies: ["同一条记录 cname 非空，目标已由本机 DHCP/hosts 知道。"],
      impact:
        "不存在的本地目标不会成为正常可用的别名；仅填外部 DNS 域名不符合此固件的说明。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 857,
          endLine: 860,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "target 非空时与 cname 一起输出 --cname。",
        },
        {
          source: "etc/dnsmasq.conf",
          line: 33,
          endLine: 37,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dnsmasq.conf 说明本地别名仅用于 DHCP 或 hosts 中的目标。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 110,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dnsmasq 内置帮助说明是 LOCAL DNS 名称的别名。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 213714,
          },
        },
      ],
    },
    ttl: {
      description:
        "既有字段表示 DNS 别名缓存有效期；本固件 cname 生成器仅输出别名和目标，不读取 ttl。",
      unit: "秒（既有 schema 语义；此路径未消费）",
      flags: ["version-dependent"],
      summary: "原厂 cname 生成器没有读取 ttl；改值不能确认改变缓存时间。",
      discovery:
        "已检索 1.0.43 完整 dhcp_cname_add、dnsmasq 启动选项、lib shell、厂商脚本与反编译 Lua，未找到 cname 段 ttl 读取；父级保存的 1.0.64 公共脚本同函数也未读取 ttl。全局 local_ttl 是不同字段。",
      impact:
        "只改此项不能据当前静态代码确认缓存时间变化；不能把界面数值当作已生效的 DNS TTL。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 850,
          endLine: 861,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "完整 dhcp_cname_add 只读取 cname/target 并输出两段 --cname，未读取或追加 ttl。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 1119,
          endLine: 1123,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dnsmasq 全局 local_ttl/max_ttl 等另有读取，不能证明 cname.ttl 的作用。",
        },
        {
          source: "live-inspection/field-help-live-1.0.64/etc/init.d/dnsmasq",
          line: 850,
          endLine: 861,
          firmware: "Xiaomi RN02 1.0.64",
          fact: "已保存的 1.0.64 公共脚本仍仅输出 cname 与 target，没有读取或追加 ttl。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 111,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dnsmasq 二进制的原生 CNAME 语法支持可选 ttl；这不等于厂商 UCI 生成器会读取 cname.ttl。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 213689,
          },
        },
      ],
    },
  },
  boot: {
    filename: {
      description:
        "设置 DHCP 网络启动的文件名；缺少文件名时整个 boot 记录被忽略。",
      dependencies: [
        "客户端需要网络启动支持；此项不自动启用或部署 TFTP 文件。",
        "serveraddress 非空时还必须填写 servername。",
      ],
      impact: "会改变匹配客户端请求的启动文件；错误名称可使 PXE/网络启动失败。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 510,
          endLine: 518,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "filename 必須非空；随后进入 --dhcp-boot 的文件名位置。",
        },
      ],
    },
    serveraddress: {
      description:
        "指定网络启动服务器 IPv4 地址；原厂生成器同时要求 servername 非空。",
      range: "单个启动服务器地址，不是网段。",
      dependencies: ["filename 与 servername 必须非空；启动文件服务另行提供。"],
      summary: "填写启动服务器单个地址；有地址时原厂还要求 servername。",
      impact:
        "错误地址让客户端无法获取启动文件；这条路径原样传入地址，不支持把 CIDR 当成服务器地址。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 513,
          endLine: 518,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "serveraddress 直接放入 --dhcp-boot；若有地址却没有名称则跳过整条记录。",
        },
      ],
    },
    servername: {
      description:
        "设置网络启动服务器名称，位于 --dhcp-boot 文件名之后。原厂脚本要求有 serveraddress 时也有名称。",
      dependencies: ["filename 非空；配合 serveraddress 检查。"],
      impact:
        "名称与地址组合不完整时整个启动记录会被跳过；不是普通 DNS 上游主机名。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 513,
          endLine: 518,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "读取 servername/serveraddress；地址非空但名称为空时提前返回，否则按位置追加。",
        },
      ],
    },
    networkid: {
      description:
        "用 dnsmasq 网络/客户端标签限定启动配置；非空时生成 net:标签 前缀。",
      dependencies: [
        "标签应与接口或客户端标签规则一致。",
        "filename 缺失时不生成任何启动规则。",
      ],
      impact:
        "让不同客户端获得不同启动文件；没有对应匹配标签时规则可能不命中。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 508,
          endLine: 518,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "读取 networkid，并在 --dhcp-boot 前追加 net:networkid 条件。",
        },
        {
          source: "etc/init.d/dnsmasq",
          line: 520,
          endLine: 522,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "同一 networkid 也传给该 boot 记录的 dhcp_option 生成器。",
        },
      ],
    },
  },
  relay: {
    interface: {
      description:
        "可选的 DHCP 中继逻辑接口；填写后需能转换为实际设备，否则此中继条目不会生成。",
      dependencies: [
        "local_addr 与 server_addr 必须非空；dnsmasq boot 阶段跳过 relay 生成。",
      ],
      impact: "会把中继限定到指定设备；错误逻辑网络名使整条中继失效。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 894,
          endLine: 899,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "interface 为空时不加接口；非空时需 network_get_device 成功，再追加实际设备。",
        },
      ],
    },
    local_addr: {
      description: "设置 DHCP 中继接收客户端请求的本地地址；此地址是必填项。",
      range: "单个本地地址，保留 dnsmasq 中继地址格式；不是 CIDR 网段。",
      dependencies: ["与 server_addr 和可选 interface 配套。"],
      summary: "填写中继本地单个地址；原厂直接传入 --dhcp-relay，不展开 CIDR。",
      impact:
        "本地地址不在正确网络时请求可能无法进入中继；脚本不对 CIDR 进行转换。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 888,
          endLine: 899,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "local_addr 必须非空，直接作为 --dhcp-relay 第一个参数。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 112,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dnsmasq 内置中继语法为 local-addr,server[,iface]，未给出 CIDR 网段形式。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 213618,
          },
        },
      ],
    },
    server_addr: {
      description: "设置接收中继请求的 DHCP 服务器地址；此地址是必填项。",
      range: "单个 DHCP 服务器地址，不是 CIDR 网段。",
      dependencies: ["必须从中继设备可达；local_addr 必须非空。"],
      summary: "填写 DHCP 服务器单个地址；路由和防火墙可达性需另行满足。",
      impact:
        "服务器不可达或地址错误会使客户端收不到租约；不会建立到服务器的路由。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 891,
          endLine: 899,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "server_addr 必须非空，直接作为 --dhcp-relay 第二个参数。",
        },
        {
          source: "docs/field-help-dhcp-research.md",
          line: 112,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dnsmasq 内置中继语法为 local-addr,server[,iface]。",
          artifact: {
            source: "usr/sbin/dnsmasq",
            sha256:
              "f251e489ca2cc138cea166bbb24446ebf0d49aa1916ab136094982bbe283a47c",
            offset: 213618,
          },
        },
      ],
    },
  },
  srvhost: {
    srv: {
      description: "指定 DNS SRV 的完整服务名，例如 _sip._tcp.example.test。",
      dependencies: ["target 与 port 必须非空，否则整条记录跳过。"],
      impact:
        "决定客户端查询哪项服务时得到这条记录；错误前缀或域名会使服务发现失败。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 816,
          endLine: 830,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "srv 必須非空，并成为 --srv-host 第一段。",
        },
        {
          source: "etc/dnsmasq.conf",
          line: 1,
          endLine: 4,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dnsmasq.conf 说明 srv-host 顺序为 name,target,port,priority,weight。",
        },
      ],
    },
    target: {
      description: "指定 SRV 服务目标主机名，不直接填写其 IP 地址映射。",
      dependencies: ["需有 srv 与 port；目标应可通过本地或上游 DNS 解析。"],
      impact: "客户端会进一步解析该主机名；目标无法解析时服务发现仍不能连接。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 819,
          endLine: 830,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "target 必須非空，并位于 --srv-host 的第二段。",
        },
      ],
    },
    port: {
      description: "指定 SRV 目标服务监听端口，不改变 dnsmasq 自身 DNS 端口。",
      unit: "端口号",
      range: "1–65535（面板范围；生成器直接传值）",
      dependencies: ["与 srv 和 target 一起构成有效 SRV 条目。"],
      impact: "客户端将连接此目标端口；端口错误会使服务发现结果无法使用。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 822,
          endLine: 830,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "port 必須非空，并位于 --srv-host 的第三段。",
        },
      ],
    },
    class: {
      description:
        "此处字段名 class 实际被用作 SRV 优先级；较小值代表更优先目标，不是 DNS 记录类别。",
      range: "0–65535（SRV 优先级）",
      dependencies: ["weight 只在 class 非空时输出。"],
      impact:
        "有多条同服务记录时影响目标选择顺序；空值时脚本也不输出后面的 weight。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 825,
          endLine: 830,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "class 位于 --srv-host 第四段；仅 class 非空才追加 weight。",
        },
        {
          source: "etc/dnsmasq.conf",
          line: 4,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂说明第四段是 priority。",
        },
      ],
    },
    weight: {
      description:
        "设置同 SRV 优先级目标的相对权重；原厂生成器仅在 class 非空时输出。",
      range: "0–65535（SRV 权重）",
      dependencies: ["需同时填写 class；仅影响同优先级目标的选择。"],
      impact:
        "有多个同优先级目标时可分配客户端选择比例；单条记录或不同优先级时不是固定流量百分比。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 825,
          endLine: 830,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "weight 位于 --srv-host 第五段，受 class 非空条件约束。",
        },
        {
          source: "etc/dnsmasq.conf",
          line: 4,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂说明第五段是 weight。",
        },
      ],
    },
  },
  mxhost: {
    domain: {
      description: "指定本地 MX 记录所属邮件域名。",
      dependencies: ["同一条记录 relay 必須非空。"],
      impact:
        "对此域的 MX 查询会得到本机配置的邮件服务器；错误域名会影响邮件投递发现。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 837,
          endLine: 847,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "domain 必須非空，并成为 --mx-host 第一段。",
        },
      ],
    },
    relay: {
      description:
        "指定 MX 记录指向的邮件服务器主机名；不是 DHCP relay 或 IP 中继地址。",
      dependencies: ["domain 必須非空；邮件服务器目标应可解析。"],
      impact:
        "邮件客户端或服务器需能解析并连接这个主机；错误目标会影响邮件投递。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 840,
          endLine: 847,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "relay 必須非空，并成为 --mx-host 第二段。",
        },
      ],
    },
    pref: {
      description: "设置 MX 邮件服务器优先级；较小值优先。",
      defaultValue: "0",
      range: "0–65535（MX 优先级）",
      dependencies: ["需 domain 与 relay 非空。"],
      impact: "同一邮件域配置多个目标时决定首选服务器顺序，不等于发送权重。",
      evidence: [
        {
          source: "etc/init.d/dnsmasq",
          line: 843,
          endLine: 847,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "pref 明确回退为 0，并成为 --mx-host 第三段。",
        },
      ],
    },
  },
};
