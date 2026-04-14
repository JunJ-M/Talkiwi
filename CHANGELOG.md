# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and Talkiwi follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html) once public releases start shipping.

## [Unreleased]

### Added

- GitHub Actions workflows for CI, desktop release packaging, and GitHub Pages deployment.
- Contributor-facing issue templates, pull request template, release playbook, operations guide, compatibility guide, and install/permission guide.
- A static download and release landing page scaffold under `website/`.
- A repository readiness check script at `scripts/check-release-readiness.sh`.

### Changed

- Desktop frontend now exposes a dedicated `typecheck` script for CI and contributor workflows.
- Repository documentation now points to real install, release, and support entry points instead of placeholder release references.

## [Pre-release baseline]

### Added

- Tauri 2 desktop shell with a floating widget window and editor window.
- Rust workspace split into `talkiwi-core`, `talkiwi-asr`, `talkiwi-track`, `talkiwi-capture`, `talkiwi-engine`, `talkiwi-db`, and evaluation tooling.
- Local-first session storage, session history, structured Markdown assembly, and quality telemetry persisted to SQLite.
- Local and cloud ASR provider abstraction, Ollama-backed intent compilation, and multi-track timeline UI.
