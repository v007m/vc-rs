# Vibranium v2: design handover

The design was worked out between Sarath and Claude in one long claude.ai conversation (22 October to 2 November 2026). This file captures what that conversation decided that a coding agent needs and that the docs don't state on their own: which document wins a conflict, what is still open, how Sarath likes to work, and where to start.

## State on handover

- **Done:** PRD, technical design, Rust LLD, syntax reference, CLI and workflow spec, and a decision log with D1–D95. The implementation kick-off grill (Round 15, D96–D137, plus D138–D140 in Round 16 and D141–D143 in Round 17) then settled the toolchain, repo layout, crate names, engineering rules and the open semantics for M0–M2. Also done: a Rust feasibility spike (passed) and clickable mockups of every CLI, TUI, web UI and report screen.
- **Not started:** engine code. The first task is M0 (see below).
- **Not yet written:**
  - the `.proto` contracts (`vibranium.{ir,engine,plugin,results}.v1`);
  - the plugin protocol spec (JSON-RPC method mapping and handshake);
  - SDK builder designs (TypeScript first);
  - the v1 → v2 migrator mapping.

  Each one is written when its milestone needs it, as part of that milestone.

## The documents

The Markdown files here were exported from Claude Docs on 2 November 2026. **From now on these repo copies are the source of truth**; the Claude Docs versions are frozen. A few diagrams did not survive the export and appear as a line such as `[embedded content: …]`. The live docs below still show them.

| File | Live original | Authoritative for |
| --- | --- | --- |
| `lld.md` | https://claude.ai/artifact/LpBFQpyn2qRL6Mjx61cPbG | Code structure, types, patterns, gates, milestones, diagnostic codes |
| `syntax-reference.md` | https://claude.ai/artifact/X8jbyunGSXD9xMtiE1J1hi | File formats and semantics |
| `cli-workflow.md` | https://claude.ai/artifact/RoNsEfNUhVD3wxnzF4LS4x | Commands, flags, exit codes, naming conventions, `vc init` output |
| `decision-log.md` | https://claude.ai/artifact/3kpFdJwZ8j6UnTEAzryq9M | Every decision and what it supersedes |
| `prd.md` | https://claude.ai/artifact/VmAc1DHTuvFBfpjj6Z5hpV | Requirements R1–R44, release plan, success metrics |
| `technical-design.md` | https://claude.ai/artifact/H8xJWMv9JBR6pFNRvZqsFT | Language-neutral architecture, IR, ports, history schema, report format |
| `language-decision.md` | https://claude.ai/artifact/NQW3t3hN9crncbVrv4whLS | Why Rust, spike numbers, conditions attached to the decision |

When documents disagree, the order of precedence is:

1. The newest row in the decision log.
2. `lld.md` for anything about code, and `syntax-reference.md` / `cli-workflow.md` for anything a user writes or types.
3. `technical-design.md`.
4. `prd.md`.

The technical design predates the Rust decision in places. Its monorepo tree (`core/`, `adapters/`, `sdks/`) is replaced by the LLD's `crates/` workspace.

Mockups (design canvas, version 31): https://claude.ai/artifact/6LkEaddcAAZ5Fs78L8NLL6. The canvas has four pages: CLI, Report, TUI and Web. Their sources and test harnesses are in `design/mockups/`.

## Decisions most likely to be misread

These changed during the conversation. The older wording may still appear in an early decision-log round or in history sections of the docs.

- **No `given`, `setup` or `scope` keys.** JUnit-style lifecycle blocks replaced them (D67–D71):
  - `beforeRun`/`afterRun` in config;
  - `beforeAll`/`afterAll` and `beforeEach`/`afterEach` in a test file;
  - `before`/`after` on a test.

  Where a block sits decides how long its data lives.
