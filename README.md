<div align="center">

# be6500panel

**小米路由器 BE6500 (RN02) 控制面板**

轻量 Rust 后端服务与现代化 Web 管理界面

<p align="center">
  <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-1.93+-DEA584?style=flat-square&logo=rust&logoColor=white" alt="Rust 1.93+" /></a>
  <a href="https://react.dev/"><img src="https://img.shields.io/badge/React-19.2-61DAFB?style=flat-square&logo=react&logoColor=black" alt="React 19" /></a>
  <img src="https://img.shields.io/badge/Target-Xiaomi%20BE6500%20%28RN02%29-E05638?style=flat-square" alt="Target Hardware" />
  <img src="https://img.shields.io/badge/Architecture-ARMv7%20musl-555555?style=flat-square" alt="ARMv7 musl" />
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-green?style=flat-square" alt="MIT License" /></a>
</p>

<p align="center">
  <a href="#核心功能">核心功能</a> •
  <a href="#系统架构">系统架构</a> •
  <a href="#界面预览">界面预览</a> •
  <a href="#配置修改与回滚保护">配置工作流</a> •
  <a href="#本地开发与构建">本地开发</a> •
  <a href="#部署与运行规格">部署规格</a>
</p>

<br />

<img src="docs/images/overview-dashboard.png" alt="be6500panel 概览页面" width="960" />

</div>

---

## 项目简介

be6500panel 是专为小米路由器 BE6500（RN02）打造的管理面板，用于替代简陋的原厂后台，提供清晰的系统状态监控、透明代理分流规则管理、网络与设备查看及后台服务运维功能。

系统包含两部分：

- **后端服务 (Rust)**：轻量常驻进程，负责读取系统状态、管理 UCI 配置与守护后台进程（sing-box、frpc），实测内存占用约 5–7 MiB。
- **前端界面 (React 19)**：基于 Tailwind CSS、Radix UI 与 ECharts 构建的 Web 页面，静态文件由 Rust 服务直接托管，路由器无需安装 Node.js。

---

## 核心功能

- **路由代理与规则管理**：
  - 本地解析节点订阅，不上传节点信息；
  - 支持 VLESS、REALITY、Vision、uTLS 等常见协议；
  - 自定义规则编辑器：支持精确域名直连、订阅规则按内容指纹保留改写；
  - 基于 routed-TUN 虚拟网卡接管流量。
- **网络与设备状态**：
  - 统计 2.5G 网口双向实时速率；
  - 基于 DHCP 租约与 ARP 记录查看局域网已连接设备及 IP-MAC 对应关系；
  - 查看 IPv4 / IPv6 系统路由表与防火墙状态。
- **系统与后台服务**：
  - 查看 CPU 负载（Load Average）、内存占用与 WAN 流量历史；
  - 管理后台系统服务启动、停止与重启；
  - 原生监管 frpc 进程，支持本地 TOML 配置文件。
- **配置防失联保护**：
  - 采用 `编辑 → 暂存 → 差异比对 → 提交` 流程；
  - 关键网络变更如果超时未确认，自动恢复上一代配置，避免断网失联。

---

## 系统架构

<div align="center">
  <img src="docs/images/architecture.svg" alt="be6500panel 系统架构" width="900" />
</div>

- **前端界面**：静态网页资源，由 Rust 服务直接托管访问；
- **接口与通信**：使用带认证的 HTTP API，配合 `/api/events`（SSE）向前端推送系统状态；
- **后端服务**：读取系统文件（procfs / sysfs / UCI）、处理配置变更，并监管 sing-box 和 frpc 进程。

---

## 界面预览

