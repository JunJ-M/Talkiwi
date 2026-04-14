<p align="center">
  <picture>
    <img src="https://raw.githubusercontent.com/JunJ-M/Talkiwi/main/assets/kiwi-sun.png" alt="Talkiwi" width="380">
  </picture>
</p>

<h1 align="center">Talkiwi</h1>

<p align="center">
  <strong>Open-source, multi-track voice context compiler for AI workflows</strong><br/>
  <sub>Turns speech + actions + selections + screenshots into AI-ready structured prompts</sub>
</p>

<p align="center">
  <!-- Build -->
  <a href="https://github.com/JunJ-M/Talkiwi/actions/workflows/ci.yml">
    <img src="https://img.shields.io/github/actions/workflow/status/JunJ-M/Talkiwi/ci.yml?branch=main&label=CI&style=flat-square&logo=github" alt="CI Status">
  </a>
  <!-- License -->
  <a href="./LICENSE">
    <img src="https://img.shields.io/badge/license-MIT%20%2F%20Apache--2.0-blue?style=flat-square" alt="License">
  </a>
  <!-- Version -->
  <a href="https://github.com/JunJ-M/Talkiwi/releases">
    <img src="https://img.shields.io/github/v/release/JunJ-M/Talkiwi?style=flat-square&color=orange&label=release" alt="Latest Release">
  </a>
  <!-- Platform -->
  <img src="https://img.shields.io/badge/platform-macOS-lightgrey?style=flat-square&logo=apple" alt="Platform">
  <!-- Rust -->
  <img src="https://img.shields.io/badge/rust-1.78%2B-orange?style=flat-square&logo=rust" alt="Rust Version">
  <!-- Stars -->
  <a href="https://github.com/JunJ-M/Talkiwi/stargazers">
    <img src="https://img.shields.io/github/stars/JunJ-M/Talkiwi?style=flat-square&color=yellow" alt="Stars">
  </a>
  <!-- PRs welcome -->
  <a href="https://github.com/JunJ-M/Talkiwi/pulls">
    <img src="https://img.shields.io/badge/PRs-welcome-brightgreen?style=flat-square" alt="PRs Welcome">
  </a>
</p>

<p align="center">
  <a href="./README.md">English</a> ·
  <a href="./README.zh-CN.md">简体中文</a> ·
  <a href="./README.ja.md">日本語</a>
</p>

---

## What is Talkiwi?

**Talkiwi** is not a voice-to-text app. It is a **voice context compiler** — a macOS desktop sidepanel that listens to what you _say_ and watches what you _do_, then assembles both into a single, AI-ready structured Markdown document you can paste into any LLM.

> **Critical distinction:** Talkiwi does **not** send anything to an AI.  
> It produces a structured prompt document — you decide where it goes.

### The problem it solves

| Gap                                   | Example                                                                              |
| ------------------------------------- | ------------------------------------------------------------------------------------ |
| Speech without context                | You say _"fix this"_ — the model has no idea what _"this"_ is                        |
| Transcription without action evidence | You selected code, took a screenshot, opened an issue — none of it reaches the model |
| Raw speech without restructuring      | Human speech has fillers, pronoun jumps, and restarts — not suitable for LLMs        |
| Closed context model                  | Coding, writing, and research need completely different context tracks               |

### What it produces

```markdown
## Task
Add caching to the selected function. Investigate whether the error is related to retry logic.

## User Intent
Code modification + bug investigation

## Context
### Selected Code
[function source from selection]

### Error Screenshot
[screenshot with OCR text]

### Referenced Issue
[issue URL + title]

### Environment
- Repository: my-project
- File: src/utils/fetcher.ts:42

## Requested Output
1. Root cause analysis of the error
2. Caching implementation suggestion
3. Code patch
```

---

## Features (V1 Alpha)

| #   | Feature                                                                 | Track    |
| --- | ----------------------------------------------------------------------- | -------- |
| 1   | Widget button to start/stop capture                                     | Core     |
| 2   | Local ASR via Whisper (whisper.cpp / mlx-whisper)                       | Speech   |
| 3   | Cloud ASR option (Deepgram / OpenAI Whisper API)                        | Speech   |
| 4   | Selected text injection                                                 | Artifact |
| 5   | In-app screenshot tool with region selection                            | Artifact |
| 6   | Current URL + page title injection                                      | Artifact |
| 7   | Clipboard content injection                                             | Artifact |
| 8   | File drag-in injection                                                  | Artifact |
| 9   | Intent compiler (local LLM default, cloud optional)                     | Core     |
| 10  | **Automatic pronoun resolution** (_"this"_/_"that"_ → nearest artifact) | Core     |
| 11  | Structured Markdown output generation                                   | Core     |
| 12  | Collapsible always-on sidebar panel                                     | UI       |
| 13  | Multi-track timeline viewer                                             | UI       |
| 14  | One-click copy to clipboard                                             | UI       |
| 15  | Auto-save session to local file                                         | Storage  |
| 16  | Session history browser                                                 | Storage  |
| 17  | Provider settings (local ↔ cloud switcher)                              | Settings |

---

## Architecture

Talkiwi is built as a **Tauri 2.0** app with a Rust backend and React frontend.

```
┌─────────────────────────────────────────────────┐
│                  Capture Layer                    │
│   Speech │ Action │ Artifact │ Trace │ Plugin    │
│          └────────┴──────────┴───────┘           │
│              Timeline Alignment                   │
│              Event Normalization                  │
│              Dedup & Compression                  │
└──────────────────────┬──────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────┐
│              Intent Compiler                      │
│  • Remove speech fillers                          │
│  • Restore omitted subjects/objects               │
│  • Resolve "this"/"that" → actual artifacts       │
│  • Merge multi-turn speech into a clear task      │
│  • Control final token budget                     │
└──────────────────────┬──────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────┐
│             Prompt Assembler                      │
│  Produces AI-friendly structured Markdown         │
│  → Sidebar preview + local file save             │
└─────────────────────────────────────────────────┘
```

