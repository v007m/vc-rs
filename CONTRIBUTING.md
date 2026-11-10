# Contributing to Vibranium v2

Read `CLAUDE.md` and `docs/design/HANDOVER.md` first. The design docs in `docs/design/` are the specification.

## Setup

1. Install [rustup](https://rustup.rs) and [mise](https://mise.jdx.dev).
2. In the repo: `rustup toolchain install` (reads `rust-toolchain.toml`), then `mise trust && mise install` (cargo tools, node, pnpm), then `pnpm install`.
3. Check everything: `mise run gates`.

## Tasks

| Command | What it does |
| --- | --- |
| `mise run gates` | Every quality gate, stopping at the first failure (`scripts/gates.sh`). `GATES_MUTANTS=1` adds mutation testing on your diff |
| `mise run test` | Tests and doctests only |
| `mise run commit-check` | Checks the commits on your branch against `origin/master` |
| `mise run gates:selftest` | Checks that each gate still rejects its bad patch in `tests/gate-fixtures/` |

Each gate prints its ID (`GATE-FMT`, `GATE-CLIPPY`, `GATE-ARCH`, `GATE-DENY`, `GATE-JSLICENSE`, `GATE-PROTO`, `GATE-MACHETE`, `GATE-DUP`, `GATE-TEST`, `GATE-MUTANTS`, `GATE-COMMIT`, `GATE-REFACTOR`) and `FAILED <id>` when it fails. The rules behind them are in the LLD's "Design rules and quality gates". Gate files are owned by the maintainer (`.github/CODEOWNERS`); if a gate looks wrong, ask instead of changing it.

## Branches, commits and PRs

- `master` is the integration branch and is protected. Each session works on one long-lived `s/<name>` branch and merges `master` into it to stay current.
- Keep PRs small: under about 400 changed lines. Every PR passes `mise run gates` and a code review before it merges.
- Commit subjects are conventional commits: `feat|fix|refactor|test|docs|chore|perf|build|ci`, an optional `(scope)` and `!`, then `: subject`.
- Tidy first: `refactor:` commits never change test assertions. CI rejects one that touches a test file or an `assert` line.
- Write the milestone's acceptance tests first, then work inward.
- Update the design docs a change touches in the same PR. A change to a decision in `decision-log.md` is asked, not decided in code.

## Review checklist

No tool catches these smells, so every PR description answers them (the PR template lists them):

1. **Naming**: names say what things are and do; test IDs are third-person verbs.
2. **Speculative generality**: a new trait or pattern has two implementations or a test that needs the seam.
3. **Duplicated logic written differently**: the same rule is not implemented twice in different shapes.
4. **Law of Demeter**: code talks to its direct collaborators, not through chains of them.
5. **Tell, don't ask**: behaviour lives with the data it uses instead of being computed from getters.
6. **Feature envy**: a function that mostly uses another type's data moves to that type.
7. **Registries, not string matches**: a new kind is one handler plus one registry line.
8. **Docs in the same PR**: the design docs describe the code as it is.
9. **Acceptance test first**: the behaviour was specified by a failing test before the code.
10. **Decisions are asked**: changes to `decision-log.md` decisions go to the maintainer.
