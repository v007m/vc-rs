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
| `decision-log.md` | Decisions D1–D95. Later rounds supersede earlier ones; the "Supersedes" column says which |
| `prd.md`, `technical-design.md` | Requirements R1–R44 and the language-neutral architecture. Where they conflict with the LLD, the LLD wins |
| `language-decision.md` | Why Rust, and the conditions attached to that choice |

The UI mockups are in `design/mockups/`. The Rust feasibility spike is in `spike/`. M0 starts from the spike's `partb` workspace.

## How to work

- Take **one milestone at a time** from the LLD's "Implementation plan". Write its acceptance tests first.
- Keep PRs small: under about 400 changed lines each. Every PR must pass `scripts/gates.sh`.
- If a change would alter a decision in `decision-log.md`, **ask Sarath instead of deciding in code**. Once he answers, add the decision to the log as a new numbered row in a new round.
- Update the design docs a change touches in the same PR. The docs must describe the code as it is.
- YAGNI: add a pattern, trait or abstraction only when two implementations exist or a test needs the seam. The LLD's pattern table is the complete list.
- Refactor before extending. When a `match` on a kind is needed a second time, replace it with a strategy and a registry. When a function grows past the limits, extract functions or a parameter object first.

## Hard rules (machine-checked; see the LLD's "Design rules and quality gates")

- The compiled `Program` is immutable, held in an `Arc`. Run state lives in executor frames, each owned by a single task.
- Nodes refer to each other by typed IDs (`ApiId`, `TestId`, `ActionId`), never by references. No lifetimes appear in a public API.
- Long-lived parts communicate through channels. There is no global or `static` mutable state.
- Shared mutable state exists only in the singleflight slots and the token cache.
- No `unwrap`, `expect`, `panic!`, `todo!` or `dbg!` in library code. Errors are `Result` values, with a diagnostic code wherever the user can act on them.
- Each kind (matcher, pipe, auth type, reporter, importer, notifier, action executor) is a registry entry, not a string `match`.
- Functions are at most 40 lines, with cognitive complexity at most 10 and at most 3 parameters.
- Execution is orchestrated, never choreographed. `RunOrchestrator`, `FileOrchestrator` and `InstanceWorkflow` are pure state machines: `transition(state, input) -> (state, commands)`. Workers carry out commands; they never call the next step.
- Crate dependencies follow the LLD's dependency table, which `arch-tests` enforces. For example, `compiler` and `planner` never touch `ports`, and adapters never import each other. Only `EngineBuilder` names concrete adapter types. Apps import only `engine`.
- **Do not edit** `clippy.toml`, `[workspace.lints]`, `deny.toml`, `scripts/gates.sh` or `crates/arch-tests`. If a gate seems wrong, stop and ask.

## Conventions

- Files and folders:
  - `vibranium.config.yaml` at the project root.
  - `services/<service>/<service>.api.yaml` for apis.
  - `services/<service>/<area>.test.yaml` for tests, with file ID `<service>.<area>`.
- IDs:
  - Every item has a camelCase `id` and a free-text `name`; the full ID is `<fileId>.<id>`.
  - Test IDs are third-person verbs, such as `rejectsDeclinedCard`.
  - Var names end in `Id` or `Url` where that applies. Profiles are lowercase. Tags are kebab-case.
- Diagnostics:
  - Codes: V1 syntax, V2 references and types, V3 lifecycle, V4 conventions, V5 run time.
  - Each diagnostic carries a code, a span, an underlined excerpt, `help:` and, where possible, a certain `fix:`.
- Binary names: `vc`, also installed as `vibranium`. Exit codes: 0 pass, 1 fail or blocked, 2 nothing ran, 3 load threshold breached, 4 sync drift.
- Toolchain: Rust 1.97, edition 2024. The web UI is Svelte/SvelteKit (static adapter, Vite), embedded in the binary.