### Tech Stack

| Layer           | Choice                                   | Rationale                                           |
| --------------- | ---------------------------------------- | --------------------------------------------------- |
| App shell       | **Tauri 2.0** (Rust + WebView)           | ~10 MB bundle, native macOS APIs, great performance |
| Frontend        | React + TypeScript                       | Fast iteration, rich ecosystem                      |
| ASR             | whisper.cpp / mlx-whisper                | Local-first, Apple Silicon optimized                |
| Intent Compiler | Ollama + small model (Qwen2.5-7B, Phi-3) | Local-first, provider-switchable                    |
| Storage         | SQLite (rusqlite)                        | Session history and event storage                   |
| IPC             | Tauri commands + event system            | Rust ↔ frontend communication                       |

### Crate Layout

```
crates/
├── talkiwi-core        # Shared types and event schema
├── talkiwi-track       # Dual-track timeline management
├── talkiwi-capture     # Built-in action captors (selection, screenshot, clipboard…)
├── talkiwi-engine      # Reference resolver + intent compiler + MD assembler
├── talkiwi-asr         # ASR abstraction layer (wraps transcribe-rs)
└── talkiwi-db          # SQLite persistence layer
```

---

## Getting Started

### Prerequisites

- macOS 13 Ventura or later
- Rust 1.78+ (`rustup install stable`)
- Node.js 20+ and npm 10+
- [Ollama](https://ollama.ai/) (for local intent compilation)

### Installation

```bash
# 1. Clone the repository
git clone https://github.com/JunJ-M/Talkiwi.git
cd Talkiwi

# 2. Install frontend dependencies
npm ci --prefix apps/desktop

# 3. Pull a local model for the intent compiler
ollama pull qwen2.5:7b

# 4. Run in development mode
npm --prefix apps/desktop run tauri -- dev
```

### Build a release DMG

```bash
npm --prefix apps/desktop run tauri -- build
# Output: apps/desktop/src-tauri/target/release/bundle/dmg/
```

## Release & Operations

The repository now includes the release and distribution scaffold needed to ship a public desktop alpha.

- [Download page](./website/index.html)
- [GitHub Pages site](https://junj-m.github.io/Talkiwi/) _(after enabling Pages in repository settings)_
- [Site source](./website/index.html)
- [GitHub Releases](https://github.com/JunJ-M/Talkiwi/releases)
- [Changelog](./CHANGELOG.md)
- [Contributing Guide](./CONTRIBUTING.md)
- [Roadmap](./ROADMAP.md)
- [Installation & Permissions](./docs/guides/installation-and-permissions.md)
- [Release Playbook](./docs/guides/release-playbook.md)
- [Compatibility & Support](./docs/guides/compatibility-and-support.md)
- [Operations & Observability](./docs/guides/operations-and-observability.md)
- [Launch Asset Checklist](./docs/guides/launch-assets-checklist.md)

### macOS Permissions

Talkiwi requires the following macOS permissions on first launch:

| Permission            | Used for               |
| --------------------- | ---------------------- |
| Microphone            | Voice capture          |
| Screen Recording      | Screenshot tool        |
| Accessibility         | Text selection capture |
| Automation (optional) | Browser URL detection  |

Each permission is requested on first use — Talkiwi will guide you through the setup wizard.

---

## Provider Configuration

Talkiwi uses a **pluggable provider registry** — you can switch between local and cloud at any time in settings.

```
Provider Interface
├── ASR Provider
│   ├── Local Whisper (default, via whisper.cpp / mlx-whisper)
│   ├── macOS Speech Framework
│   └── Cloud: Deepgram / AssemblyAI / OpenAI Whisper API
│
└── Intent Compiler Provider
    ├── Local: Ollama (default, e.g. Qwen2.5-7B, Phi-3)
    └── Cloud: Claude API / OpenAI API
```

All default providers run **fully locally**. Cloud providers require an API key and explicit opt-in.

---

## Privacy

Talkiwi is **local-first by design**:

- All processing runs locally by default — nothing leaves your machine unless you explicitly enable a cloud provider
- Per-track permission grants (screenshot, clipboard, accessibility)
- Explicit consent required for any cloud provider
- Session-level **"Do not record"** mode
- No telemetry without opt-in

---

## Roadmap

| Version                  | Scope                                                                                       |
| ------------------------ | ------------------------------------------------------------------------------------------- |
| **V1 Alpha** _(current)_ | Core capture, ASR, intent compiler, sidebar UI, session history                             |
| **V1.5**                 | Plugin SDK with track declaration API, IDE/Terminal/Git plugins, prompt templates           |
| **V2**                   | Ambient (continuous) mode, auto scene recognition, cross-session recall, team collaboration |

---

## Contributing

Contributions are welcome! Please read the [Contributing Guide](./CONTRIBUTING.md) before opening a pull request.

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

npm --prefix apps/desktop run typecheck
npm --prefix apps/desktop run test
npm --prefix apps/desktop run build
bash scripts/check-release-readiness.sh
```

Please open an issue before starting on a significant feature or architectural change.

---

## License

Talkiwi is dual-licensed under **MIT** and **Apache 2.0**. You may use it under either license.

See [LICENSE](./LICENSE) for details.

---

<p align="center">Made with ☕ by the Talkiwi team · <a href="https://github.com/JunJ-M/Talkiwi/issues">Report an issue</a></p>
