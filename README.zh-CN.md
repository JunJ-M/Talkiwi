<p align="center">
  <picture>
    <img src="https://raw.githubusercontent.com/JunJ-M/Talkiwi/main/assets/kiwi-sun.png" alt="Talkiwi" width="180">
  </picture>
</p>

<h1 align="center">Talkiwi</h1>

<p align="center">
  <strong>面向 AI 工作流的开源多轨道语音上下文编译器</strong><br/>
  <sub>将语音 + 操作 + 选中内容 + 截图 转化为 AI 友好的结构化提示词</sub>
</p>

<p align="center">
  <!-- Build -->
  <a href="https://github.com/JunJ-M/Talkiwi/actions/workflows/ci.yml">
    <img src="https://img.shields.io/github/actions/workflow/status/JunJ-M/Talkiwi/ci.yml?branch=main&label=CI&style=flat-square&logo=github" alt="CI 状态">
  </a>
  <!-- License -->
  <a href="./LICENSE">
    <img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue?style=flat-square" alt="许可证">
  </a>
  <!-- Version -->
  <a href="https://github.com/JunJ-M/Talkiwi/releases">
    <img src="https://img.shields.io/github/v/release/JunJ-M/Talkiwi?style=flat-square&color=orange&label=release" alt="最新版本">
  </a>
  <!-- Platform -->
  <img src="https://img.shields.io/badge/platform-macOS-lightgrey?style=flat-square&logo=apple" alt="平台">
  <!-- Rust -->
  <img src="https://img.shields.io/badge/rust-1.78%2B-orange?style=flat-square&logo=rust" alt="Rust 版本">
  <!-- Stars -->
  <a href="https://github.com/JunJ-M/Talkiwi/stargazers">
    <img src="https://img.shields.io/github/stars/JunJ-M/Talkiwi?style=flat-square&color=yellow" alt="Stars">
  </a>
  <!-- PRs welcome -->
  <a href="https://github.com/JunJ-M/Talkiwi/pulls">
    <img src="https://img.shields.io/badge/PRs-welcome-brightgreen?style=flat-square" alt="欢迎 PR">
  </a>
</p>

<p align="center">
  <a href="./README.md">English</a> ·
  <a href="./README.zh-CN.md">简体中文</a> ·
  <a href="./README.ja.md">日本語</a>
</p>

---

## Talkiwi 是什么？

**Talkiwi** 不是一个语音转文字工具。它是一个**语音上下文编译器** —— 一个 macOS 桌面侧边栏应用，同时监听你所_说的话_和你所_做的操作_，然后将两者组装成一份结构化的 AI 友好 Markdown 文档，供你粘贴到任何 LLM 中使用。

> **关键区别：** Talkiwi **不会**将任何内容发送给 AI。  
> 它生成一份结构化的提示词文档 —— 你来决定将它发送到哪里。

### 它解决了什么问题？

| 缺口 | 示例 |
|------|------|
| 缺乏上下文的语音 | 你说"修一下这个" —— 模型根本不知道"这个"是什么 |
| 缺乏操作证据的转录 | 你刚才选中了代码、截了图、开了个 Issue —— 这些信息都没有传给模型 |
| 缺乏重组的原始口语 | 人类口语中有大量语气词、代词跳转和重复纠正 —— 不适合直接喂给 LLM |
| 封闭的上下文模型 | 编程、写作和研究需要完全不同的上下文轨道 |

### 它输出什么？

```markdown
## 任务
为选中的函数添加缓存。调查该错误是否与重试逻辑有关。

## 用户意图
代码修改 + 错误排查

## 上下文
### 选中的代码
[选区中的函数源码]

### 错误截图
[带有 OCR 文本的截图]

### 引用的 Issue
[Issue URL + 标题]

### 环境信息
- 仓库: my-project
- 文件: src/utils/fetcher.ts:42

## 期望输出
1. 错误的根本原因分析
2. 缓存实现建议
3. 代码补丁
```

---

## 功能特性（V1 Alpha）

| # | 功能 | 归属轨道 |
|---|------|---------|
| 1 | 小组件按钮开始/停止捕获 | 核心 |
| 2 | 通过 Whisper 实现本地 ASR（whisper.cpp / mlx-whisper） | 语音 |
| 3 | 云端 ASR 选项（Deepgram / OpenAI Whisper API） | 语音 |
| 4 | 选中文本注入 | 产物 |
| 5 | 应用内截图工具（支持区域选择） | 产物 |
| 6 | 当前 URL + 页面标题注入 | 产物 |
| 7 | 剪贴板内容注入 | 产物 |
| 8 | 文件拖入注入 | 产物 |
| 9 | 意图编译器（默认本地 LLM，云端可选） | 核心 |
| 10 | **自动代词解析**（"这个"/"那个" → 最近的产物） | 核心 |
| 11 | 结构化 Markdown 输出生成 | 核心 |
| 12 | 可折叠常驻侧边栏面板 | UI |
| 13 | 多轨道时间线查看器 | UI |
| 14 | 一键复制到剪贴板 | UI |
| 15 | 会话自动保存到本地文件 | 存储 |
| 16 | 会话历史浏览器 | 存储 |
| 17 | Provider 设置（本地 ↔ 云端切换） | 设置 |

---

## 架构设计

Talkiwi 基于 **Tauri 2.0** 构建，采用 Rust 后端和 React 前端。