<table align="center" width="100%">
  <tr>
    <td width="50%" align="center">
      <a href="docs/images/overview-dashboard.png">
        <img src="docs/images/overview-dashboard.png" alt="概览页面" width="100%" />
      </a>
      <br />
      <b>概览页面 (<code>/#/overview</code>)</b>
      <p align="left">展示系统平均负载、内存占用、WAN 流量趋势、设备活跃热力图与服务状态。</p>
    </td>
    <td width="50%" align="center">
      <a href="docs/images/proxy-rules-editor.png">
        <img src="docs/images/proxy-rules-editor.png" alt="代理与规则管理" width="100%" />
      </a>
      <br />
      <b>代理与规则管理 (<code>/#/proxy</code>)</b>
      <p align="left">节点选择、精确域名直连、订阅规则改写、草稿保存、合并规则预览与一次性生效。</p>
    </td>
  </tr>
  <tr>
    <td colspan="2" align="center">
      <a href="docs/images/network-topology.png">
        <img src="docs/images/network-topology.png" alt="网络与设备" width="90%" />
      </a>
      <br />
      <b>网络与设备 (<code>/#/network</code>)</b>
      <p align="center">网口物理状态与速率统计、系统路由表与局域网设备列表。</p>
    </td>
  </tr>
</table>

---

## 配置修改与回滚保护

<div align="center">
  <img src="docs/images/config-workflow.svg" alt="be6500panel 配置管理工作流" width="880" />
</div>

为防止改错配置导致断网，系统采用分阶段配置修改流程：

1. **草稿隔离**：在未点击应用前，所有修改仅保存在草稿中，不写入路由器实际系统；
2. **差异比对**：提供行级差异对比（Diff），方便核对修改内容；
3. **超时保护**：应用可能影响网络连接的修改时启动倒计时，若超时未确认，系统自动恢复上一代配置。

---

## 本地开发与构建

项目采用 **Rust + Bun** 组织，结构简单直接。

### 环境准备

- **Rust**：1.93 或更高版本（包含 `cargo`）
- **Bun**：1.0+（前端依赖管理与脚本执行）
- **Node.js**：20.20+（开发环境兼容）

### 常用命令

```bash
# 1. 安装前端依赖
make setup

# 2. 启动本地开发服务（回环只读模式，监听 127.0.0.1:8790）
make dev-api

# 3. 另起终端启动前端开发服务器（带热重载）
make dev-web

# 4. 执行全套测试（前端单测与类型检查、Rust 单元与集成测试）
make test

# 5. 构建本地运行二进制与前端静态产物
make build

# 6. 交叉编译目标路由器平台产物（ARMv7 musl 静态二进制）
make armv7
```

### 原生 Cargo 命令

```bash
# 运行全部 Rust 单元测试与代码检查
cargo test --locked
cargo fmt --check
cargo clippy --locked -- -D warnings

# 构建优化后的发布版本
cargo build --release --locked
```

---

## 目录结构

```text
be6500panel/
├── Cargo.toml          # Rust 项目配置
├── Cargo.lock          # 依赖版本锁定
├── Makefile            # 开发、测试、构建与交叉编译指令
├── src/                # 后端服务源码 (Rust)
│   ├── main.rs         # 入口与参数解析
│   ├── server.rs       # HTTP / SSE 服务与路由
│   ├── product_*.rs    # 系统状态、配置、网络与代理逻辑
│   └── capture_*.rs    # routed-TUN 流量接管逻辑
├── web/                # 前端页面源码 (React 19 / Vite / Tailwind)
│   ├── src/app/        # 页面路由与主框架
│   ├── src/modules/    # 功能模块（代理、网络、系统、设备、FRPC）
│   ├── src/components/ # 通用组件与图表
│   └── src/lib/        # API 请求与类型定义
├── tests/              # 测试用例
│   └── fixtures/       # 测试基准数据
├── examples/           # 独立测试工具与示例
└── docs/               # 文档与配图
    └── images/         # 架构图与界面截图
```

---

## 部署与运行规格

| 项目 | 说明 |
| :--- | :--- |
| **适用设备** | 小米路由器 BE6500（RN02），高通 IPQ5322 四核 1.5GHz |
| **系统环境** | 原厂 QSDK / OpenWrt Linux（内核 5.4） |
| **运行文件** | `/tmp/be6500panel/boot-release/be6500-panel`（同级放置 `web/` 静态目录） |
| **持久存储** | `/data/be6500panel/panel.tar.gz`（持久包与配置备份） |
| **内存占用** | 后端服务实测驻留内存（RSS）约 5–7 MiB（不含 sing-box 独立核心） |
| **验证状态** | 23 个核心视图通过零告警验证，12 项浏览器自动化交互测试全部通过 |

---

## 开源许可

本项目采用 [MIT 许可证](LICENSE)。
