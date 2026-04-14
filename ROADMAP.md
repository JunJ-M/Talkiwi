# Roadmap

This roadmap is a product and release planning artifact, not a promise. The goal is to keep Talkiwi shipping in coherent milestones instead of accumulating half-finished surface area.

## Release Channels

- `alpha`: fast iteration, schema and onboarding can still change, release notes must call out operational risk.
- `beta`: broader testing, installer and permission recovery should be stable, rollback path must be documented.
- `stable`: signed and notarized desktop builds, repeatable release checklist, support docs and smoke tests are mandatory.

## Milestones

| Milestone | Goal | Exit Criteria |
| --- | --- | --- |
| `0.1.x alpha` | Public desktop alpha for local-first voice context capture | Signed DMG, install guide, permission recovery guide, changelog, release notes, basic smoke test |
| `0.2.x beta` | Harden capture reliability and first-run onboarding | Permission verification is no longer stubbed, crash export exists, compatibility matrix validated |
| `0.3.x beta` | Provider polish and trust surface | Better provider setup UX, telemetry review dashboard, support bundle export |
| `1.0.0` | Stable macOS desktop release | Release process is repeatable, rollback is proven, onboarding and session recovery are production-grade |

## Near-term Priorities

### Release Engineering

- Ship the first tagged desktop release from GitHub Actions.
- Lock signing and notarization secrets into documented environment contracts.
- Keep `CHANGELOG.md`, release notes, and landing page copy aligned.

### Distribution

- Add first public DMG to GitHub Releases.
- Replace placeholder landing-page copy with real screenshots, GIFs, and a 30-45 second demo clip.
- Validate Apple Silicon and Intel install paths on clean macOS machines.

### Reliability and Operations

- Replace permission-status stubs with real status detection.
- Add a user-facing diagnostics export flow.
- Add startup, permission, and capture failure smoke tests to the release checklist.

## Not Planned Yet

- Windows release channel
- Team cloud sync
- Hosted backend or web workspace

Those may happen later, but they are explicitly not gates for the first public macOS alpha.
