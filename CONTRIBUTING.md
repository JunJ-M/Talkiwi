# Contributing

Talkiwi is a local-first macOS desktop product. Contributions are welcome, but the bar is product quality, not just passing compilation.

## Before You Start

- Open or reference an issue before starting significant feature work, release engineering changes, or architectural refactors.
- Keep changes scoped. Small, reviewable pull requests move faster than broad rewrites.
- If you change product behavior, update the relevant guide in `docs/` and note release impact in your PR.

## Local Setup

### Prerequisites

- macOS 13 or later
- Rust stable
- Node.js 20+ with npm 10+
- Ollama for local intent compilation

### Install dependencies

```bash
npm ci --prefix apps/desktop
```

### Run the app

```bash
npm --prefix apps/desktop run tauri -- dev
```

## Validation Checklist

Run the smallest relevant set locally before opening a PR:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm --prefix apps/desktop run typecheck
npm --prefix apps/desktop run test
npm --prefix apps/desktop run build
bash scripts/check-release-readiness.sh
```

Use the readiness script whenever you touch workflows, docs, landing page content, or release metadata.

## Design and Documentation Expectations

- Important product or architecture changes should leave a design note in `docs/design/DATE-topic.md`.
- If you add a new operator flow, permission path, or contributor step, document it in `docs/guides/`.
- Update `CHANGELOG.md` for anything user-visible, operationally relevant, or release-facing.

## Pull Request Expectations

- Explain the product intent, not just the code diff.
- Call out platform assumptions, secret requirements, and rollback risk.
- Include screenshots or short recordings for UI and onboarding changes.
- Do not merge release workflow changes without verifying the referenced secrets and guide still match.

## Branching and Versioning

- Feature work: `feature/<topic>`
- Fixes: `fix/<topic>`
- Release preparation: `release/<version>`

Tag desktop releases as `vX.Y.Z` or `vX.Y.Z-alpha.N`. The release workflow is triggered from tags, not from arbitrary branch pushes.

## Review Guidelines

- Prefer review comments that identify regressions, release risk, missing tests, or operational ambiguity.
- If a change affects install, permissions, signing, or release packaging, ask for a manual validation note before approval.
