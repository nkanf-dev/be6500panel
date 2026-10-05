# be6500panel

专为**小米路由器 BE6500（RN02）**打造的现代化、专业级嵌入式网关控制中心。

[![Rust](https://img.shields.io/badge/Rust-1.93%2B-orange?logo=rust)](https://www.rust-lang.org/)
[![React](https://img.shields.io/badge/React-18-blue?logo=react)](https://react.dev/)
[![Arch](https://img.shields.io/badge/Architecture-ARMv7%20musl-green)]()
[![Platform](https://img.shields.io/badge/Target-Xiaomi%20BE6500%20%28RN02%29-red)]()
[![Status](https://img.shields.io/badge/Production-Verified%20%280%20Alerts%29-brightgreen)]()

---

## 📖 产品概述

**be6500panel** 旨在彻底改变传统家用路由器管理面板信息闭塞、功能单一、难以扩展的现状。针对高通 IPQ5322 四核平台量身打造，在极低资源开销下，提供集**原生路由控制、分流代理、配置版本审计、实时网络遥测与内网穿透**于一体的现代化专业控制台。

### 🌟 核心设计理念

1. **高性能嵌入式单所有者内核（Rust）**  
   后端完全采用 Rust 构建，采用单监听、绝对时间预算与代次保护机制。**运行内存占用严格控制在 20MB 以内**，路由器端**零 Node.js 运行时依赖**，兼顾极致性能与长期运行稳定性。
2. **现代化专业级 Web 控制台**  
   前端基于 React 18、Tailwind CSS、Radix UI、Apache ECharts 以及 Effect.js 打造。提供极高信息密度的专业图表、即时全局搜索（Cmd+K）、状态徽章与无缝交互，支持暗色与明色主题随系统自动切换。
3. **去防御性、以用户为中心的健康体验**  
   彻底摒弃法务式“免责条款”与“括号自辩”说教腔；原型与图表展示遵循健康设计常识，运行时数据严密绑定；高风险变更一次明确确认，杜绝弹窗嵌套与操作负担。
4. **可靠的原子配置与自动回滚安全网**  
   所有网络与系统变更遵循标准的 `编辑 → Stage → Diff/校验 → Commit` 生命周期。高风险网络变更引入连接确认倒计时与**超时自动回滚保护**，防止因误配置导致路由器失联。

---

## 🏗️ 系统架构

![be6500panel 系统架构](docs/images/architecture.svg)

- **浏览器控制台**：纯静态前端资源，由嵌入式服务统一托管；
- **通信桥梁**：采用 Session Cookie 认证的严格 RESTful API，配合 `/api/events` 实时 Server-Sent Events（SSE）单向遥测推送；
- **嵌入式控制平面（Rust）**：独立管理系统 procfs/sysfs/UCI 原生观察、原子配置代次引擎，以及 sing-box、frpc 的本地守护进程生命周期。

---

## 🚀 核心功能矩阵

| 功能模块 | 核心能力与用户收益 |
| :--- | :--- |
| **运行概览 (Dashboard)** | 宏观掌握设备心跳。提供 CPU/内存占用、实时上行/下行速率、WAN 流量历史、设备活跃热力图及模块健康状态卡片。 |
| **代理与规则管理 (Proxy)** | **本地专属智能分流中心**。<br>• 本地订阅解析，节点凭据绝不上传第三方托管服务；<br>• 支持 VLESS、REALITY、Vision、uTLS 等现代加密协议；<br>• **独立自定义规则编辑器**：支持域名精确直连、订阅规则按内容指纹保留、草稿保存 / 合并预览 / 运行应用三阶段明确生命周期；<br>• 采用已验证的纯净 **routed-TUN** 网关接管，拒绝 TPROXY 复杂污染。 |
| **原生网络与防火墙 (Network)** | 物理网口（2.5G）双向流量计数器差值速率统计；实时观察 IPv4 / IPv6 系统路由表；原生原厂防火墙区域规则展示与提交。 |
| **局域网设备管理 (Devices)** | 基于 DHCP 租约与 ARP 状态实时探测局域网活跃设备；展示单设备 IP-MAC 拓扑，支持设备核心连接请求审计与自定义个性化命名标注。 |
| **系统服务运维 (Services)** | 基于系统 `procd` 与 `/proc` 实时状态监督守护进程；支持常见后台服务的启动、停止与安全重启；提供全量系统配置的一键快照备份与安全恢复。 |
| **内网穿透监督 (FRPC)** | 原生 frpc 守护进程监管与本地 TOML 配置支持；独立生命周期控制与崩溃自愈，运行状态客观精炼呈现。 |

---

## 🖼️ 产品界面预览

### 1. 控制中心主仪表盘
![控制中心主仪表盘](docs/images/overview-dashboard.png)
> **运行概览**：实时聚合呈现网关核心指标、WAN 流量历史趋势、已连接终端数量与服务运行状态，全域数据 30 秒自动无感刷新。

### 2. 网关代理与自定义规则编辑器
![网关代理与规则编辑器](docs/images/proxy-rules-editor.png)
> **智能分流控制台**：包含节点选择、网关代理状态卡片、置顶直连规则、草稿编辑保存、合并规则即时预览与一次确认生效的完整流水线。

### 3. 网络与设备拓扑观察
![网络与设备观察](docs/images/network-topology.png)
> **全屋终端与接口观察**：清晰罗列 2.5G 网口物理连接状态，基于实时租约与 ARP 探测局域网已接入终端，支持快速检索与出口分析。

---

## 🛡️ 配置安全工作流

为确保路由设备在任何复杂操作下均可恢复，be6500panel 实现了严谨的四阶段工作流：

```text
[ 用户编辑草稿 ] ──> [ 加入暂存 (Stage) ] ──> [ 查看差异 (Diff) 与校验 ] ──> [ 确认应用 (Commit) ]
                                                                                   │
                                                                   [ 超时未确认？] ──> [ 自动回滚至上一代 ]
```

- **草稿隔离**：在未点击应用前，所有参数改动仅保存在本地草稿，不触碰路由器实际系统配置；
- **差异对账**：提供直观的 Unified Diff 对照视图，每次变更影响精确到行；
- **防失联保护**：管理连接相关的网络变更采用试探性应用，若在设定倒计时内未能完成心跳确认，系统自动安全回滚至上一代稳定配置。

---

## 🛠️ 本地开发与构建

项目采用标准 **Rust + Bun** MonoRepo 结构进行统一组织，无需安装 Go 或其他外部构建链路。

### 环境准备

- **Rust**：1.93 或更高版本（需安装 `cargo`）
- **Bun**：1.0+（前端依赖与脚本执行）
- **Node.js**：20.20+（开发工具链兼容）

### 常用构建命令

```bash
# 1. 初始化前端依赖
make setup

# 2. 启动本地模拟后端 API（监听 127.0.0.1:8790）
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
├── Cargo.toml          # Rust 工作区配置与依赖声明
├── Cargo.lock          # 严格锁定的 Rust 依赖清单
├── Makefile            # 项目标准开发、测试、构建与交叉编译入口
├── src/                # 嵌入式控制平面核心源码 (Rust)
│   ├── main.rs         # 服务启动入口与参数解析
│   ├── server.rs       # 单所有者监听服务与请求路由
│   ├── product_*.rs    # 系统观察、遥测、配置、诊断与网关核心业务
│   └── capture_*.rs    # 透明接管与 routed-TUN 状态机实现
├── web/                # 现代化反应式 Web 前端 (React + Vite + Tailwind)
│   ├── src/
│   │   ├── app/        # 页面路由与主框架 Shell
│   │   ├── modules/    # 业务模块（Proxy, Network, System, Devices, FRPC）
│   │   ├── components/ # 视觉基础组件、图表及配置工作区
│   │   └── lib/        # API 客户端、Effect 强类型契约与格式化工具
│   └── package.json
├── tests/              # 跨模块端到端与集成测试套件
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
2. **资源与运行时约束**：
   - 生产环境采用单静态二进制部署（`/data/be6500panel/be6500panel`），无任何动态运行时外链依赖；
   - 静态 Web 资源直接内嵌编译在二进制内部，无需部署外部 Web 服务器；
   - 严格遵循内存预算配额（RSS 驻留内存 < 20MB），零常驻泄漏。
3. **已验证生产指标**：
   - 全面板 23 个核心视图通过零告警验证（0 Alerts, 0 Page Errors）；
   - 包含节点分页、直连置顶、设备健康以及核心指标监控在内的 12 项真实浏览器交互行为验证全部通过。

---

## 📄 开源许可证

本项目基于 [MIT License](LICENSE) 开源。
