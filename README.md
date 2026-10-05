# be6500panel

专为**小米路由器 BE6500（RN02）**打造的现代化嵌入式网关控制中心。

[![Rust](https://img.shields.io/badge/Rust-1.93%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![React](https://img.shields.io/badge/React-19-blue?logo=react)](https://react.dev/)
[![Arch](https://img.shields.io/badge/Architecture-ARMv7%20musl-green)]()
[![Platform](https://img.shields.io/badge/Target-Xiaomi%20BE6500%20%28RN02%29-red)]()
[![Status](https://img.shields.io/badge/Production-Verified%20%280%20Alerts%29-brightgreen)]()

---

## 📖 产品概述

**be6500panel** 专为高通 IPQ5322 四核平台构建，在极低的系统资源占用下，提供集**原生路由控制、分流代理、配置版本审计、网络遥测与内网穿透**于一体的专业级网关控制台。

### 🌟 核心设计

1. **轻量嵌入式单所有者内核（Rust）**  
   后端完全采用 Rust 构建，采用单监听服务循环、绝对时间预算与代次管理。实测管理器进程驻留内存（RSS）约为 **5–7 MiB**（包含已加载配置数据，不含独立的 sing-box 核心进程），路由器端**无需 Node.js 运行时**。
2. **现代化 Web 前端控制台**  
   前端基于 React 19、Tailwind CSS、Radix UI、Apache ECharts 以及 Effect.js 打造。静态资源由 Rust 原生服务直接托管，提供高信息密度图表、即时全局搜索（Cmd+K）、状态徽章与无缝交互，支持暗色与明色主题自动切换。
3. **清晰直观的交互与状态呈现**  
   状态、错误码与日志客观直截了当呈现；图表与统计基于真实采样，高风险操作一次明确确认，避免操作负担。
4. **可靠的配置生命周期与安全回滚**  
   网络与系统变更遵循标准的 `编辑 → 暂存 (Stage) → 差异校验 (Diff) → 提交 (Commit)` 流程。管理连接相关的关键网络变更引入连接确认与**超时自动回滚保护**，防止误配置导致路由器失联。

---

## 🏗️ 系统架构

![be6500panel 系统架构](docs/images/architecture.svg)

- **前端控制台**：静态构建产物，由嵌入式服务统一对外托管；
- **通信通道**：基于 Session Cookie 认证的 HTTP API，配合 `/api/events` 实时 Server-Sent Events（SSE）事件流推送系统状态；
- **嵌入式控制平面（Rust）**：统一处理系统 procfs/sysfs/UCI 原生观察、原子配置代次引擎，以及 sing-box、frpc 守护进程的生命周期管理。

---

## 🚀 核心功能矩阵

| 功能模块 | 核心能力说明 |
| :--- | :--- |
| **运行概览 (Dashboard)** | 实时掌握网关状态。呈现系统平均负载（Load Average）、内存分布、实时上下行速率、WAN 流量历史、设备活跃热力图及模块健康状态。 |
| **代理与规则管理 (Proxy)** | **本地智能分流中心**。<br>• 本地订阅解析，节点凭据不经过第三方托管；<br>• 支持 VLESS、REALITY、Vision、uTLS 等加密协议；<br>• **自定义规则编辑器**：支持精确域名直连、订阅规则按内容指纹保留改写、草稿保存 / 合并预览 / 应用三阶段生命周期；<br>• 采用 routed-TUN 虚拟网卡进行流量分流。 |
| **原生网络与防火墙 (Network)** | 物理网口（2.5G）双向流量计数器差值速率统计；实时观察 IPv4 / IPv6 系统路由表；原生原厂防火墙区域规则观察与提交。 |
| **局域网设备管理 (Devices)** | 基于 DHCP 租约与 ARP 缓存被动观察局域网在线设备；展示 IP-MAC 对应关系与核心连接出口，支持设备自定义命名标注。 |
| **系统服务运维 (Services)** | 基于系统 `procd` 与 `/proc` 实时状态监督后台进程；支持常见服务的启动、停止与重启；提供系统配置的一键快照备份与恢复。 |
| **内网穿透监督 (FRPC)** | 原生 frpc 守护进程监管与本地 TOML 配置支持；独立生命周期控制与自愈，无隧道配置时客观显示状态。 |

---

## 🖼️ 产品界面预览

### 1. 控制中心主仪表盘 (`/#/overview`)
![控制中心主仪表盘](docs/images/overview-dashboard.png)
> **运行概览**：实时聚合呈现网关负载指标、WAN 流量历史趋势、已连接终端与服务运行状态，按模块采样周期与 SSE 事件流实时更新。

### 2. 网关代理与自定义规则编辑器 (`/#/proxy`)
![网关代理与规则编辑器](docs/images/proxy-rules-editor.png)
> **智能分流控制台**：包含节点选择、网关代理状态卡片、置顶直连规则、草稿编辑保存、合并规则即时预览与一次确认生效的完整流水线。

### 3. 网络与设备拓扑观察 (`/#/network`)
![网络与设备观察](docs/images/network-topology.png)
> **全屋终端与接口观察**：清晰罗列 2.5G 网口物理连接状态，基于实时租约与 ARP 观察局域网已接入终端，支持快速检索与出口分析。

---

## 🛡️ 配置安全工作流

为确保路由设备在配置修改时保持稳定，be6500panel 实现了四阶段安全流程：

```text
[ 用户编辑草稿 ] ──> [ 加入暂存 (Stage) ] ──> [ 查看差异 (Diff) 与校验 ] ──> [ 确认应用 (Commit) ]
                                                                                   │
                                                                   [ 超时未确认？] ──> [ 自动回滚至上一代 ]
```

- **草稿隔离**：在未点击应用前，参数变更仅保存在本地草稿，不写入路由器实际系统配置；
- **差异对账**：提供直观的 Unified Diff 对照视图，变更内容精确呈现；
- **防失联保护**：管理连接相关的网络变更采用试探性应用，若在设定倒计时内未能完成确认，系统自动回滚至上一代稳定配置。

---

## 🛠️ 本地开发与构建

项目采用 **Rust + Bun** 统一组织，无需安装 Go 或其他外部构建工具。

### 环境准备

- **Rust**：1.93 或更高版本（需安装 `cargo`）
- **Bun**：1.0+（前端依赖与脚本执行）
- **Node.js**：20.20+（开发工具链兼容）

### 常用构建命令

```bash
# 1. 初始化前端依赖
make setup

# 2. 启动本地开发服务（回环只读与草稿诊断模式，监听 127.0.0.1:8790）
make dev-api

# 3. 另起终端启动前端 Web 开发服务器（带热重载）
make dev-web

# 4. 运行全套测试套件（Rust 589+ 测试 + 前端 1600+ 单测及类型检查）
make test

# 5. 构建本地发布版二进制与打包前端静态资源
make build

# 6. 交叉编译路由器目标平台（ARMv7 / musl 静态二进制）
make armv7
```

### 原生 Cargo 指令

```bash
# 运行全部 Rust 单元测试与格式化检查
cargo test --locked
cargo fmt --check
cargo clippy --locked -- -D warnings

# 构建优化后的发布版本
cargo build --release --locked
```

---

## 📂 项目工程结构

```text
be6500panel/
├── Cargo.toml          # Rust 根项目配置与依赖声明
├── Cargo.lock          # 严格锁定的 Rust 依赖清单
├── Makefile            # 项目标准开发、测试、构建与交叉编译入口
├── src/                # 嵌入式控制平面核心源码 (Rust)
│   ├── main.rs         # 服务启动入口与参数解析
│   ├── server.rs       # 单所有者监听服务与请求路由
│   ├── product_*.rs    # 系统观察、遥测、配置、诊断与网关核心业务
│   └── capture_*.rs    # 透明接管与 routed-TUN 状态机实现
├── web/                # 现代化前端控制台工程 (React 19 + Vite + Tailwind)
│   ├── src/
│   │   ├── app/        # 页面路由与主框架 Shell
│   │   ├── modules/    # 业务模块（Proxy, Network, System, Devices, FRPC）
│   │   ├── components/ # 视觉基础组件、图表及配置工作区
│   │   └── lib/        # API 客户端、Effect 强类型契约与格式化工具
│   └── package.json
├── tests/              # 跨模块端到端与集成测试套件
│   └── fixtures/       # 原生核心回归基准数据 (native, subscription, capture, local-policy)
├── examples/           # 独立验证工具与示例代码
└── docs/               # 架构规范、技术文档与产品截图
    └── images/         # 系统架构图与真实界面截图
```

---

## ⚙️ 部署与生产运行边界

1. **目标硬件环境**：
   - 设备型号：小米路由器 BE6500（产品代号：RN02）
   - CPU 架构：高通 IPQ5322（ARM Cortex-A53 四核 1.5GHz / `armv7-unknown-linux-musleabihf`）
   - 系统底座：原厂 OpenWrt / QSDK Linux
2. **部署与运行路径**：
   - 生产二进制运行路径：`/tmp/be6500panel/boot-release/be6500-panel`，并在同级挂载 `web/` 静态资源目录；
   - 持久存储与归档：`/data/be6500panel/panel.tar.gz` 维护持久配置与包备份；
   - 内存预算：管理器进程实测 RSS 约 5–7 MiB（不包含独立的 sing-box 核心）。
3. **生产验证指标**：
   - 23 个核心视图通过零告警验证（0 Alerts, 0 Page Errors, 0 API 失败）；
   - 12 项真实浏览器自动化行为验证全部通过（节点分页切换、原节点选择保持、设备详情与 eligible 标记、核心指标、运行时状态、DNS、防火墙、系统服务，以及停用状态下开启按钮保持可用）。

---

## 📄 开源许可证

本项目基于 [MIT License](LICENSE) 开源。
