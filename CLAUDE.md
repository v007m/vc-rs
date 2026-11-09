# Vibranium v2

Vibranium v2 is a ground-up rewrite of the Vibranium API testing CLI (v1: github.com/sarathm09/vibranium). It is one Rust engine for functional, contract and load tests of HTTP APIs. A CLI, TUI, MCP server and web UI all sit on top of that engine. Tests are plain YAML in git. Maintainer: Sarath. Licence: Apache-2.0.

The design is finished and no engine code exists yet. Work starts at milestone **M0**.

## Read before writing code

Everything is specified in `docs/design/`. Read `docs/design/HANDOVER.md` first. It gives the current state, which document wins when two disagree, and the open questions.

| Doc | Authoritative for |
| --- | --- |
| `lld.md` | Crates, types, orchestration, patterns, gates, diagnostic codes, milestones M0–M10 |
| `syntax-reference.md` | Every file format: config, `*.api.yaml`, `*.test.yaml`, `*.load.yaml`, templates, lifecycle blocks |
| `cli-workflow.md` | Every `vc` command, flags, selectors, exit codes, naming conventions, `vc init` output |
| `decision-log.md` | Decisions D1–D202. Later rounds supersede earlier ones; the "Supersedes" column says which |
| `prd.md`, `technical-design.md` | Requirements R1–R44 and the language-neutral architecture. Where they conflict with the LLD, the LLD wins |
| `language-decision.md` | Why Rust, and the conditions attached to that choice |
| `docs/style/google-rust-style-guide.md` | How Rust code is written (D157). Where it differs from these project rules, the project rules win |

The UI mockups are in `design/mockups/`. The Rust feasibility spike is in `spike/`. M0 starts from the spike's `partb` workspace.

## How to work

- Take **one milestone at a time** from the LLD's "Implementation plan". Write its acceptance tests first.
- Git: `master` is the integration branch. Each session works on one long-lived `s/<name>` branch and lands changes through small PRs to `master` (review with `/code-review`, then merge). Commits use conventional prefixes (`feat|fix|refactor|test|docs|chore|perf|build|ci`); `refactor:` commits never change test assertions.
- Keep PRs small: under about 400 changed lines each. Every PR must pass `mise run gates` (which runs `scripts/gates.sh`).
- If a change would alter a decision in `decision-log.md`, **ask Sarath instead of deciding in code**. Once he answers, add the decision to the log as a new numbered row in a new round.
- Update the design docs a change touches in the same PR. The docs must describe the code as it is.
- YAGNI: add a pattern, trait or abstraction only when two implementations exist or a test needs the seam. The LLD's pattern table is the complete list.
- Refactor before extending. When a `match` on a kind is needed a second time, replace it with a strategy and a registry. When a function grows past the limits, extract functions or a parameter object first.

## Hard rules (machine-checked; see the LLD's "Design rules and quality gates")

- The compiled `Program` is immutable, held in an `Arc`. Run state lives in executor frames, each owned by a single task.
- Nodes refer to each other by typed IDs (`ApiId`, `TestId`, `ActionId`), never by references. No lifetimes appear in a public API.
- Long-lived parts communicate through channels. There is no global or `static` mutable state.
- Shared mutable state exists only in the singleflight slots, the token cache and the redactor (`arc-swap`). Locks are allowed only in the token cache and redactor files.
- No `unwrap`, `expect`, `panic!`, `todo!` or `dbg!` in library code. Errors are `Result` values, with a diagnostic code wherever the user can act on them.
- Each kind (matcher, pipe, auth type, reporter, importer, notifier, action executor) is a registry entry, not a string `match`.
- Functions are at most 40 lines, with cognitive complexity at most 10 and at most 3 parameters. Source files are at most 400 lines; traits have at most 7 methods.
- Core crates do no I/O. They talk to the outside only through `vc-interfaces` traits, injected as `Arc<dyn Trait>`. The compiler and planner are pure (D132).
- Each library crate has its own `thiserror` error enum; `anyhow` is allowed only in `vc-cli`. A crate's `lib.rs` is a facade: private modules plus `pub use`, no `pub mod`.
- Execution is orchestrated, never choreographed. `RunOrchestrator`, `FileOrchestrator` and `InstanceWorkflow` are pure state machines: `transition(state, input) -> (state, commands)`. Workers carry out commands; they never call the next step.
- Crate dependencies follow the LLD's dependency table, which `arch-tests` enforces. Crates are layered (model → diagnostics → parser/template/interfaces/contracts → compiler/planner/executor/adapters → loadtest → engine → tui/mcp/server → cli); every edge points to a lower layer. For example, `compiler` and `planner` never touch `interfaces`, and adapters never import each other. Only `EngineBuilder` names concrete adapter types. Apps import only `engine` (plus `contracts` for mcp/server); `vc-cli` is the launcher and may also import tui/mcp/server. `vc-testkit` is a dev-dependency only.
- **Do not edit** `clippy.toml`, `[workspace.lints]`, `deny.toml`, `scripts/gates.sh` or `crates/arch-tests`. If a gate seems wrong, stop and ask.

## Conventions

- Files and folders:
  - `vibranium.config.yaml` at the project root.
  - `services/<service>/<service>.api.yaml` for apis.
  - `services/<service>/<area>.test.yaml` for tests, with file ID `<service>.<area>`.
- IDs:
  - Every item has a camelCase `id` and a free-text `name`; the full ID is `<fileId>#<id>` (`users.crud#getsCreatedUser`, D138). A bare dotted word is always a file ID. A file's ID is derived from its path; deeper folders add words (`services/payments/refunds/partial.test.yaml` → `payments.refunds.partial`). An `id:` key is optional, and must match the path (error V2007, D139). Select one example row with `:n` (`users.crud#rejectsBadEmail:2`).
  - Test IDs are third-person verbs, such as `rejectsDeclinedCard`.
  - Var names end in `Id` or `Url` where that applies. Profiles are lowercase. Tags are kebab-case.
- Diagnostics:
  - Codes: V1 syntax, V2 references and types, V3 lifecycle, V4 conventions, V5 run time.
  - Each diagnostic carries a code, a span, an underlined excerpt, `help:` and, where possible, a certain `fix:`.
- Binary names: `vc`, also installed as `vibranium`. Exit codes: 0 pass, 1 fail or blocked, 2 nothing ran, 3 load threshold breached, 4 sync drift, 70 internal error, 130 cancelled.
- Crates live in `crates/<dir>` with package names `vc-<dir>` (adapters: `vc-adapter-<x>`): model, diagnostics, parser, template, interfaces, contracts, compiler, planner, executor, loadtest, engine, testkit, arch-tests; apps tui, mcp, server, cli. The mock server is `bench/mock` (`vc-mock-server`).
- Toolchain: Rust 1.99 via rustup (`rust-toolchain.toml`), edition 2024. Tools and tasks are pinned in `mise.toml` (`mise run gates/test/bench/mock`); JS packages use pnpm. The web UI is Svelte/SvelteKit (static adapter, Vite), embedded in the binary.
