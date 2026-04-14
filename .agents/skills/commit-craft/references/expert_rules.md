# Expert Rules

## High-Signal Commit Boundaries

The default unit is not a file and not a command. The unit is reviewer intent.

Use separate commits when:

- A code movement or rename can be reviewed independently from behavioral change
- A refactor changes structure while a fix changes outcome
- Test rewrites change harness shape while product code changes runtime behavior
- Tooling or formatting churn obscures the semantic diff of the real change

Keep changes together when:

- One change exists only to enable the other
- The intermediate state would not build, test, or make sense to review
- The supporting change is too small to justify a separate commit and has no independent value

## Message Strategy

The subject line names the intent. The body explains the reason.

Good subject properties:

- Uses a strong action verb such as `add`, `fix`, `refactor`, `remove`, `rename`, `tighten`
- Mentions the product or subsystem, not implementation trivia
- Stays concise enough to scan in `git log --oneline`

Good body properties:

- Explains why the change is needed
- Notes trade-offs or constraints if they shaped the implementation
- Mentions notable scope boundaries when helpful to reviewers

## Message Construction Heuristic

Use this shape:

`<type>: <intent>`

Optional body:

- Why now
- What reviewer should pay attention to
- Why this is one commit instead of multiple commits

## Recommended Types

- `feat`: new user-visible capability
- `fix`: bug correction or correctness improvement
- `refactor`: structural change without intended behavior change
- `test`: test-only improvement or coverage addition
- `docs`: documentation-only change
- `build`: tooling, dependency, or build pipeline change
- `chore`: maintenance work with low product semantics

## Trade-offs Worth Calling Out

- Why two related edits stay in one commit
- Why a broader refactor was intentionally deferred
- Why naming or structure changed to support later work
- Why a workaround is accepted temporarily