```
┌─────────────────────────────────────────────────┐
│                  捕获层 (Capture Layer)          │
│   语音 │ 操作 │  产物  │ 轨迹 │ 插件            │
│        └──────┴────────┴──────┘                 │
│              时间轴对齐 (Timeline Alignment)      │
│              事件规范化 (Event Normalization)    │
│              去重与压缩 (Dedup & Compression)    │
└──────────────────────┬──────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────┐
│              意图编译器 (Intent Compiler)        │
│  • 移除口头禅/语气词                             │
│  • 补全省略的主语/宾语                           │
│  • 解析"这个"/"那个" → 对应的实际产物            │
│  • 将多轮语音合并为清晰的任务                    │
│  • 控制最终 Token 预算                           │
└──────────────────────┬──────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────┐
│             提示词组装器 (Prompt Assembler)      │
│  生成结构化的 AI 友好 Markdown                   │
│  → 侧边栏预览 + 本地文件保存                     │
└─────────────────────────────────────────────────┘
```

### 技术栈

| 层级 | 选型 | 理由 |
|------|------|------|
| 应用壳 | **Tauri 2.0**（Rust + WebView） | 包体约 10 MB，可访问 macOS 原生 API，性能优秀 |
| 前端 | React + TypeScript | 迭代快，时间轴/Markdown 组件生态丰富 |
| ASR | whisper.cpp / mlx-whisper | 本地优先，针对 Apple Silicon 优化 |
| 意图编译器 | Ollama + 小型模型（Qwen2.5-7B、Phi-3） | 本地优先，Provider 可切换 |
| 存储 | SQLite（rusqlite） | 会话历史和事件存储 |
| IPC | Tauri commands + 事件系统 | Rust ↔ 前端通信 |

### Crate 结构

```
crates/
├── talkiwi-core        # 共享类型和事件 Schema
├── talkiwi-track       # 双轨时间线管理
├── talkiwi-capture     # 内置操作捕获器（选区、截图、剪贴板…）
├── talkiwi-engine      # 指代解析 + 意图编译 + MD 组装
├── talkiwi-asr         # ASR 抽象层（封装 transcribe-rs）
└── talkiwi-db          # SQLite 持久化层
```

---

## 快速开始

### 环境要求

- macOS 13 Ventura 或更高版本
- Rust 1.78+（`rustup install stable`）
- Node.js 20+ 和 pnpm（`npm i -g pnpm`）
- [Ollama](https://ollama.ai/)（用于本地意图编译）

### 安装步骤

```bash
# 1. 克隆仓库
git clone https://github.com/JunJ-M/Talkiwi.git
cd Talkiwi

# 2. 安装前端依赖
pnpm install

# 3. 拉取本地意图编译模型
ollama pull qwen2.5:7b

# 4. 以开发模式运行
pnpm tauri dev
```

### 构建正式版 DMG

```bash
pnpm tauri build
# 输出位置：apps/desktop/src-tauri/target/release/bundle/dmg/
```

### macOS 权限说明

Talkiwi 在首次启动时需要以下 macOS 权限：

| 权限 | 用途 |
|------|------|
| 麦克风 | 语音捕获 |
| 屏幕录制 | 截图工具 |
| 辅助功能 | 文本选区捕获 |
| 自动化（可选） | 浏览器 URL 检测 |

每项权限仅在首次使用时请求 —— Talkiwi 会通过引导向导带你完成设置。

---

## Provider 配置

Talkiwi 使用**可插拔的 Provider 注册表** —— 你可以随时在设置中切换本地或云端。

```
Provider 接口
├── ASR Provider
│   ├── 本地 Whisper（默认，通过 whisper.cpp / mlx-whisper）
│   ├── macOS Speech 框架
│   └── 云端：Deepgram / AssemblyAI / OpenAI Whisper API
│
└── 意图编译器 Provider
    ├── 本地：Ollama（默认，如 Qwen2.5-7B、Phi-3）
    └── 云端：Claude API / OpenAI API
```

所有默认 Provider 均**完全在本地运行**。云端 Provider 需要 API Key 并进行明确授权。

---

## 隐私说明

Talkiwi 在设计上**以本地为优先**：

- 默认所有处理均在本地进行 —— 除非你明确启用了云端 Provider，否则不会有任何数据离开你的设备
- 分轨道细粒度授权（截图、剪贴板、辅助功能）
- 使用任何云端 Provider 均需明确同意
- 会话级**"不录制"**模式
- 未经许可不收集任何遥测数据

---

## 路线图

| 版本 | 范围 |
|------|------|
| **V1 Alpha**（当前） | 核心捕获、ASR、意图编译、侧边栏 UI、会话历史 |
| **V1.5** | 带有轨道声明 API 的插件 SDK、IDE / 终端 / Git 插件、提示词模板 |
| **V2** | 环境感知（持续捕获）模式、自动场景识别、跨会话智能召回、团队协作 |

---

## 贡献指南

欢迎贡献代码！请在开启 Pull Request 之前阅读 [贡献指南](./CONTRIBUTING.md)。

```bash
# 运行 Rust 测试
cargo test --workspace

# 运行前端测试
pnpm --filter desktop test

# 代码检查
cargo clippy --workspace
pnpm --filter desktop lint
```

如果计划实现一个重要功能或进行架构调整，请先开 Issue 进行讨论。

---

## 许可证

Talkiwi 采用 **MIT** 和 **Apache 2.0** 双重许可证，你可以选择任意一种许可证使用本项目。

详见 [LICENSE](./LICENSE)。

---

<p align="center">由 Talkiwi 团队用 ☕ 制作 · <a href="https://github.com/JunJ-M/Talkiwi/issues">提交一个 Issue</a></p>
