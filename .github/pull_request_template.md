## Summary

- what changed
- why it changed
- release or migration impact

## Validation

- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `npm --prefix apps/desktop run typecheck`
- [ ] `npm --prefix apps/desktop run test`
- [ ] `bash scripts/check-release-readiness.sh` (if docs, workflows, or release metadata changed)

## Release Notes

- user-facing changes:
- permissions / config changes:
- rollback considerations:

## Screenshots or Recordings

Add before/after visuals for UI, onboarding, permissions, or release-page changes.
