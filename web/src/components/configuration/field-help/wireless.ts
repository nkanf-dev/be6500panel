import type { ModuleFieldHelp } from "./types";

// Static use tracing: Xiaomi RN02 1.0.43. Defaults below are source fallbacks,
// not sampled settings. The public 1.0.64 comparison set has no Wi-Fi consumers.
export const wirelessFieldHelp: ModuleFieldHelp = {
  "wifi-device": {
    type: {
      description:
        "选择负责启动这块射频的驱动。原厂按此值调用 scan_<驱动> 与 enable/disable_<驱动>；它不是无线网卡名称。",
      dependencies: ["应与 /lib/wifi 中的厂商驱动脚本匹配。"],
      flags: ["hardware-dependent", "version-dependent"],
      impact: "改错会找不到驱动处理函数，使该射频及其无线网络无法启动。",
      evidence: [
        {
          source: "sbin/wifi",
          line: 218,
          endLine: 223,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "读取 wifi-device.type 后按驱动名分派扫描与启停；无对应脚本时报告不支持。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 1560,
          endLine: 1561,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "兼容处理会把旧 qcawifi 驱动类型改为 qcawificfg80211。",
        },
      ],
    },
    path: {
      description:
        "按硬件路径定位无线射频。原厂 QCA 实际读取 phy 与 macaddr，尚未找到 wifi-device.path 的读取；填写路径不能保证改变射频绑定。",
      dependencies: ["已验证的 QCA 定位入口是 phy 或 macaddr。"],
      flags: ["hardware-dependent", "version-dependent"],
      discovery:
        "已检索 sbin/wifi、lib/wifi、lib/netifd、初始化脚本及反编译 XQWifiUtil；未发现读取 wifi-device 的 path 选项。出现的 path 是程序或固件文件路径，不能证明此字段生效。",
      impact:
        "原厂射频绑定是否受 path 改动影响尚未确认；已验证的定位过程使用 phy/macaddr。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 613,
          endLine: 624,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "射频查找读取 macaddr 与 phy，并用 /sys/class/net/wifi* 的地址匹配。",
        },
      ],
      summary:
        "通用硬件路径字段；原厂 QCA 定位读取 phy/macaddr，尚未找到 path 消费。",
    },
    phy: {
      description:
        "指定厂商物理射频接口标识。QCA 脚本要求它对应 /sys/class/net 中的设备；未填写时可通过 macaddr 查找 wifi*。",
      dependencies: [
        "与 macaddr、真实 QCA 射频设备相符；厂商 wifi* 标识不等于通用 phy0 示例。",
      ],
      flags: ["generated", "hardware-dependent"],
      impact: "标识不存在会使射频查找失败，随后该射频的接口无法正常创建。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 613,
          endLine: 626,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "phy 为空且有 macaddr 时按系统网卡地址匹配 wifi*；目标不存在时返回失败。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 7836,
          endLine: 7841,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "创建 VAP 时将 phy 作为 wlandev，并从该网卡的 phy80211/name 获取内核 wiphy。",
        },
      ],
      summary:
        "原厂使用 /sys/class/net 中的射频标识；通用 phy0 示例不保证适用于 QCA。",
    },
    macaddr: {
      description:
        "用于按硬件 MAC 地址查找物理射频。此处不是为每个 SSID 指定 BSSID；缺失时脚本从已找到的 phy 网卡读取地址。",
      dependencies: ["主要在 phy 未指定时参与定位。"],
      flags: ["generated", "hardware-dependent"],
      impact:
        "改成不匹配的地址可能找不到射频；已有 phy 时该字段并不直接执行网卡 MAC 修改。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 613,
          endLine: 629,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "macaddr 转为小写后用于匹配 /sys/class/net/wifi* 地址；缺失时由 phy 的 address 补入。",
        },
      ],
      summary: "按硬件地址定位射频；不是设置此射频下所有 SSID 的 BSSID。",
    },
    band: {
      description:
        "通用模型使用 2g/5g/6g；原厂 QCA 脚本却读取数值 band，并把 3 用于 6 GHz 判断。两种表示法不能视为可互换。",
      defaultValue: "0（QCA 脚本缺省值；不代表固定频段）",
      range: "取值编码由驱动决定；已证实厂商 3 表示 6 GHz 检查分支。",
      dependencies: [
        "关联 hwmode、channel 和真实射频能力；不要由下拉选项推断硬件有 6 GHz。",
      ],
      flags: ["hardware-dependent", "version-dependent"],
      impact:
        "换成通用字符串可能破坏厂商数值比较，影响信道和加密检查；这不是为 RN02 增加新射频的方法。",
      evidence: [
        {
          source: "lib/netifd/netifd-wireless.sh",
          line: 71,
          endLine: 79,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "通用 netifd 将 2g、5g、6g、60g 映射为 hwmode。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 7481,
          endLine: 7486,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商 VAP 初始化读取 band，缺失时使用 0。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 7775,
          endLine: 7781,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "TKIP 分支把 band=3 识别为 6 GHz，并在未强制允许时跳过。",
        },
      ],
      summary:
        "通用频段字符串与原厂数值 band 不可视为可互换；3 用于厂商 6 GHz 判断。",
    },
    hwmode: {
      description:
        "指定射频协议族，不仅是旧 11b/11g/11a。厂商还按 11ac、11axg/11axa、11beg/11bea 与 htmode 组合选择实际模式。",
      defaultValue: "auto（QCA 读取缺省）",
      dependencies: [
        "与 htmode、厂商 ax 开关、硬件 hwmodes 相配；保留现有 11ax/11be 厂商值。",
      ],
      flags: ["hardware-dependent", "version-dependent"],
      impact:
        "改动会改变 VAP 的协议模式与客户端兼容性；ax=0 时厂商会覆盖为 11ng 或 11ac。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 7890,
          endLine: 7906,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "hwmode 缺失用 auto；ax=0 时按硬件频段改为 11ng/11ac。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 7926,
          endLine: 7945,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "hwmode 与 htmode 联合映射到 HT/VHT/HE/EHT 驱动模式。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 7986,
          endLine: 8008,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "11beg/11bea 分支识别 HT 与 EHT 宽度值。",
        },
      ],
      summary:
        "原厂仍用 hwmode 选择 HT/VHT/HE/EHT 协议族；保留现有厂商 11ax/11be 值。",
    },
    channel: {
      description:
        "选择无线射频的主工作信道。原厂 QCA 将 auto/AUTO 转成自动选择值 0；信道 165 会强制采用 HT20，不能维持宽频模式。",
      defaultValue: "0 / auto（脚本缺省与自动信道表示）",
      unit: "信道编号",
      range: "auto、AUTO、0 或硬件/监管域允许的信道；没有通用连续范围。",
      dependencies: [
        "受 country、band、hwmode、厂商 bw 与 htmode 共同影响。",
        "MLO 接口更新另行协调伙伴链路；TWT 使用独立 twt_responder，不由信道值启用。",
      ],
      flags: ["hardware-dependent"],
      impact:
        "厂商更新逻辑把信道改变列为重启该射频全部 VAP 的条件，已连接客户端可能需要重连。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 7483,
          endLine: 7486,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "channel 缺失用 0，auto/AUTO 归一化为 0。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 7598,
          endLine: 7600,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "信道 165 强制 htmode=HT20。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 5944,
          endLine: 5956,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "channel、ax 或 bw 改变会设置 restart_all。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 6436,
          endLine: 6461,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "信道、认证及 twt_responder 等变化进入同一接口更新流程；其中有接口下线与伙伴 MLO 链路协调。",
        },
      ],
    },
    htmode: {
      description:
        "请求无线信道宽度与扩展方向。原厂按厂商 bw、信道和硬件最大宽度重算；例如 HT20 表示 20 MHz 请求，保存值不保证直接生效。",
      defaultValue: "auto（读取缺省；之后可能被 bw/硬件重算）",
      unit: "模式中宽度数字为 MHz",
      range:
        "由 hwmode 和驱动分支决定；不要假定所有 VHT/HE/EHT 下拉值都适用于原厂。",
      dependencies: [
        "关联厂商 bw、ax、channel、standby_htmode 与硬件最大宽度。",
        "MLO 绑定在 wifi-iface.mld；TWT 为独立 twt_responder，HE/EHT 宽度本身不等于开启 TWT。",
      ],
      flags: ["hardware-dependent", "version-dependent"],
      impact:
        "有效宽度会影响无线速率与占用频谱；厂商 bw 改动会触发全部 VAP 更新，不能保证只改 htmode 就采用所选宽度。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 7481,
          endLine: 7486,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "QCA 初始化读取 htmode，缺失用 auto。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 7515,
          endLine: 7538,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "读取厂商 bw，并据频段和信道重算 htmode。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 7575,
          endLine: 7593,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "5 GHz 宽度分支使用 HT80/HT160；自动分支可读取 5g_maxchwidth。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 7986,
          endLine: 8019,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "EHT 带宽取值与 11beg/11bea 联合映射，包含 EHT320。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 7839,
          endLine: 7849,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "MLO 接口创建另由 mld 关联选择 mld_iface 或 mld_addr。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 9785,
          endLine: 9786,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "TWT 接口响应由独立 twt_responder 读取并下发。",
        },
      ],
      summary: "原厂会按 bw、信道和硬件能力重算宽度；保存值不保证直接生效。",
    },
    country: {
      description:
        "设置监管国家/地区。QCA 支持国家代码或数值监管标识：数字开头走 setCountryID，其余走 setCountry。",
      defaultValue:
        "未填写且存在启用 AP 类接口时下发 156；其他模式此函数不改国家。",
      range: "驱动认可的国家代码或数值监管标识。",
      dependencies: ["与 VAP 的 mode、channel 和厂商功率控制关联。"],
      flags: ["hardware-dependent"],
      impact:
        "会改变射频监管配置；信道与功率必须符合设备及当地规则，修改不保证驱动接受。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 4803,
          endLine: 4826,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "country 为空时调用 set_default_country；非空按数字或代码选择驱动命令。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 1104,
          endLine: 1112,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "缺省函数跳过停用 VAP；有 AP 类接口时实际下发 setCountryID 156。",
        },
      ],
      summary: "原厂支持国家代码或数值监管标识；数字开头走 setCountryID。",
    },
    txpower: {
      description:
        "射频功率请求，通用单位为 dBm。原厂虽读取它，但启动时主要按厂商 txpwr 档位及 misc 中的最大功率重算并下发。",
      unit: "dBm（厂商另有半 dBm 编码）",
      range:
        "实际范围由驱动、监管域及厂商最大功率配置决定；未证实固定 0–40 范围。",
      dependencies: [
        "主要关联 txpwr、misc.wireless.if_2g_maxpower/if_5g_maxpower、country；接口须已启动。",
      ],
      flags: ["hardware-dependent", "version-dependent"],
      impact:
        "不能保证直接编辑 txpower 就得到该功率；实际输出还受射频、监管域和厂商功率档位影响。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 4837,
          endLine: 4841,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "射频初始化读取 txpower，没有在该读取处指定缺省值。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 9694,
          endLine: 9700,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商 txpwr 的 mid/min/其他分支分别取最大功率减 1、减 3 或最大功率。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 9703,
          endLine: 9709,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "半 dBm 编码另行转换；CN/156 分支才在此处通过 iwconfig 下发计算结果。",
        },
      ],
      summary:
        "功率以 dBm 表示；原厂主要按 txpwr 档位及最大功率重算，不保证直接采用此值。",
    },
    disabled: {
      description:
        "停用整个物理射频，不只是其中一个 SSID。wifi 入口见到 1 时切换为 disable 操作。",
      defaultValue: "0（厂商更新比较缺省）",
      dependencies: [
        "覆盖其下 wifi-iface 是否启用；MLO 的可用链路也依赖关联射频。",
      ],
      impact:
        "该射频上的全部无线接口可能下线；通过此射频访问管理面板的连接也可能断开。",
      evidence: [
        {
          source: "sbin/wifi",
          line: 212,
          endLine: 221,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "遍历 wifi-device 时读取 disabled=1 并改为 disable，然后分派驱动启停。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 5991,
          endLine: 6002,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "射频 disabled 缺失按 0 比较，状态改变分别调用 disable 或 enable。",
        },
      ],
    },
    legacy_rates: {
      description:
        "通用无线配置中表示允许旧式低速率。此固件未找到 legacy_rates 的配置读取，不能据此确认 RN02 会改变基础速率。",
      dependencies: [
        "旧速率行为通常关联 hwmode 与基础速率，但原厂映射未证实。",
      ],
      flags: ["legacy", "version-dependent"],
      discovery:
        "已检索 sbin/wifi、lib/wifi、lib/netifd、初始化脚本、全部可读 Lua/反编译 Lua；未出现 legacy_rates。已找到 basic_rate/basic_rates 生成路径，但不能证明它与此字段相连。",
      impact: "尚不能确认修改会改变原厂速率集或旧客户端兼容性。",
      evidence: [
        {
          source: "lib/wifi/hostapd.sh",
          line: 108,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "通用 hostapd 设备声明的是 basic_rate 数组。",
        },
        {
          source: "lib/wifi/hostapd.sh",
          line: 138,
          endLine: 144,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "basic_rate 列表经转换后写为 hostapd basic_rates。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 9539,
          endLine: 9540,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商另读取 dis_legacy 并下发同名命令；不是 legacy_rates 选项。",
        },
      ],
      summary: "通用旧速率开关；尚未找到原厂读取 legacy_rates 的路径。",
    },
    noscan: {
      description:
        "通用字段用于跳过 40 MHz 共存扫描。netifd 公共设备声明包含 noscan，但原厂 QCA 的共存控制读取 disablecoext。",
      dependencies: [
        "与 2.4 GHz 的 bw/channel 及厂商 disablecoext 路径相关；未证实 noscan 到该命令的转换。",
      ],
      flags: ["hardware-dependent", "version-dependent"],
      discovery:
        "已检索 sbin/wifi、lib/wifi、lib/netifd 和反编译 Lua；仅发现 netifd 的 noscan 声明，未找到读取 noscan 后影响 QCA 共存扫描的消费者。",
      impact:
        "此字段是否影响 RN02 共存扫描尚未确认；已验证的厂商入口是另一个配置项 disablecoext。",
      evidence: [
        {
          source: "lib/netifd/netifd-wireless.sh",
          line: 365,
          endLine: 367,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "公共设备配置声明 noscan 字符串字段。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 8126,
          endLine: 8130,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "QCA 从 wifi-iface 读取 disablecoext 并下发；强制 11NGHT40 时另下发 1。",
        },
      ],
      summary:
        "公共模型声明此字段；原厂共存命令读取 disablecoext，未证实 noscan 映射。",
    },
    beacon_int: {
      description:
        "通用设备级信标间隔。公共 hostapd 生成器会写入 beacon_int，但原厂主要 VAP 生成路径读取的是 wifi-iface.bintval。",
      unit: "TU（802.11 时间单位，1 TU = 1024 微秒）",
      dependencies: [
        "信标间隔与 DTIM 周期共同影响组播通知节奏；厂商入口是 iface.bintval。",
      ],
      flags: ["version-dependent"],
      discovery:
        "已追踪 lib/wifi/hostapd.sh、qcawificfg80211.sh、wifi 入口和初始化脚本；找到公共 beacon_int 生成器，但未找到 QCA 主 VAP 路径读取 wifi-device.beacon_int；已验证该路径读 bintval。",
      impact:
        "此设备级字段能否控制原厂 VAP 尚未确认；不能把通用生成器的支持等同于 RN02 主启动路径已接入。",
      evidence: [
        {
          source: "lib/wifi/hostapd.sh",
          line: 112,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "公共设备配置声明 beacon_int 整数。",
        },
        {
          source: "lib/wifi/hostapd.sh",
          line: 123,
          endLine: 144,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "公共生成器读取 beacon_int，非空时写入同名 hostapd 配置。",
        },
        {
          source: "lib/wifi/hostapd.sh",
          line: 1574,
          endLine: 1581,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 VAP 生成器从 wifi-iface 读取 bintval。",
        },
        {
          source: "lib/wifi/hostapd.sh",
          line: 2292,
          endLine: 2295,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "VAP 路径把 bintval 写为 beacon_int；同处另读取 dtim_period。",
        },
      ],
      summary: "通用设备信标间隔；原厂主 VAP 路径读取的是 iface.bintval。",
    },
    distance: {
      description:
        "通用长距离链路参数，通常用于 ACK 超时计算。此 QCA 脚本读取 distance 后明确输出“不支持此驱动”。",
      unit: "米（通用字段语义）",
      dependencies: ["行为取决于驱动；此证据仅适用于原厂 QCA 路径。"],
      flags: ["hardware-dependent", "legacy"],
      impact:
        "本版本这条驱动路径没有把 distance 下发为 ACK 超时；填入距离不会据此完成链路优化。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 4970,
          endLine: 4977,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "读取 distance；非空时输出 distance option not supported on this driver。",
        },
      ],
      summary: "原厂 QCA 脚本明确表示不支持 distance；不能据此调整 ACK 超时。",
    },
  },
  "wifi-iface": {
    device: {
      description:
        "把此无线网络挂到同文档中的 wifi-device 章节。扫描器据此把 wifi-iface 收集到所属射频的 VAP 列表。",
      dependencies: ["必须对应实际 wifi-device；MLO 链路另需匹配 mld 关联。"],
      flags: ["hardware-dependent"],
      impact:
        "换射频会改变所用频段、信道和驱动能力；引用不存在的章节可能使此网络无法建立。",
      evidence: [
        {
          source: "lib/wifi_interface_helper.sh",
          line: 94,
          endLine: 98,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "wifi-iface.device 被读取，并把该 iface 追加到对应 device 的 vifs。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 3942,
          endLine: 3948,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "MLO 链路从 iface.device 寻找射频的 mldphy_name。",
        },
      ],
    },
    network: {
      description:
        "连接到 network 文档中的逻辑网络，可使用原生列表。缺省并非一律 lan：帮助函数会尝试按 ifname 查找已有网络配置。",
      dependencies: [
        "引用 /etc/config/network 中的逻辑接口；STA 桥接需匹配 WDS/厂商桥接模式。",
      ],
      impact:
        "会改变无线流量所在桥接网络及其地址分配路径；关联错网络可能无法访问 LAN 或管理面板。",
      evidence: [
        {
          source: "lib/wifi_interface_helper.sh",
          line: 10,
          endLine: 21,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "读取 network；为空时按 ifname 调用 find_config。",
        },
        {
          source: "lib/wifi_interface_helper.sh",
          line: 46,
          endLine: 51,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "对关联网络逐项调用 setup_interface。",
        },
        {
          source: "lib/wifi/wpa_supplicant.sh",
          line: 186,
          endLine: 189,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "STA 桥接需要 wds/extap/qwrap 等条件，否则拒绝桥接。",
        },
      ],
    },
    ifname: {
      description:
        "内核无线接口名，也是 hostapd/supplicant 配置与状态路径的定位依据。厂商在缺省或 SON 分支中会自动生成它。",
      dependencies: ["与 device、驱动生成规则及厂商服务引用保持一致。"],
      flags: ["generated", "hardware-dependent"],
      impact:
        "改名会改变创建和关联配置的目标；厂商更新也用 ifname 匹配旧 VAP，可能删除重建接口。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 1021,
          endLine: 1026,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "SON 分支写入生成名称；其他分支使用生成名作为 ifname 缺省。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 6009,
          endLine: 6021,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "更新用旧、新 ifname 匹配接口，未匹配或停用时加入删除列表。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 7836,
          endLine: 7841,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "ifname 用于 wlanconfig 与 iw 接口创建。",
        },
      ],
    },
    mode: {
      description:
        "确定接口角色：AP 提供接入，STA 连接上级，其余模式走对应驱动分支。原厂 mesh 被转为 AP 类型，不能直接等同通用 802.11s。",
      dependencies: [
        "STA 桥接依赖 wds/extap；AP 与 STA 的认证配置由不同服务消费。",
        "MLO 链路角色还受 wifi-iface.mld 和 wifi-mld.role 影响；mode 不能替代 MLO/TWT 开关。",
      ],
      flags: ["hardware-dependent", "version-dependent"],
      impact:
        "切换角色会改变认证服务和桥接条件，并可能重建接口；原有客户端连接可能中断。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 1049,
          endLine: 1052,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商扫描接受 ap/sta/adhoc/monitor/mesh 等模式并收集 VAP。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 7794,
          endLine: 7801,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "创建前把 ap 转为 __ap、sta 转为 managed，并把 mesh 也转为 __ap。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 3932,
          endLine: 3938,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "MLO 的 link_mode=sta 会使所选 wlanmode 为 managed。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 9785,
          endLine: 9786,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "TWT 由独立 twt_responder 选项下发。",
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQWifiUtil.lua",
          line: 12386,
          endLine: 12411,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "set_twt_hostap 对所选 wifinet 写入 twt_responder，并保存提交 wireless。",
        },
      ],
      summary: "选择接口角色；原厂 mesh 创建为 AP 类型，不等同通用 802.11s。",
    },
    ssid: {
      description:
        "AP 广播或 STA 匹配的无线网络名。原厂分别写入 hostapd 的 ssid 与 supplicant 的 network 块。",
      unit: "字节（不是字符数）",
      range:
        "802.11 SSID 最多 32 字节；厂商界面另有按频段长度策略，脚本此处未校验上限。",
      dependencies: [
        "MLO STA 同组链路需要一致 SSID；hidden 只控制可见性，不替代名称。",
      ],
      impact:
        "改名会让旧名称的客户端需要重新选择网络；MLO STA 伙伴链路名称不一致会触发配置不匹配。",
      evidence: [
        {
          source: "lib/wifi/hostapd.sh",
          line: 628,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "hostapd BSS 配置读取 iface.ssid。",
        },
        {
          source: "lib/wifi/hostapd.sh",
          line: 768,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "SSID 被写为 hostapd ssid。",
        },
        {
          source: "lib/wifi/wpa_supplicant.sh",
          line: 598,
          endLine: 603,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "supplicant network 块写入 ssid。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 4398,
          endLine: 4415,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "MLO STA 读取各链路 SSID，并在与已记录名称不一致时标记失败。",
        },
      ],
    },
    encryption: {
      description:
        "选择认证/加密配置。原厂解析加密字符串；WPA3/增强开放还涉及独立 sae/owe、sae_password 与 PMF 选项，不保证单改通用名称就等价。",
      defaultValue: "none（脚本缺省；并非建议使用开放网络）",
      dependencies: [
        "PSK/WEP 关联 key；企业认证关联 RADIUS；SAE/OWE 关联厂商独立开关与 ieee80211w。",
        "MLO STA 各链路认证参数须一致；TWT 仍是独立设置，没有从加密值自动开启的证据。",
      ],
      flags: ["hardware-dependent", "version-dependent"],
      impact:
        "会改变客户端认证及密码要求；不支持的组合可能跳过或销毁 VAP，导致网络无法上线。",
      evidence: [
        {
          source: "lib/wifi/hostapd.sh",
          line: 290,
          endLine: 291,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "读取 encryption，缺失用 none，并转为小写。",
        },
        {
          source: "lib/wifi/hostapd.sh",
          line: 415,
          endLine: 465,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "解析 WPA 版本及 TKIP/CCMP/GCMP 字符串组件。",
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQWifiUtil.lua",
          line: 4855,
          endLine: 4875,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商 ccmp 方案同步 sae、sae_password、ieee80211w；psk2+ccmp 方案设置混合认证相关值。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 4418,
          endLine: 4425,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "MLO STA 链路 encryption 不一致时报告 MLD ENC Mismatch 并失败。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 6884,
          endLine: 6889,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "SAE/OWE 启用时拒绝 TKIP/WEP。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 9785,
          endLine: 9786,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "TWT 响应另读 twt_responder，并非从 encryption 推导。",
        },
      ],
      summary:
        "原厂 WPA3 等还需 sae/owe 与 PMF 伴随参数；不保证单改通用加密名称就等价。",
    },
    key: {
      description:
        "按认证方案提供密码或原生密钥。PSK 分支将 64 字节内容按预共享密钥输出，否则作为口令；WEP 时可用 1–4 选择 key1–key4。",
      range:
        "PSK 标准口令 8–63 字节或 64 位十六进制；厂商密码接口检查 8–63 字节，底层另支持 64 字节分支。",
      dependencies: [
        "与 encryption 配套；WEP 数字值选择密钥槽，SAE 可另用 sae_password；MLO STA 密钥须一致。",
      ],
      flags: ["credential"],
      impact:
        "密钥与客户端不一致会导致认证失败；厂商某些加密分支发现 key 与替代密钥来源都为空时会销毁该 VAP。",
      evidence: [
        {
          source: "lib/wifi/hostapd.sh",
          line: 514,
          endLine: 520,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "PSK 分支读取 key；长度 64 写为 wpa_psk，其他非空值写为 wpa_passphrase。",
        },
        {
          source: "lib/wifi/hostapd.sh",
          line: 544,
          endLine: 560,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "WEP 的 key 为 1–4 时读取对应密钥槽并减 1 作为默认索引，其他值作为直接密钥。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 6874,
          endLine: 6882,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "PSK/WEP 等分支检查 key 和 wpa_psk_file；均为空时销毁 VAP。",
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQWifiUtil.lua",
          line: 4855,
          endLine: 4871,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商纯 SAE 路径改用 sae_password；混合路径同时保留 key 与 sae_password。",
        },
        {
          source:
            "static/lua-analysis/decompiled/usr/lib/lua/xiaoqiang/util/XQWifiUtil.lua",
          line: 4424,
          endLine: 4437,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商非开放/非 wep-open 密码校验要求长度不少于 8 且不超过 63。",
        },
      ],
    },
    key1: {
      description:
        "WEP 的第 1 组密钥，生成器映射为 wep_key0。只有 encryption 走 WEP 且 key 使用 1–4 槽位选择时才遍历读取。",
      dependencies: [
        "WEP 认证；key=1 选中本槽。原厂还会按 force_wep 检查是否允许 WEP。",
      ],
      flags: ["credential", "legacy"],
      impact: "key=1 时它成为发送默认密钥；不匹配会使 WEP 认证或解密失败。",
      evidence: [
        {
          source: "lib/wifi/hostapd.sh",
          line: 548,
          endLine: 556,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "key=1–4 时循环读取 key1–key4，并以 idx-1 生成 wep_key0–3。",
        },
        {
          source: "lib/wifi/wpa_supplicant.sh",
          line: 199,
          endLine: 208,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "supplicant 同样读取四个 WEP 槽位并以 key-1 选择发送索引。",
        },
      ],
    },
    key2: {
      description:
        "WEP 的第 2 组密钥，映射为 wep_key1。厂商企业认证辅助路径也可能把它作为第二认证服务器共享密钥的旧式回退。",
      dependencies: [
        "WEP 的 key=2；另需注意企业认证 auth_secret2 的回退路径。",
      ],
      flags: ["credential", "legacy"],
      impact:
        "key=2 时用于 WEP 默认发送密钥；若第二 RADIUS 密钥未单列，也可能影响备用服务器认证。",
      evidence: [
        {
          source: "lib/wifi/hostapd.sh",
          line: 548,
          endLine: 556,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "槽位循环把 key2 映射为索引 1，默认发送索引按 key-1。",
        },
        {
          source: "lib/wifi/hostapd.sh",
          line: 173,
          endLine: 175,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "auth_secret2 缺失时读取 key2，再生成第二服务器 shared_secret。",
        },
      ],
    },
    key3: {
      description:
        "WEP 的第 3 组密钥，映射为 wep_key2；仅在 WEP 槽位模式下被读取，不是第三组 WPA 密码。",
      dependencies: ["encryption 为 WEP 且 key 为槽位选择；key=3 选中本槽。"],
      flags: ["credential", "legacy"],
      impact: "key=3 时选为发送默认密钥；与对端不一致会导致 WEP 解密失败。",
      evidence: [
        {
          source: "lib/wifi/hostapd.sh",
          line: 548,
          endLine: 556,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "key=1–4 时循环读 key1–key4，并按 idx-1 输出 WEP 密钥与默认索引。",
        },
        {
          source: "lib/wifi/wpa_supplicant.sh",
          line: 199,
          endLine: 208,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "supplicant 槽位循环生成 wep_key0–3 与 wep_tx_keyidx。",
        },
      ],
    },
    key4: {
      description: "WEP 的第 4 组密钥，映射为 wep_key3；只用于 WEP 槽位配置。",
      dependencies: [
        "encryption 为 WEP；key=4 选中本槽；底层是否允许 WEP 受厂商 force_wep 检查。",
      ],
      flags: ["credential", "legacy"],
      impact: "key=4 时成为默认发送密钥；不能把它当作 WPA 备用密码来轮换。",
      evidence: [
        {
          source: "lib/wifi/hostapd.sh",
          line: 548,
          endLine: 556,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "槽位循环覆盖 idx=4，按 idx-1 输出并按 key-1 选择默认密钥。",
        },
        {
          source: "lib/wifi/wpa_supplicant.sh",
          line: 199,
          endLine: 208,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "supplicant 读取 idx=1–4 的密钥槽并设置发送索引。",
        },
      ],
    },
    disabled: {
      description:
        "停用此 wifi-iface。厂商在 VAP 创建与启动阶段读到 1 会跳过，不会因此停用同射频其他 SSID。",
      defaultValue: "0（创建与启动读取缺省）",
      dependencies: ["所属 device 也必须启用；MLO 更新另外协调同组链路。"],
      impact:
        "该 SSID 或上联接口会下线；若是回程 STA/MLO 链路，可能连带失去上级连接。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 7748,
          endLine: 7749,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "VAP 初始化读取 disabled，缺省 0；非 0 跳过。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 9263,
          endLine: 9264,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "VAP 启动阶段再次读取 disabled，非 0 返回。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 5769,
          endLine: 5782,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "MLO 更新比较 iface.disabled、mlo_enable 与上联变化，并标记需更新的链路。",
        },
      ],
    },
    hidden: {
      description:
        "控制 SSID 的信标/扫描可见性。厂商同时下发 hide_ssid 与 hostapd ignore_broadcast_ssid；这不是认证或加密措施。",
      defaultValue: "0（显示 SSID）",
      dependencies: ["保留 ssid 与正确密码；隐藏网络的发现行为依客户端实现。"],
      impact:
        "客户端可能需要手动填写名称；厂商更新把 hidden 变化纳入接口重新配置流程。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 8108,
          endLine: 8112,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "hidden 缺省 0，下发 hide_ssid；值为 1 时还处理 dynamicbeacon。",
        },
        {
          source: "lib/wifi/hostapd.sh",
          line: 2296,
          endLine: 2297,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "hostapd BSS 写入 ignore_broadcast_ssid。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 6436,
          endLine: 6440,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "hidden_changed 与认证、信道等变化触发接口更新处理。",
        },
      ],
    },
    isolate: {
      description:
        "阻止 AP 类接口内客户端通过无线桥接直接互访。QCA 使用 isolate 的反值设置 ap_bridge；wrap 模式还可能被射频隔离强制覆盖。",
      defaultValue: "0（不要求接口隔离；wrap 可能覆盖）",
      dependencies: [
        "主要作用于 AP/mesh/wrap 类路径；关联射频 ap_isolation_enabled。",
      ],
      impact:
        "改变同一 BSS 客户端的互通；不等同于为不同逻辑网络配置完整防火墙隔离。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 9268,
          endLine: 9273,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "读取 isolate 缺省 0；射频隔离启用时 wrap 强制 isolate=1。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 9360,
          endLine: 9364,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "AP 类模式将 isolate 的反值下发到 ap_bridge。",
        },
      ],
    },
    wds: {
      description:
        "启用四地址无线桥接。厂商把 1/on/enabled 视为启用，并在非 Multi-AP 分支执行 iw set 4addr on。",
      defaultValue: "0（supplicant 读取缺省；QCA 未匹配启用值也转为 0）",
      dependencies: [
        "主要关联 STA、network 桥接与对端支持；map 非 0 时由 supplicant 关联后启用四地址。",
      ],
      impact:
        "会改变 STA 桥接条件；对端需支持相应四地址连接，否则可能无法传递下游流量。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 8172,
          endLine: 8185,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "wds 启用值转换为 1；map=0 时设置 4addr，随后下发驱动 wds。",
        },
        {
          source: "lib/wifi/wpa_supplicant.sh",
          line: 180,
          endLine: 189,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "supplicant 将 wds 缺省为 0，STA 桥接检查允许 wds=1。",
        },
      ],
    },
    wmm: {
      description:
        "无线多媒体优先级开关。QCA 驱动层读取 wmm 并下发，但原厂 hostapd BSS 生成路径另固定写 wmm_enabled=1。",
      dependencies: [
        "驱动 wmm 与 hostapd 宣告分别处理；不要将单个字段视为全链路开关。",
      ],
      flags: ["hardware-dependent", "version-dependent"],
      impact:
        "驱动命令可变化，但不能保证关闭此字段就会关闭对客户端宣告的 WMM；两层配置可能不一致。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 8527,
          endLine: 8528,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "读取布尔 wmm，并在非空时下发同名驱动命令。",
        },
        {
          source: "lib/wifi/hostapd.sh",
          line: 2289,
          endLine: 2290,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "原厂 BSS 生成器固定输出 wmm_enabled=1。",
        },
      ],
      summary:
        "QCA 下发 wmm，但 hostapd BSS 固定宣告开启；不保证单关此项就关闭 WMM。",
    },
    ieee80211r: {
      description:
        "启用快速 BSS 切换。AP 生成器据此增加 FT 认证算法及漫游域参数；STA 路径另下发 ft 驱动选项。",
      defaultValue: "0（AP 读取缺省）",
      dependencies: [
        "同一漫游域的 SSID、认证方式、mobility_domain 与 R0/R1 配置须协调；MLO 还有桥接 FDB 处理。",
      ],
      impact:
        "改变漫游认证方式；需其他 AP、认证参数和客户端配合，单独开启不能保证无缝漫游。",
      evidence: [
        {
          source: "lib/wifi/hostapd.sh",
          line: 298,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "AP 配置把 ieee80211r 缺省为 0。",
        },
        {
          source: "lib/wifi/hostapd.sh",
          line: 821,
          endLine: 838,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "启用时据 SAE/Suite-B/普通认证选择 FT 算法。",
        },
        {
          source: "lib/wifi/hostapd.sh",
          line: 1290,
          endLine: 1313,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "FT 分支读取 mobility_domain 与密钥持有者等参数。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 9563,
          endLine: 9565,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "STA 模式读取 ieee80211r 并下发 ft。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 2696,
          endLine: 2716,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "MLD 且 ieee80211r 非空时，桥接辅助路径处理本地 FDB 项。",
        },
      ],
    },
    ieee80211k: {
      description:
        "通用字段表示无线资源测量/邻居信息。此固件已找到厂商 rrm 命令入口，但未找到 ieee80211k 选项到该入口的映射。",
      dependencies: [
        "与支持测量的 AP/客户端及厂商 rrm 配置相关；不等同于 ieee80211r。",
      ],
      flags: ["version-dependent"],
      discovery:
        "已检索 sbin/wifi、lib/wifi、lib/netifd、初始化脚本及全部反编译 Lua，未发现 ieee80211k。hostapd ELF 可找到 rrm_neighbor_report 选项字符串，但无证据表明此 UCI 字段被转换到它；rrm 是已验证的独立厂商读取。",
      impact:
        "不能确认切换此字段会改变 RN02 的测量或邻居报告；不应当作已验证的漫游开关。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 8900,
          endLine: 8901,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商读取 wifi-iface.rrm 并下发 rrm。",
        },
      ],
      summary: "通用无线测量开关；原厂读取 rrm，尚未找到 ieee80211k 映射。",
    },
    ieee80211w: {
      description:
        "控制受保护管理帧（PMF）：0 停用、1 可选、2 必须。原厂 SAE/OWE/Suite-B 分支可能强制或提升有效值。",
      defaultValue: "0（一般路径）；SAE/OWE/Suite-B 有条件覆盖。",
      range: "0 / 1 / 2（PMF 协议含义）",
      dependencies: [
        "关联 encryption、sae、owe、suite_b；MLO STA 各链路需一致，实际值以生成配置为准。",
      ],
      impact:
        "设为必须可能使不支持 PMF 的客户端无法连接；它也参与 MLO STA 伙伴链路的一致性检查。",
      evidence: [
        {
          source: "lib/wifi/hostapd.sh",
          line: 796,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "一般 AP 读取 ieee80211w 缺省 0。",
        },
        {
          source: "lib/wifi/hostapd.sh",
          line: 888,
          endLine: 905,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "SAE 分支对 PSK 的 0 提升为 1，其他非企业分支设为 2。",
        },
        {
          source: "lib/wifi/hostapd.sh",
          line: 919,
          endLine: 925,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "OWE 非 WPA/PSK 分支读取 ieee80211w，缺省为 2。",
        },
        {
          source: "lib/wifi/hostapd.sh",
          line: 958,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "最终值写入 hostapd ieee80211w。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 4448,
          endLine: 4455,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "MLO STA 的 PMF 参数不一致会报告 MLD PMF Mismatch。",
        },
      ],
      summary: "PMF 的 0/1/2 为停用/可选/必须；SAE、OWE 等可能提升有效值。",
    },
    bssid: {
      description:
        "指定 STA/Ad-Hoc 的目标 AP 地址。supplicant 网络块会写入 bssid；QCA 在客户端/Ad-Hoc 分支也用 iwconfig ap 设置目标。",
      dependencies: [
        "主要用于 mode=sta/adhoc；须匹配上级 SSID 与认证，MLO 另有 preferred_ap_mld_addr。",
      ],
      flags: ["hardware-dependent"],
      impact:
        "会限制所连接的上级；目标地址错误或上级更换 BSSID 后可能无法关联，不是 AP 自身地址设置。",
      evidence: [
        {
          source: "lib/wifi/wpa_supplicant.sh",
          line: 521,
          endLine: 523,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "读取 bssid，非空时生成 bssid=<地址>。",
        },
        {
          source: "lib/wifi/wpa_supplicant.sh",
          line: 601,
          endLine: 604,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "目标 bssid 插入 supplicant network 块。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 8446,
          endLine: 8453,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "sta/adhoc 分支读取 bssid 并通过 iwconfig ap 下发。",
        },
      ],
    },
    macfilter: {
      description:
        "按 maclist 设置第二套厂商 MAC ACL。allow 走允许列表，deny 走拒绝列表；其他值在列表非空时仍走拒绝。",
      range:
        "allow / deny；其他值的行为依列表是否为空，未证实显式 disable 命令。",
      dependencies: ["与 maclist 配套；厂商 backhaul AP 跳过该路径。"],
      flags: ["hardware-dependent", "version-dependent"],
      impact:
        "会改变客户端接入权限；在此脚本中“disable”加非空列表并不保证停用过滤，可能仍拒绝列表内设备。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 8631,
          endLine: 8638,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商回程 AP 接口被排除在这段 MAC 过滤设置之外。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 8649,
          endLine: 8661,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "allow/deny 分别下发 maccmd_sec 1/2；其他值在 maclist 非空时也下发 2。",
        },
      ],
      summary:
        "allow/deny 使用第二套 ACL；其他值加非空列表仍走拒绝，disable 不保证停用。",
    },
    maclist: {
      description:
        "MAC ACL 的客户端地址列表。原厂在非回程 AP 分支先清空第二套 ACL，再逐项调用 addmac_sec。",
      range: "原生 MAC 地址列表；不是一个 IP 地址池。",
      dependencies: ["与 macfilter 一起配置；回程 AP 排除在该段处理之外。"],
      flags: ["version-dependent"],
      impact:
        "配合 macfilter 可阻止客户端接入；空列表时此分支不执行清空，不能保证旧运行时列表立刻消失。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 8632,
          endLine: 8638,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "按厂商回程接口名称判断是否跳过 MAC ACL。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 8640,
          endLine: 8647,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "非空 maclist 先下发 maccmd_sec 3，再遍历地址并调用 addmac_sec。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 8657,
          endLine: 8659,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "macfilter 未匹配 allow/deny 且 maclist 非空时采用拒绝策略。",
        },
      ],
      summary: "非空列表先清空再写第二套 ACL；空列表不保证清除旧运行时项。",
    },
    maxassoc: {
      description:
        "通用字段表示最多关联客户端数。原厂同类控制读取的是 maxsta，并按 2.4/5 GHz 的 misc 最大站点配置覆盖；未找到 maxassoc 映射。",
      unit: "客户端数量（通用字段语义）",
      dependencies: [
        "原厂已验证入口为 maxsta 与 misc.wireless.if_2g_maxsta/if_5g_maxsta。",
      ],
      flags: ["hardware-dependent", "version-dependent"],
      discovery:
        "已检索 sbin/wifi、lib/wifi、lib/netifd、初始化脚本与全部反编译 Lua，未发现 maxassoc 或到 maxsta/max_num_sta 的映射。hostapd ELF 有 max_num_sta 字符串，但它不能证明 maxassoc 被消费。",
      impact:
        "不能确认修改 maxassoc 会限制 RN02 客户端数量；不能把厂商 maxsta 逻辑的上限或缺省搬到此字段。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 8703,
          endLine: 8710,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "读取 maxsta；随后按频段读取 misc 的最大站点配置，再下发 maxsta 命令。",
        },
      ],
      summary: "通用客户端上限字段；原厂读取 maxsta，尚未找到 maxassoc 映射。",
    },
    dtim_period: {
      description:
        "DTIM 通知相隔的信标周期数。AP 类路径为空时补入缺省，并向驱动和 hostapd 下发。",
      defaultValue: "1；厂商 ap_lp_iot 模式为 41。",
      unit: "信标周期数",
      range: "802.11 DTIM 周期 1–255；此脚本未自行检查范围。",
      dependencies: [
        "用于 AP 类模式；与信标间隔（厂商 bintval）共同决定通知间隔。",
      ],
      impact:
        "改变省电客户端接收缓存组播/广播的通知节奏；值越大通常等待越久，但具体耗电和时延取决于客户端。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 9378,
          endLine: 9385,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "ap_lp_iot 缺省 DTIM 为 41，其他 AP 类模式为 1，缺失时写入。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 8542,
          endLine: 8543,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "dtim_period 非空时下发同名驱动命令。",
        },
        {
          source: "lib/wifi/hostapd.sh",
          line: 2294,
          endLine: 2295,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "hostapd 生成读取 dtim_period，缺省 1，并写入配置。",
        },
      ],
    },
    auth_server: {
      description:
        "AP 企业认证使用的 RADIUS 认证服务器。原厂将其输出为 auth_server_addr，缺失时还尝试旧字段 server。",
      dependencies: [
        "AP 企业 WPA/EAP 或 802.1X 认证；配合 auth_port 与 auth_secret。",
      ],
      flags: ["version-dependent"],
      impact:
        "服务器无法访问会使依赖它的企业认证失败；PSK/开放网络不因此自动变为 RADIUS 认证。",
      evidence: [
        {
          source: "lib/wifi/hostapd.sh",
          line: 155,
          endLine: 157,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "auth_server 为空时回退 server，并生成 auth_server_addr。",
        },
        {
          source: "lib/wifi/hostapd.sh",
          line: 540,
          endLine: 542,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "企业 WPA 分支调用 RADIUS 参数生成函数。",
        },
      ],
    },
    auth_port: {
      description:
        "RADIUS 认证服务器端口。原厂先读 auth_port，再尝试旧 port，仍为空时使用 1812。",
      defaultValue: "1812（auth_port 与旧 port 都为空时）",
      unit: "UDP 端口号",
      range: "1–65535（网络端口范围；脚本未单独校验）",
      dependencies: ["与 auth_server 配套，仅在 RADIUS 参数生成分支使用。"],
      impact:
        "端口需与服务器一致；改错会导致认证请求无法到达，客户端无法完成企业认证。",
      evidence: [
        {
          source: "lib/wifi/hostapd.sh",
          line: 160,
          endLine: 163,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "auth_port 为空时读取 port，最终回退 1812，再写入 auth_server_port。",
        },
      ],
    },
    auth_secret: {
      description:
        "与 RADIUS 认证服务器约定的共享密钥。生成器读取此字段，缺失时回退 key；它不是 Wi-Fi PSK 的通用替代名称。",
      dependencies: [
        "AP 企业认证/802.1X；配合 auth_server、auth_port，留意 key 回退。",
      ],
      flags: ["credential"],
      impact:
        "共享密钥与服务器不一致会导致 RADIUS 校验失败，客户端无法完成相应企业认证。",
      evidence: [
        {
          source: "lib/wifi/hostapd.sh",
          line: 164,
          endLine: 166,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "auth_secret 缺失时读取 key，写为 auth_server_shared_secret。",
        },
        {
          source: "lib/wifi/hostapd.sh",
          line: 540,
          endLine: 542,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "企业认证分支调用 RADIUS 参数生成器。",
        },
      ],
    },
    acct_server: {
      description:
        "企业认证 AP 的 RADIUS 计费服务器。只有非空时才写入 acct_server_addr，与认证服务器地址分别配置。",
      dependencies: [
        "企业认证 RADIUS 生成分支；配合 acct_port 与 acct_secret。",
      ],
      impact:
        "会改变计费消息目标；计费可达性和认证可达性是两条配置路径，不保证单改此项启用完整计费。",
      evidence: [
        {
          source: "lib/wifi/hostapd.sh",
          line: 176,
          endLine: 177,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "非空 acct_server 写为 acct_server_addr。",
        },
        {
          source: "lib/wifi/hostapd.sh",
          line: 540,
          endLine: 542,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "企业 WPA 分支调用包含计费参数的 RADIUS 生成器。",
        },
      ],
    },
    acct_port: {
      description:
        "RADIUS 计费端口。此脚本仅当 acct_port 非空才输出；虽然出现 1813 回退表达式，但外层非空条件使空值不会在此补成 1813。",
      unit: "UDP 端口号",
      range: "1–65535（网络端口范围；脚本未单独校验）",
      dependencies: [
        "acct_server、acct_secret 与企业 RADIUS 生成分支；不要把源码的条件表达式误当确定缺省。",
      ],
      flags: ["version-dependent"],
      impact:
        "必须与计费服务器监听端口一致；未填写时该生成器省略端口，后续服务如何补值未由此处证明。",
      evidence: [
        {
          source: "lib/wifi/hostapd.sh",
          line: 178,
          endLine: 180,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "acct_port 非空才执行 ${acct_port:-1813}，并且非空才写入 acct_server_port。",
        },
      ],
      summary: "非空才生成计费端口；源码此处未证实空值默认 1813。",
    },
    acct_secret: {
      description:
        "与 RADIUS 计费服务器约定的共享密钥。原厂仅在非空时输出 acct_server_shared_secret，没有从认证密钥回退的代码。",
      dependencies: ["配合 acct_server、acct_port；企业 RADIUS 参数生成分支。"],
      flags: ["credential"],
      impact:
        "与计费服务器不一致会导致计费报文校验失败；它与 auth_secret 不应假定相同。",
      evidence: [
        {
          source: "lib/wifi/hostapd.sh",
          line: 181,
          endLine: 182,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "读取 acct_secret，非空时写为 acct_server_shared_secret。",
        },
      ],
    },
    mesh_id: {
      description:
        "通用 802.11s 网络标识字段。原厂有同名驱动命令，但其参数来自 xiaoqiang.common.NETWORK_ID，而非已证实的 wifi-iface.mesh_id。",
      dependencies: [
        "通用语义用于 802.11s；厂商 Mesh 则涉及 miwifi_mesh、NETWORK_ID 与回程配置。",
      ],
      flags: ["version-dependent"],
      discovery:
        "已检索 lib/wifi、lib/mimesh、usr/sbin Mesh 脚本、netifd 与反编译 Lua；未找到读取 wifi-iface.mesh_id。存在的 mesh_id 驱动命令读取的是 xiaoqiang.common.NETWORK_ID，不证明这个字段被消费。",
      impact:
        "此字段是否影响 Xiaomi Mesh 归属尚未确认；厂商 NETWORK_ID 是另一个配置入口，不等同 SSID。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 8136,
          endLine: 8137,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商接口另读取 miwifi_mesh 开关。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 8142,
          endLine: 8148,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "按 mesh_cmd 版本读取 NETWORK_ID，并以 0x 前缀下发 mesh_id 命令。",
        },
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 7800,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商创建路径把 mode=mesh 映射为 __ap。",
        },
      ],
      summary:
        "通用 802.11s 标识；原厂同名命令读取 NETWORK_ID，未证实此 UCI 字段消费。",
    },
    mesh_fwding: {
      description:
        "通用 802.11s 节点转发开关。原厂 mesh 接口创建为 AP 类型；目前未找到 wifi-iface.mesh_fwding 到驱动或 supplicant 的传递。",
      dependencies: [
        "通用 802.11s 模式语义不等于原厂 Xiaomi Mesh 回程；需相应模式和消费者支持。",
      ],
      flags: ["version-dependent"],
      discovery:
        "已检索 sbin/wifi、lib/wifi、lib/mimesh、netifd、初始化及全部反编译 Lua，未出现 mesh_fwding。wpa_supplicant ELF 存在 mesh_fwding 解析字符串，但没有找到此 UCI 字段到该后端选项的转换。",
      impact:
        "此开关是否影响 Xiaomi Mesh 回程转发尚未确认；通用 802.11s 语义不能证明原厂转发路径会停止。",
      evidence: [
        {
          source: "lib/wifi/qcawificfg80211.sh",
          line: 7794,
          endLine: 7801,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "厂商创建接口时把 mesh 等 AP 类模式映射为 __ap。",
        },
        {
          source: "lib/wifi/wpa_supplicant.sh",
          line: 598,
          endLine: 605,
          firmware: "Xiaomi RN02 1.0.43",
          fact: "supplicant network 生成块写入模式、SSID、BSSID 与认证等参数。",
        },
      ],
      summary: "通用 802.11s 转发开关；尚未找到它控制 Xiaomi Mesh 回程的路径。",
    },
  },
};
