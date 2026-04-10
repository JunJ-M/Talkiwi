---
name: commit-craft
description: Use when preparing a git commit that may contain mixed intent, unclear scope, or a weak commit message, especially when the agent needs to decide whether changes should be split and produce a concise English message that explains why the change exists.
---

# Commit Craft

Use this skill to turn raw working tree changes into a reviewable commit plan.

Prioritize deterministic inspection with scripts. Use judgment only for intent boundaries and trade-offs that cannot be derived mechanically.

## When to Use

- A commit may mix multiple intents such as feature work, refactor, formatting, or fixes
- The right commit message is unclear from filenames alone
- The agent needs to explain why a change exists instead of listing files touched
- The repository has no explicit commit helper, but high signal commits still matter

## Never Do

- Never create one commit just because the files were edited together
- Never group formatting with functional changes unless the formatting is required for the change to work
- Never group refactors with bug fixes when the refactor can be reviewed independently
- Never write commit subjects as file lists, implementation diaries, or vague updates like `update stuff`
- Never let the prompt guess deterministic facts that scripts can compute from git

## Workflow

1. Run `scripts/analyze_changes.py` to extract a structured view of staged or working-tree changes.
2. Run `scripts/suggest_commit_groups.py` to detect likely split boundaries.
3. Use `references/expert_rules.md` to decide whether split suggestions reflect real intent boundaries.
4. Draft a commit message that states the primary intent in the subject and the reason or trade-off in the body.
5. Run `scripts/lint_commit_message.py` before finalizing the message.

## Judgment Rules

- Split by intent, not by file extension
- Keep generated rename noise out of the message unless it changes behavior or workflow
- If one change only exists to enable another, keep them together and explain the dependency in the body
- If a reviewer could reasonably accept one part and reject another, they likely belong in separate commits

## References

- Load `references/expert_rules.md` for commit boundary heuristics and message strategy
- Load `references/anti_patterns.md` for failure modes and examples that should be rejected

## Scripts

- `scripts/analyze_changes.py`: summarize git changes into structured JSON
- `scripts/suggest_commit_groups.py`: propose review-oriented commit groupings
- `scripts/lint_commit_message.py`: validate commit message quality and common anti-patterns
