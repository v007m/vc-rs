# Contributing to Vibranium v2

Thanks for helping. Everyone who takes part follows the [Code of Conduct](CODE_OF_CONDUCT.md).

Read `CLAUDE.md` and `docs/design/HANDOVER.md` first. The design docs in `docs/design/` are the specification.

## Ways to contribute

- **Report a bug, ask for a feature or flag a docs problem**: [open an issue](https://github.com/v007m/vc-rs/issues/new/choose) and pick the form. A bug report wants `vc --version`, a minimal project, the command and its `--reporter json` output, with secrets removed.
- **Report a vulnerability** privately, as [SECURITY.md](SECURITY.md) describes. Never in a public issue.
- **Send a pull request.** For anything bigger than a small fix, open a feature request first, so the design question is settled before you write the code (see [Decisions](#decisions)).

## Setup

1. Install [rustup](https://rustup.rs) and [mise](https://mise.jdx.dev).
2. In the repo: `rustup toolchain install` (reads `rust-toolchain.toml`), then `mise trust && mise install` (cargo tools, node, pnpm), then `pnpm install`.
3. Check everything: `mise run gates`.

Notes:

- `rustup show active-toolchain` should name `1.99` "overridden by `rust-toolchain.toml`". If it doesn't, another Rust (Homebrew's, say) comes first on your `PATH`; put rustup's `bin` folder ahead of it.
- Run `pnpm` through mise, so you get the version `mise.toml` pins: [activate mise](https://mise.jdx.dev/getting-started.html) in your shell, or prefix commands with `mise exec --` (`mise exec -- pnpm install`).
- The web UI (`web/`) is only built into `vc` with `--features web-ui`, after `pnpm --filter web run build`. The gates build `vc` without it.

## Tasks

| Command | What it does |
| --- | --- |
| `mise run gates` | Every quality gate, stopping at the first failure (`scripts/gates.sh`). `GATES_MUTANTS=1` adds mutation testing on your diff |
| `mise run test` | Tests and doctests only |
| `mise run commit-check` | Checks the commits on your branch against `origin/master` |
| `mise run gates:selftest` | Checks that each gate still rejects its bad patch in `tests/gate-fixtures/`. It builds in `target/gates-selftest/` (or `GATES_SELFTEST_TARGET_DIR`), apart from your own builds |
| `mise run public-api` | Regenerates the public API snapshots of `vc-model`, `vc-interfaces` and `vc-engine`; run it after changing a public type (needs rustup's nightly) |
| `mise run mock` | Runs the mock HTTP server (`bench/mock`) the end-to-end tests use |
| `mise tasks` | Lists every task, benches included |

Each gate prints its ID (`GATE-FMT`, `GATE-CLIPPY`, `GATE-ARCH`, `GATE-DENY`, `GATE-JSLICENSE`, `GATE-PROTO`, `GATE-MACHETE`, `GATE-DUP`, `GATE-TEST`, `GATE-MUTANTS`, `GATE-COMMIT`, `GATE-REFACTOR`) and `FAILED <id>` when it fails. The rules behind them are in the LLD's "Design rules and quality gates". Gate files are owned by the maintainer (`.github/CODEOWNERS`); if a gate looks wrong, ask instead of changing it.

## Branches, commits and PRs

- `master` is the integration branch and is protected. Each session works on one long-lived `s/<name>` branch and merges `master` into it to stay current.
- Keep PRs small: under about 400 changed lines. Every PR passes `mise run gates` and a code review before it merges.
- Commit subjects are conventional commits: `feat|fix|refactor|test|docs|chore|perf|build|ci`, an optional `(scope)` and `!`, then `: subject`.
- Tidy first: `refactor:` commits never change test assertions. CI rejects one that touches a test file or an `assert` line.
- Write the milestone's acceptance tests first, then work inward.
- Update the design docs a change touches in the same PR. A change to a decision in `decision-log.md` is asked, not decided in code.

## The design docs

Everything is specified in [`docs/design/`](docs/design/). Start with [`HANDOVER.md`](docs/design/HANDOVER.md): it says which document wins when two disagree and what is still open.

| Doc | Authoritative for |
| --- | --- |
| [`lld.md`](docs/design/lld.md) | Crates, types, orchestration, patterns, gates, diagnostic codes, milestones |
| [`syntax-reference.md`](docs/design/syntax-reference.md) | Every file format: config, `*.api.yaml`, `*.test.yaml`, `*.load.yaml`, templates, lifecycle blocks |
| [`cli-workflow.md`](docs/design/cli-workflow.md) | Every `vc` command, flag, selector and exit code |
| [`decision-log.md`](docs/design/decision-log.md) | Every decision, numbered `D1`, `D2`, ... |
| [`prd.md`](docs/design/prd.md), [`technical-design.md`](docs/design/technical-design.md) | Requirements and the language-neutral architecture |
| [`google-rust-style-guide.md`](docs/style/google-rust-style-guide.md) | How Rust code is written; the project rules win where they differ |

## Decisions

Each settled choice is a numbered row (`D1` to `D3xx`) in [`decision-log.md`](docs/design/decision-log.md), grouped in dated rounds. A later row overrides an earlier one, and its "Supersedes" column names the rows it replaces.

- A change that would alter a decision is asked, not decided in code. Raise it in a feature request or on the PR, naming the row (`D138`).
- The maintainer decides. The answer goes in as a new numbered row in a new round, in the same PR as the code that follows it.
- Docs describe the code as it is, so a PR updates every design doc and site page its change touches.

## Adding a kind

Matchers, pipes, auth types, reporters, importers, notifiers and action executors are **registries**, never a string `match` (LLD "Factory and registry"). Adding one is one new file with its handler plus one registration line:

- a pipe: a function in `crates/template/src/pipe_fns.rs` and a line in the `BUILTIN` table of `crates/template/src/builtin_pipes.rs`;
- a console reporter: a `Reporter` type and a `register` line in `ReporterRegistry::builtin` (`crates/adapters/reporters/src/registry.rs`);
- an auth type: an `AuthProvider` and its entry in `AuthTypes` (`crates/adapters/auth/src/registry.rs`).

A new kind also needs:

- a behavioural test that runs a real YAML example of it;
- when it adds YAML syntax, the JSON Schemas in `schema/` exported again with `cargo run -p vc-cli --bin vc -- schema export --out schema` (a test fails while they are stale);
- a reference page on the docs site, whose YAML examples pass `vc check` (`crates/engine/tests/docs_examples.rs` checks every `yaml` block);
- a diagnostic code for each error a user can fix.

If a second place needs a `match` on a kind, turn it into a strategy with a registry first, in a `refactor:` commit.

## The docs site

The user docs are an [Astro Starlight](https://starlight.astro.build) site in `site/`, published to <https://v007m.github.io/vc-rs/> from `master`.

| Command | What it does |
| --- | --- |
| `pnpm --filter site dev` | Serves the site at <http://localhost:4321/vc-rs/>, reloading as you edit |
| `pnpm --filter site build` | Builds it into `site/dist/`, as CI does |
| `pnpm --filter site preview` | Serves the built `site/dist/` |
| `pnpm --filter site wasm` | Builds the playground's runner (`crates/browser`) to WebAssembly; the playground's Run and Check need it |
| `pnpm --filter site test` | Runs the Playwright tests (after `build`, `wasm` and `pnpm --filter site exec playwright install chromium`) |

Pages live in `site/src/content/docs/` as `.mdx`, with the sidebar in `site/astro.config.mjs`. Link pages with the `/vc-rs/` prefix (`/vc-rs/start/quickstart/`). Don't run `vc` inside `site/src/examples`: the cache it writes there breaks the playground tests. Copy the folder somewhere else to try it.

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
