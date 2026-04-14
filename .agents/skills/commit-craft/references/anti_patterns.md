# Anti-Patterns

## Bad Commit Boundaries

- Feature code plus opportunistic cleanup in unrelated modules
- Bug fix plus mass formatting in the same diff
- Rename plus logic rewrite with no explanation of coupling
- Adding tests for one bug while also changing unrelated fixtures

## Bad Subjects

- `update code`
- `fix files`
- `changes`
- `misc cleanup`
- `wip`

These fail because they hide intent and force reviewers to inspect the diff to learn basic context.

## Bad Bodies

- Repeating the subject in longer words
- Listing touched files instead of reasons
- Narrating implementation steps with no reviewer value
- Omitting why two different-looking edits must stay together

## False Signals

- A directory boundary is not an intent boundary
- A large diff is not automatically multiple commits
- A small diff is not automatically one coherent commit
- Passing tests do not prove the commit is reviewable