- **Lifecycle actions call apis.** `call: <api>` is the normal action. `test: <test>` exists only to reuse a multi-step flow and runs only that flow's calls, never its checks. `function:` and `command:` run code.
- **Cleanup is declared once, on the api.** Every 2xx call registers its cleanup. Cleanups run newest first when the owning block ends, including on Ctrl-C. `keep: true` opts out.
- **Shared saves** from `beforeRun` and `beforeAll` are read-only and live under the action's `id` (`${var.admin.userId}`). `beforeRun` and `beforeAll` actions run eagerly: they are planned when their run or file has a selected instance, and finish before any instance starts (D122). The token cache and cache-store hits are still lazy.
- **Profiles are mandatory.** `vc init` creates `default`. "Environment" now means OS environment variables only.
- **Each api routes to its own file's `baseUrl` and `auth`**, so one test can span several services.
- **Full IDs use `#`:** `<fileId>#<id>` (`users.crud#getsCreatedUser`) in YAML, selectors and output (D138). A bare dotted word is always a file ID. File IDs are derived from paths, and an explicit `id:` must match (D139); example rows are selected with `:n` (D140).
- **Language:** Rust. Hooks are TypeScript, type-checked by the user with `tsc` against the types-only `@vibranium/sdk`. At load time oxc strips the types and the hook runs in QuickJS (rquickjs), not V8.
- **Web UI:**
  - Svelte/SvelteKit (D95, Sarath's own edit).
  - One project per UI: no sign-in, no avatar, no project switcher.
  - Run buttons appear only where they act on something.
  - `vc ui` is the local UI; `vc serve` is the later team server.
- **HTML report:** a small app over JS data files loaded on demand (`VIB.load(...)`, so it works from `file://`), with a virtualized table. It is not one big HTML file; v1's report was very slow.

## Waiting on Sarath

Each item has a default the plan assumes. Ask before relying on it past the milestone shown.

| Question | Default | Needed by |
| --- | --- | --- |
| Sample project target: a demo service built into the binary (`vc demo`, localhost:7421) or a public echo API? | Built-in `vc demo` (D89) | M5 |
| Docs and JSON Schema hosting | GitHub raw URLs on `master` of `v007m/vc-rs`, e.g. `https://raw.githubusercontent.com/v007m/vc-rs/master/schema/<kind>.schema.json`, plus SchemaStore entries | v2.0 release |
| Staging agent policy: should PATCH and definition edits also need approval? | `approve: [PUT, PATCH, DELETE]`, `writeDefinitions: ask` | M7 |
| Review of the LLD and a click-through of the mockups in Play mode | Treat both as accepted | Before M2 and M9 |
| Default secrets provider: Varlock or plain env | — | M3 kick-off |
| Notifiers: v2.0 (PRD) or M8 (LLD) | — | M3 kick-off |
| Does `vc demo` reuse the mock server (`bench/mock`)? | — | M5 kick-off |

The open questions in the LLD are listed in its last section:
- the Varlock CLI contract (M3);
- CEL `timestamp()` and `duration()`, blocked by `cargo deny` (M4), and other `cel` 0.15 gaps (M4);
- mapping Faker names to the `fake` crate (M4);
- Windows plugin enforcement (M8);
- the `@vibranium` npm scope.

## How Sarath likes to work

- He wants to be **questioned on decisions** rather than have them made for him. Bring several questions at once with a recommended option for each, and record the answers as a new round in `decision-log.md`.
- He wants **docs kept current as decisions land**, not caught up at the end.
- His recurring requirements:
  - loose coupling through interfaces, plus the named patterns (orchestrator, strategy, factory/registry, builder, decorator, …);
  - Fowler's *Refactoring* smells kept out by machine checks;
  - YAGNI and DRY;
  - speed, measured against budgets.
- Feedback that shaped the UI:
  - plain-looking layouts get rejected;
  - layouts must be responsive, with no clipped text and no stranded narrow controls;
  - editors must never damage the YAML (switching views must not rewrite text, and form edits must be minimal line edits);
  - every button shown must work;
  - settings and agent pages must explain themselves.
- Naming conventions are enforced by every creator command, not only linted.

## Starting M0

The repository is `v007m/vc-rs`; `master` is the integration branch and each session works on one `s/<name>` branch. The approved plan for bootstrap, M0, M1 and M2 is recorded as decisions D96–D137 in `decision-log.md` (Round 15). M0 delivers:
- the Cargo workspace (resolver 3, edition 2024, Apache-2.0) with `rust-toolchain.toml` pinned to 1.99, `mise.toml` (tools and tasks) and a pnpm workspace;
- skeleton crates for M0–M2 only: model, diagnostics, parser, template, compiler, planner, interfaces, executor, engine, testkit, adapter-system, adapter-http, adapter-reporters, cli, arch-tests, and the mock server at `bench/mock`;
- `vc-arch-tests`, which enforces the layered dependency table and the engineering rules (each rule prints a stable ID such as `ARCH-LAYER`);
- `scripts/gates.sh`, `scripts/commit-check.sh`, `scripts/gates-selftest.sh` with one bad patch per gate, CI on Linux (all gates) and macOS/Windows (build and test), CODEOWNERS, `CONTRIBUTING.md` and branch protection.

It is done when CI is green on all three systems, every gate fixture is rejected by its expected gate ID, and `vc --version` works.

Start from `spike/rust-feasibility/partb`. It already has the workspace lints, `clippy.toml` (40 lines, complexity 10, disallowed lock and global types), `deny.toml`, `rustfmt.toml`, a working `gates.sh` and an `arch-tests` crate. Carry those over, then apply the LLD: the Apache-2.0 licence and NOTICE, the `crates/` layout with the Round 15 crate names, `too-many-arguments-threshold = 4` plus the arch-test parameter rule, the D135 `deny.toml` settings, and `gates.sh` moved to `scripts/` using `cargo nextest`, `jscpd` and the per-crate `cargo-mutants` threshold (85 % in executor, planner, template and compiler).

The spike's mock server, singleflight proof and borrow-checker log (`spike/rust-feasibility/logs/borrow_checker_log.md`) are worth reading first. Some spike scripts contain absolute paths from the original cloud workspace (`/home/claude/vibranium-spike`); they are reference material, not part of the build.
