# Vibranium v2 — Decision Log

Oct 22, 2026 · @Sarath

## Context

Vibranium v2 is a ground-up rewrite of the [v1 API testing CLI](https://github.com/sarathm09/vibranium) into a git-native, extensible API testing engine shared by CLI, TUI, web UI, SDKs and AI agents.

Research baseline (from the deep-research report):

- v1's slowness is mostly a design flaw: dependencies re-run as full tests, assertions included. A 4-test chain triggers 10 executions instead of 4.
- The proposed fix: split API definitions (`*.api.yaml`) from tests (`*.test.yaml`), add scoped, memoized fixtures, and run everything through a compiled, cycle-checked DAG.
- Weighted language matrix: TypeScript 92, Go 69, Rust 58. Main drivers: fluency, dynamic JSON + Zod, native JS hooks, MCP SDK, web UI type sharing.
- White space vs Hurl, Bruno, Step CI, Postman: typed cross-file reuse, real fixtures, compile-time integrity checks, local run history, agent-native interface.
- v1 has 5 stars and was last published \~3 years ago, so a clean break is cheap.

## v1 source findings

The v1 source (4,773 lines of JavaScript in 13 files) was read on 22 October 2026 and confirms the dependency flaw.

| Area | What v1 does | Where | v2 response |
| --- | --- | --- | --- |
| Dependencies | Each dependency re-runs the full pipeline: scripts, request and every assertion. Any failed assertion fails the parent | `testexecutor.js` 661–705 | `given` runs with guards only (D12, D15) |
| Repeat × dependency | Dependencies resolve inside every repeat iteration, one after another | `testexecutor.js` 745–785 | `given` is cached per scope, outside repeats |
| Caching | Opt-in `cache: true` in in-memory NeDB, keyed by collection, scenario and name only; input values are ignored | `testexecutor.js` 53–142, `dbhandler.js` | Cache key includes resolved inputs |
| Concurrency | One global counter polled every 300 ms; the cache-hit path decrements without incrementing, so the limit drifts | `testexecutor.js` 32–42, 135 | Proper semaphores per host and scope |
| HTTP | axios plus the deprecated `request` library | `servicehandler.js` | Transport behind an interface (D23) |
| Expressions | Assertion strings and timing checks run through vm2, discontinued in 2023 after sandbox escapes | `asserthandler.js` | CEL plus code hooks (D17) |
| Config | Reads `~/.vib/config.json` for the workspace path, then the workspace `config.json` | `constants.js` 31–33 | Project-root config (D20) |

## Decided

Round 1 set an ambitious scope: a full, fully open-source platform with load testing, built solo with AI agents. The language is now decided by a benchmark spike rather than the research matrix.

| # | Decision | Choice | Rationale |
| --- | --- | --- | --- |
| D1 | Product boundary | Full platform: runner, exploratory client, mocks, monitoring, team features | Goal is to be the de facto API testing tool |
| D2 | Load testing | In scope, full load testing | Same definitions should serve functional and performance testing |
| D3 | Primary users | Developers, QA, and AI agents alike | Must work in-repo and as a standalone test repo |
| D4 | Protocols | HTTP/REST in v2.0; GraphQL, gRPC, WebSocket/SSE next; UI testing via HTML/XML parsing later | Protocols added as executor plugins |
| D5 | Builders | Solo maintainer plus AI coding agents | Favours strict types, small modules and spec-first docs agents can follow |
| D6 | Load engine and language | Decide after a TS vs Go benchmark spike | Load testing reopened the language question; measure instead of guessing |
| D7 | Hosting and licence | Self-hosted server, everything open source | Team features via a server anyone can run; no paid tier |
| D8 | Load scale target | Single machine, about 1,000 to 5,000 virtual users | Covers most teams; no distributed coordinator in v2 |
| D9 | API definition source | Vibranium's own `*.api.yaml`, re-syncable from OpenAPI later | Specs can't hold Vibranium names, outputs or defaults; sync must keep local edits |
| D10 | Load test definition | Separate `*.load.yaml` that references existing tests | No duplication; functional tests reused as load scenarios |
| D11 | Shared setup failure | Retry, then mark dependents blocked; every failure and block carries a detailed cause | One root cause shown once, with exactly what broke |
| D12 | Core concepts | Three: **api** (how to call), **test** (call and check; `steps:` for multi-step), **given** (reusable, cached setup that produces values) | Fewer names; "suite" and "flow" dropped; a file is only grouping |
| D13 | Where givens live | Anywhere: in test files or `*.given.yaml`, referenced by ID across files | Shared setup without forced file layout |
| D14 | OpenAPI resync | Three-way merge with a conflict report | Spec changes flow in; local names and outputs survive |
| D15 | API guards | Built-in sanity checks on apis, overridable per test | Negative tests (e.g. expect 404) stay possible |
| D16 | Data-driven tests | `data:` holds input rows; one test runs per row | Freed by naming setup `given` |
| D17 | Custom logic | CEL expressions in YAML; code hooks for power users | CEL is sandboxed and portable across TS, Go and Rust |
| D18 | Config formats | YAML and JSON only | Declarative, safe in CI, independent of core language |
| D19 | Plugin runtime | First-party plugins built in; third-party plugins in any language over JSON-RPC on stdio | Language-neutral; same protocol family as MCP and LSP |

Round 4 additions:

| # | Decision | Choice | Rationale |
| --- | --- | --- | --- |
| D20 | Environments | `environments:` blocks inside the one project config | One file to read; selected with `--env` |
| D21 | Secrets | Built on Varlock, wrapped in Vibranium's own reference syntax and style | Varlock's provider coverage without exposing its DSL as the user-facing syntax |
| D22 | History contents | Configurable and redacted; default is headers plus truncated body on failure, full bodies opt-in | Debuggable without disk or privacy blow-up |
| D23 | Architecture principle | Every layer sits behind an interface (port) with configurable adapters, accessors and clients | Nothing tightly coupled; any layer swappable |
| D24 | History storage | SQLite in WAL mode with one batched writer by default; storage engine configurable through the store interface | Zero setup locally; server or other databases plug in |
| D25 | Plugin and service transport | gRPC internally; JSON-RPC, webhooks and REST offered to developers with ready-made clients | Fast internals, easy integration; one contract serves all |
| D26 | Contract language | Protobuf for plugins, SDKs and services | Generates types for every language; maps to JSON |
| D27 | Default alert triggers | On failure, on recovery, on flaky, on threshold breach | Covers functional and load runs |

Note for the technical design: Connect RPC serves gRPC, gRPC-Web and plain JSON over HTTP from the same protobuf definitions, which fits D25 without hand-writing three servers.

Round 5 additions:

| # | Decision | Choice | Rationale |
| --- | --- | --- | --- |
| D28 | Code SDKs | TypeScript, Python, Go and Java/Kotlin | Covers JS, Python, Go and JVM test ecosystems |
| D29 | Agent capabilities (MCP) | Read (list, search, history), run and explain failures, create and edit tests, call any api ad hoc | Agents are full users of the platform |
| D30 | Interface build order | CLI, then MCP, then TUI, then web UI | Agent support is cheap and differentiating; web UI needs the server |
| D31 | Editing in TUI and web UI | Both can edit; changes write back to YAML files | Files stay the single source of truth |
| D32 | Agent guardrails | Policy per environment: allowed envs and methods in config; writes to prod need human approval | Agents can't hit production destructively by accident |
| D33 | SDK sequencing | Generated protobuf types for all four on day one; hand-written builders in order TS, Python, Go, Java | Keeps solo workload bounded |
| D34 | v1 compatibility | One-shot `vib migrate v1` converter; no legacy runtime | v1 has few users; clean core |
| D35 | Licence | Apache-2.0 | Patent grant; preferred by companies and foundations |

Round 6 additions:

| # | Decision | Choice | Rationale |
| --- | --- | --- | --- |
| D36 | Spike criteria | Compile speed for 1,000 files, hook and CEL overhead under load, type strictness, interface-based design, library support, extensibility | Judge the language on the whole engine, not raw speed alone |
| D37 | Spike languages | TypeScript, Go and Rust | Measure all three before ruling any out |
| D38 | Mocks and scheduled monitoring | After the web UI | Both need the server; runner and load engine come first |
| D39 | CLI name | `vibranium`, with `vc` as a short alias | Unambiguous full name plus v1 continuity |

Round 7 additions:

| # | Decision | Choice | Rationale |
| --- | --- | --- | --- |
| D40 | Load throughput in spike | Included at low weight | Load matters, but maintainability decides |
| D41 | Spike weights | Code quality and type strictness 30, library support 25, extensibility 20, performance 25 (compile 10, CEL/hooks 10, load 5) | Maintainability-heavy, as chosen |
| D42 | Third-party plugin trust | Permissions manifest: a plugin declares network, file and secret access; the user approves on install | Safe for agents installing plugins |
| D43 | Canonical results | CTRF JSON; JUnit, spec and HTML derived from it | One standard on disk; existing CTRF tooling |
| D44 | CLI names | `vibranium`, with `vc` kept as the short alias; installer warns if another `vc` (such as Vercel's) is already on PATH | Continuity with v1, accepting the collision |
| D45 | Package names | Scoped npm packages (`@vibranium/*` once the org is claimed); Homebrew through an own tap | Unscoped npm and crates.io names are taken |

Round 8 additions (syntax review, 23 October 2026):

| # | Decision | Choice | Supersedes |
| --- | --- | --- | --- |
| D46 | Language and runtime | Decided 23 Oct after the Rust feasibility spike: Rust, with TypeScript hooks in embedded QuickJS (see the Language Decision doc). Before the spike, Rust was the leading candidate, decided after a focused Rust spike (embedded TypeScript hooks, borrow-checker-light architecture, automated clean-code gates, build times). Fair rerun scores: TS on Bun 86, TS on Node 85, Rust 81, Go 75 with the original weights; Rust 85–88 vs TS 83 with performance-first weights | D6 |
| D47 | Test model | Tests read given / when / then, with `cleanup`. Any test can be another test's given: only its `when` runs, with guards, cached per scope; its `then` never re-runs. No separate given or setup concept or file | D12, D13 |
| D48 | Setup-only tests | `standalone: false` marks a test that runs only as another test's given | — |
| D49 | Given scope | Set on the given reference (`scope: run \| file \| test`), default `file`; cache key is test ID, scope and `with` values | — |
| D50 | File kinds | `vibranium.config.yaml`, `*.api.yaml`, `*.test.yaml`, `*.load.yaml`; suffix decides the kind | D18 (file name) |
| D51 | Item identity | Every item has `id` (camelCase, enforced) and optional `name` (free text); full ID `<fileId>.<id>`; IDs are user-written, OpenAPI import fills them from `operationId` | — |
| D52 | Structure | Things with several properties are lists of items with `id`; plain name–value pairs are maps; nested YAML style, `vc fmt` normalises; JSON bodies can be pasted as is | — |
| D53 | Naming rule | Sections plural, references singular: `vars` → `${var.x}`, `inputs` → `${input.x}`, `examples` → `${example.x}` | D16 (`data` renamed `examples`) |
| D54 | Syntax version | `syntaxVersion` = file-format version for backward compatibility; changed only by `vc migrate`, never by upgrades | — |
| D55 | Inline logic | CEL one-liners in YAML; functions in code files for hooks and custom checks | confirms D17 |
| D56 | Generators | Faker catalogue with literal arguments inline, plus named generators in config, plus user datasets (`${dataset.cities.name}`) | — |
| D57 | Hooks | `hooks:` at run (`beforeRun`, `afterRun`), file (`beforeAll`, `afterAll`, `beforeEach`, `afterEach`), test (`before`, `after`) and api (`beforeRequest`, `afterResponse`); entries are functions or, at run and file level, shell commands. Custom checks live in `then.custom` | — |
| D58 | Multi-step auth | Auth profile `type: flow`: steps call apis, `apply` maps step outputs into headers, query or cookies, with expiry, refresh before expiry, refresh on 401, and scope; profiles can take `vars` and be used per step with `auth: {profile, with}` | — |

Round 9 additions (syntax review, 23 October 2026):

| # | Decision | Choice | Supersedes |
| --- | --- | --- | --- |
| D59 | Test shape | Tests use `call` (or `steps`), `vars`, `expect`; given/when/then dropped | D47 (shape) |
| D60 | Setup key | `setup:` lists tests reused as setup (`test`, `as`, `scope`, `vars`); reuse semantics unchanged | D47 (`given` name) |
| D61 | One value concept | `vars` everywhere; api inputs become api `vars` (defaults); outputs dropped; values closer to the call win; an unset var is a `vc check` error | D51 inputs, outputs |
| D62 | Saved values | `save:` names response values (body fields, headers) as vars; on an api it applies to every caller; a setup's saves appear as `${var.<as>.<name>}` | — |
| D63 | Api checks | Api-level `expect` replaces "guards"; a test's `expect` adds to it and overrides matching keys | guards |
| D64 | Request body | One `body`: nested YAML or pasted JSON → JSON, string → text, `$file` → file bytes, `$base64` → bytes, absent → empty; `content-type` decides form or multipart encoding | `form`, `multipart`, `raw` |
| D65 | Profiles | Spring-style profiles replace environments: config `vars` plus `profiles` overrides; any api or test file may carry its own `profiles`; `--profile a,b`, later wins | D20 |
| D66 | Auth location | Auth definitions in `vibranium.config.yaml`; flow steps may describe requests inline (`method`, `url`), so login endpoints need no api file | D58 (location) |

Round 10 additions (lifecycle and data reuse, 23 October 2026):

| # | Decision | Choice | Supersedes |
| --- | --- | --- | --- |
| D67 | Lifecycle blocks | `beforeRun`/`afterRun` (config), `beforeAll`/`afterAll` and `beforeEach`/`afterEach` (test file), `before`/`after` (test) hold lists of actions; where a block sits decides how long its data lives; no `scope` key and no `hooks:` wrapper | D49, D57 (wrapper), D60 |
| D68 | Actions | `call: <api>` is the normal action; `test: <test>` reuses a multi-step flow (its calls only); `function:` and `command:` run code; a test's `steps` use the same action list | D47 (test as setup) |
| D69 | Shared values | `beforeRun` and `beforeAll` saves land under the action's `id` (`${var.admin.userId}`) and are read-only; value-producing `beforeRun` actions run lazily, once, with concurrent callers waiting on the one run | D62 (`as`) |
| D70 | Loops | `repeat: N` and `forEach: <list>` on any `call` or `test` action; sequential by default, `parallel: N` to opt in; every name saved in a loop collects a list; `${item}`, `${iteration.*}` | — |
| D71 | Cleanup | Declared once on the api (`cleanup: call + vars`), registered by every 2xx call, run newest first when the owning block ends; `keep: true` opts out; test-level `cleanup` and `${self}` removed in favour of `after` | D47 (`cleanup`) |
| D72 | Cross-run cache | `cache: {for, check}` on an action, shipping in v2.0; stored in `.vibranium/cache.db` per profile, keyed by api or test, vars and profile; cached data cleaned up on expiry or `vc cache clear`; secret-derived values encrypted with an OS keychain key (`VIB_CACHE_KEY` without one); auth definitions take `cache: true` | v1 `cache: true` |
| D73 | Mandatory profiles | At least one profile; `vc init` creates `default`; a run with no active profile is a `vc check` error; "environment" now means OS environment variables only | D65 (optional) |
| D74 | Per-api routing | Each api uses its own file's `baseUrl` and `auth`, so one test can call several services; `auth:` overrides per call; the base URL changes only through vars | — |
| D75 | Payload reuse | `$extend` starts a map from an existing object (maps merge deeply, lists are replaced) and `$remove` drops paths; `body` on a call replaces the api's body for that call | — |
| D76 | Type conversion | Template pipes `number`, `int`, `bool`, `string`, `json`, `jsonString`, `base64`, `fromBase64`, `lower`, `upper`, `trim`, `urlEncode`, `default()`; `$cel` computes values | extends D55 |

Round 11 additions (developer workflow, 23 October 2026):

| # | Decision | Choice | Supersedes |
| --- | --- | --- | --- |
| D77 | Skipping | `skip: <reason>` or `skip: {reason, until}` on a test or at file level; reported as skipped with the reason; after `until` the test runs again and `vc check` warns; `--include-skipped`; `vc skip` / `vc unskip` edit the YAML | v1 `ignore` |
| D78 | Datasets | `${dataset.x}` is the whole list; used as `examples:` (one test per row), `forEach:` (one call per row) or with the `random` pipe (one row); `take(n)` and `shuffle` pipes; CSV cells are text | D56 (random-row template) |
| D79 | OpenAPI sources | `source.openapi` is a URL or a file, JSON or YAML, Swagger 2.0 converted; `source.headers` for protected specs, `source.include` to split one spec over several api files; every import and sync commits a snapshot to `openapi/<fileId>.yaml` as the three-way merge base; removed operations become `deprecated: true`; `vc sync --check` for CI | D14 (base location) |
| D80 | Generated tests | One happy-path test per api (status plus schema referenced from the snapshot); `--negative` adds an invalid-body test with one example row per case; `--auth` adds a 401 test; creating calls found in the same file become `before` actions | — |
| D81 | Run selection | Test ID, `#row`, file ID, ID glob, path, `-t` tags with `!`, `-g` grep, `--changed`, `--failed`, `--include-skipped`; the planner pulls in only the lifecycle actions selected tests need; exit codes 0 pass, 1 fail or blocked, 2 nothing ran, 3 load threshold, 4 sync drift | — |
| D82 | Authoring commands | `vc new api` (including `--from-curl`), `vc new test --from`, `vc new auth`, `vc call --save-as-test`, `vc explain`, `vc watch`, shell completion of project IDs; every writing command compiles before it writes | — |

Round 12 additions (interfaces and authoring, 23 October 2026):

| # | Decision | Choice | Supersedes |
| --- | --- | --- | --- |
| D83 | Web look | Web UI and HTML report share one look: a soft gradient with grain behind flat chrome (header, side navigation), and one elevated rounded content panel; no cards inside it; responsive to phone width | earlier card-based web mockups |
| D84 | API client | The web UI includes a Postman-style client over the project's apis: requests per profile, response inspection, multi-stage flows with chained values, history, save as test or flow test; it replaces the API console | API console |
| D85 | HAR import | `vc import har`, the TUI and the web UI turn browser recordings into apis (conventional IDs, path vars, merged duplicates, secrets stripped) and optionally a flow test | — |
| D86 | Editor | Every file kind is editable in form, YAML or split view over one source of truth (form edits regenerate YAML; YAML edits re-parse; switching views never rewrites text), with autocomplete, live diagnostics, quick fixes and diff | — |
| D87 | Naming conventions | One table of conventions for files, folders, IDs, vars, profiles, tags, datasets, schemas and hooks; every creator applies it, `vc check` lints it (V4xxx), `--fix` and `vc rename` rename with references | — |
| D88 | Diagnostics | Code, location, underlined excerpt, explanation, `help:` and `fix:`; `vc check --fix` previews certain fixes as a diff; `vc explain <code>`; code ranges V1 syntax, V2 references and types, V3 lifecycle rules, V4 conventions, V5 run-time | — |

Round 13 additions (polish, 23 October 2026):

| # | Decision | Choice | Supersedes |
| --- | --- | --- | --- |
| D89 | Project scaffold | `vc init` writes defaults for every setting, one `default` profile and an annotated sample (four apis, four tests) against a demo service built into the binary (`vc demo`), so a new project passes offline; `--no-sample` skips it | — |
| D90 | Documentation headers | Every created file starts with a schema line, a short description, a reference link and commands to try; schemas are also registered with SchemaStore; comment level `full`, `brief` or `none` via `new.comments` in the user config or `--comments` | — |
| D91 | Local web UI chrome | One project per UI: no sign-in, no avatar, no project switcher; the profile select is the config profile; Run buttons only where they act on something; collapsible sidebar; a Settings screen is the form view over `vibranium.config.yaml` and `.env.schema` (profiles, auth with a test button, secrets status, reports, alerts, agent access, plugins) | D83 shell details |
| D92 | AI agents page | Explains itself: what agents can do, how to connect (Claude Code, Cursor and other MCP clients, stdio or the UI's HTTP endpoint), approvals as plain sentences with Approve once, Always allow or Deny, a readable access summary (edited in Settings) and an activity log | — |
| D93 | HTML report format | A small app over data files loaded on demand (summary and index at open; per-test shards and bodies when opened); JavaScript data files so it works from `file://`; virtualized table; deep links per test; optional single-file layout with lazily decompressed per-test data | v1 single file and v1 split files |

Round 14 additions (engine structure, 23 October 2026):

| # | Decision | Choice | Supersedes |
| --- | --- | --- | --- |
| D94 | Execution model and patterns | Hierarchical orchestration: `RunOrchestrator` → `FileOrchestrator` → `InstanceWorkflow`, each a pure state machine that decides, while strategies and adapters act. Cleanups are saga compensations. Ports and adapters throughout, with strategy, factory and registry, builder (including `EngineBuilder` as the composition root), facade, observer, middleware chain (`tower`), decorator, command, visitor, specification, repository and null object, each used where it removes coupling or branching (listed in the LLD); interfaces only across crates | — |
| D95 | Web UI framework | Svelte with SvelteKit (static adapter), built by Vite and embedded in the binary | LLD default (React) |

Round 15 additions (implementation kick-off grill for M0–M2, 2 November 2026):

| # | Decision | Choice | Supersedes |
| --- | --- | --- | --- |
| D96 | Planning scope | M0–M2 planned in detail; M3+ each get a grill-and-review at kick-off | — |
| D97 | Git model | `master` is the integration branch; each session has one long-lived `s/<name>` branch; small PRs → code review → merge | `main` |
| D98 | Toolchain | rustup with `rust-toolchain.toml` pinned to 1.99 | LLD 1.97 |
| D99 | Monorepo | Cargo workspace + pnpm workspace + `mise.toml` (pins node, pnpm, cargo tools, jscpd, buf; tasks `mise run gates/test/bench/mock`) | — |
| D100 | Mock server | Rust axum crate `bench/mock` (`vc-mock-server`): library started in-process on port 0 by tests, plus a bin | spike Go mock |
| D101 | Wire contract | Hand-written `.proto` files are the contract (`buf lint`, `buf breaking`), generated into `vc-contracts` and TypeScript; `vc-model` stays hand-written; each proto is written at its first consumer | TD "IR defined in protobuf" |
| D102 | Crate names | `vc-<dir>`; ir→`model`, diag→`diagnostics`, syntax→`parser`, ports→`interfaces`, load→`loadtest`, proto→`contracts`; adapters `vc-adapter-<x>` | LLD names |
| D103 | Plan and Event types | Live in `vc-model` | — |
| D104 | Load engine | `vc-loadtest` may depend on `vc-executor` | LLD dep table |
| D105 | Diagnostics in leaf crates | `vc-parser` and `vc-template` may depend on `vc-diagnostics` | LLD dep table |
| D106 | Redactor and flow auth | Redactor is core logic in `vc-executor` (secrets adapter only supplies values); flow auth runs steps through an `ActionRunner` trait implemented by the executor and injected by `EngineBuilder` | LLD secrets adapter owning the redactor |
| D107 | Guarded edits | `vc-parser` produces minimal `TextEdit`s and checks they re-parse; compile-before/after and refusal of new errors live in `vc-engine` | — |
| D108 | App dependencies | Apps depend only on `vc-engine` (which re-exports model types), plus `vc-contracts` for server and MCP | LLD apps → ir |
| D109 | Parameter limit | Clippy `too-many-arguments-threshold = 4` plus an arch-test: at most 3 parameters excluding `self` | — |
| D110 | Launcher | `vc-cli` owns `main` and dispatches `tui`/`mcp`/`ui`; the only app→app edges are cli→{tui, mcp, server} | — |
| D111 | External types in public APIs | `vc-model` and `vc-interfaces` public items may name only std, serde/serde_json, indexmap, smol_str, bytes and `http`; enforced by arch-tests | — |
| D112 | Test kit | `vc-testkit` (fakes, fixture builders, one contract suite per interface trait); dev-dependency only | LLD `ports/contract` |
| D113 | Lock allow-list | Exactly two files may hold a lock: TokenCache (mutex map of slots, never held across an await) and Redactor (`arc-swap`) | — |
| D114 | Size caps | Source file ≤400 lines; trait ≤7 methods; generated and test files exempt | — |
| D115 | Dispatch | Dependencies injected as `Arc<dyn Trait>`; async methods via `async-trait` unless native dyn-async is stable; generics only where a bench proves the need | — |
| D116 | Commits | Conventional commits; CI fails a `refactor:` commit that changes test assertions (tidy first) | — |
| D117 | API snapshots | `cargo public-api` snapshots for model, interfaces and engine from the end of M2 | — |
| D118 | Branch protection | `master`: required gates and cross-OS test checks, CODEOWNERS review with admin bypass, linear history, no force-push | — |
| D119 | CI matrix | Ubuntu runs every gate; macOS and Windows build and test; nightly runs full mutation testing and wall-clock budgets | — |
| D120 | Performance gate | Instruction-count benches (gungraun) on PRs fail at a >5% regression; criterion wall-clock budgets run nightly and open an issue on breach | LLD "fail on 10% in CI" |
| D121 | Gate self-test | `scripts/gates-selftest.sh` applies one bad patch per gate and asserts that gate rejects it | — |
| D122 | Lifecycle timing | `beforeRun` and `beforeAll` run eagerly: planned when their run or file has a selected instance, finished before any instance starts; the needs analysis decides which instances are blocked by a failure; laziness remains in the token cache and cache-store hits | LLD planner step 3 (lazy shared actions); D81 "pulls in only the lifecycle actions selected tests need" |
| D123 | Expect merge key | Check kind + leaf path; the more specific level replaces (api → test → step; the step wins); `custom` and CEL checks append; `expect: { replace: true }` drops inherited checks | — |
| D124 | Test-level keys with steps | Test-level `auth/headers/query/body/poll/expect` are defaults for every step (step wins); body default on GET/HEAD is V3205; unused default is V3206; test-level `save` reads the last step's response | — |
| D125 | CEL and template roots | `response` (`status`, `headers`, `body`, `timing`), `request`, `var` (D53), `env`, `item`, `iteration`, `example`, step ids; bare `status/headers/body` alias `response.*` inside `expect`; a root unavailable at that spot is V2105 | — |
| D126 | Retry, flaky, skipped | Test `retry` re-runs the whole instance with cleanups between attempts; pass after fail is `flaky`; blocked is never retried; `defaults.retry` applies to tests; transport retries are `http.retry`; new `Skipped` state | — |
| D127 | Exit codes | Keeps D81 (0 pass, 1 fail or blocked, 2 nothing ran, 3 load threshold, 4 sync drift) and adds 130 (cancelled by signal) and 70 (internal error, with a report hint and redacted debug bundle); `vc sync --check` exits 4 everywhere | CLI doc `sync --check` = 1 |
| D128 | IDs and dynamic saves | One ID namespace per file across kinds; cross-file references `<fileId>#<id>` (see D138); `function:`/`test:` may declare `provides: [x]`; undeclared dynamic save used is V5108, declared but missing is V5109 | — |
| D129 | Config layering | built-in < user config < `vibranium.config.yaml` < profiles in order < `.env` < `VIB_*` < `--set`; each layer a `ConfigSource`; every value keeps its origin | — |
| D130 | Command milestones | `vc plan`, `vc list`, `--changed` → M2; `--failed` → M3; `vc call`, `unskip`, `new auth/file`, `config explain` → M5; `vc import http` → M6 | — |
| D131 | Fake data memo | A var defined as `${fake.…}` is generated once per instance or iteration; inline `${fake.…}` is fresh each time; seeded by `--seed`, printed on failure | — |
| D132 | Pure compiler and planner | The engine supplies source texts (`SourceProvider`) and preloaded data; the compiler returns the program or diagnostics plus a `CompileCache` the engine persists | LLD compiler discovery with `ignore` |
| D133 | System adapter | `vc-adapter-system`: discovery and file system, clock, random, environment, command runner, git (`Vcs`), config file sources | — |
| D134 | Request pipeline order | timing → timeout → retry (`refreshOn`, `http.retry`) → auth → `beforeRequest` hooks → per-host limit → transport | LLD order (auth outside retry) |
| D135 | Supply-chain and duplication gates | cargo-deny `multiple-versions = deny` with a skip list; bans openssl-sys, v8, deno_core; unknown registries and git denied; jscpd at 10 lines; nextest plus doctests | spike `warn` |
| D136 | Added crates | thiserror, async-trait, tower, hickory-resolver, regex, smallvec, camino, strsim, arc-swap, assert_cmd, gungraun, criterion; rust-embed, zstd, zeroize, rustls-platform-verifier when first used | — |
| D137 | New diagnostic codes | V2006 reserved ID; V2102 covers `${iteration}` outside a loop; V2105 root unavailable; V3205, V3206; V5108, V5109; V5110 command exited non-zero; V5111 render failure at run time | — |

Round 16 additions (full-ID separator, 2 November 2026):

| # | Decision | Choice | Supersedes |
| --- | --- | --- | --- |
| D138 | Full ID separator | An item's full ID is `<fileId>#<id>` (as in Java's `Foo#bar`) everywhere: YAML references, CLI selectors, reports, diagnostics and the UI. A bare dotted word is always a file ID (`users.crud`); inside the same file the short `id` works. A file's ID comes from its `id:` key, not its path; by convention deeper folders add words (`services/payments/refunds/partial.test.yaml` → `payments.refunds.partial`). This removes the ambiguity between `users.create` as a file and as an item | Dotted full IDs `<fileId>.<id>` |
| D139 | File IDs from paths | A file's `id:` is optional; when absent it is derived from the path: drop the conventional root (`services/`, `load/`, else the project root) and the kind suffix, join folders and stem with dots, lowercase, and collapse a stem equal to its folder (`services/users/users.api.yaml` → `users`, `services/payments/refunds/partial.test.yaml` → `payments.refunds.partial`, `load/checkout/peak.load.yaml` → `checkout.peak`). An explicit `id:` must equal the derived one: a mismatch is error V2007, whose certain fix renames the ID and its references. A move that breaks references (file without `id:`) is detected through the compile cache, and V2003 then offers a certain fix that updates them. `vc new` and `vc init` still write `id:`. Two files deriving the same ID are V2001 | D138 "from its `id:` key, not its path"; file `id` required |
| D140 | Instance selector | One example row or repeat of a test is selected with `:n` (`vc run users.crud#rejectsBadEmail:2`), safe unquoted in every shell | `#n` instance selector |

Round 17 additions (PR #1 review, 2 November 2026):

| # | Decision | Choice | Supersedes |
| --- | --- | --- | --- |
| D141 | Request pipeline order | timing → retry (`refreshOn`, `http.retry`) → auth → `beforeRequest` hooks → per-host limit → timeout (per attempt) → transport. A timed-out attempt can be retried; a total cap across attempts, if ever needed, is a separate optional setting | D134 order (timeout outside retry) |
| D142 | File-ID collapse scope | Only `*.api.yaml` files collapse a stem equal to their folder (`services/users/users.api.yaml` → `users`). Other kinds keep both words (`services/orders/orders.test.yaml` → `orders.orders`), so they never clash with the api file; V4003 suggests a better name | D139 collapse for every kind |
| D143 | `http.retry` shape | `{ attempts: 0, on: [connect, timeout, 502, 503, 504], methods: [GET, HEAD, OPTIONS, PUT, DELETE], backoff: { base: 100ms, max: 2s, jitter: true } }`: off by default, idempotent methods only, exponential backoff with jitter. Set in config, overridable per api | D126 left the shape open |

Round 18 additions (M1 template review, 2 November 2026):

| # | Decision | Choice | Supersedes |
| --- | --- | --- | --- |
| D144 | Template syntax error code | Template syntax errors are V2305, retitled "Template or CEL does not parse": an unclosed `${`, an empty `${}`, a bad path segment, malformed pipe arguments | V2305 CEL-only |
| D145 | Pipe call error code | New code V2308 "Unknown pipe or wrong pipe arguments": an unknown pipe name (with a did-you-mean fix when one is close), wrong arity or argument types. V2304 stays for a pipe or path that cannot apply to the value's type. V2307 is left unused | V2304 for unknown pipes |
| D146 | Size unit base | Sizes are binary: `kb` = 1024 bytes, `mb` = 1024², `gb` = 1024³ | Unit base unspecified |

Round 19 additions (M1 parser review, 2 November 2026):

| # | Decision | Choice | Supersedes |
| --- | --- | --- | --- |
| D147 | YAML aliases | YAML aliases (`*name`) are rejected (V1007), so values are never shared or expanded behind the reader and alias bombs cannot happen; the help points to `vars` or `$extend` for reuse. An anchor alone (`&name`) is ignored | Unspecified |
| D148 | JSON reading | `.json` files are read by a dedicated span-keeping JSON reader into the same `Node` tree, added in M1 D when the compiler first reads `.json` files. Until then JSON goes through saphyr with its documented gaps (surrogate-pair `\u` escapes refused, a trailing comma accepted) | JSON via saphyr |
| D149 | Parser diagnostic codes | One code per problem: V1006 duplicate key, V1007 alias or anchor not supported, V1008 tag not supported, V1009 map key not text, V1010 more than one document, V1011 nesting deeper than 128. V1002 keeps the remaining structure errors (indentation, a mapping expected) | V1002 for all YAML structure errors |

Round 20 additions (template answers, 2 November 2026):

| # | Decision | Choice | Supersedes |
| --- | --- | --- | --- |
| D150 | No bare response fields | Every template and CEL expression writes `response.status`, `response.headers` and `response.body`; the roots are `response`, `request`, `var`, `env`, `item`, `iteration`, `example` and step ids. A bare `status`, `headers` or `body` that is not a known step id is V2105, help "write `response.body`", with a certain fix that inserts `response.` | D125 bare `status/headers/body` alias inside `expect` |
| D151 | Literal arguments | Pipe and generator arguments are JSON numbers, `true`/`false` and single-quoted strings only. `null` and double-quoted strings are V2305 with help "use single quotes"; a double-quoted string with no `'` or escapes gets a certain fix that swaps the quotes | `null` and double-quoted arguments accepted |

Round 21 additions (M1/M2 dependency and code answers, 2 November 2026):

| # | Decision | Choice | Supersedes |
| --- | --- | --- | --- |
| D152 | HTTP dependencies | TLS is rustls with the `ring` provider (aws-lc-rs carries an OpenSSL licence term); root certificates come from `rustls-native-certs` with rustls's webpki verifier; the DNS phase is timed around `tokio::net::lookup_host` (no hickory). `deny.toml` skips `getrandom@0.2` and `windows-sys@0.52`, both pulled only by `ring` 0.17 | D136 hickory-resolver and rustls-platform-verifier |
| D153 | CEL time functions | `timestamp()` (RFC 3339) and `duration()` (Go-style `1h30m`) are our own functions registered like `sum()`, not the `cel` crate's `chrono` feature, whose dependencies fail `cargo deny`. Until they land (by M4) both are V2305 "not supported yet" | LLD open question |
| D154 | Unreadable file code | New code V1012 "File cannot be read": unreadable, not UTF-8, a link outside the project root, or a name that is not a valid path, each with its own `help:`. V1001 stays for YAML/JSON syntax | V1001 for unreadable files |

Round 22 additions (M1 linker answers, 3 November 2026):

| # | Decision | Choice | Supersedes |
| --- | --- | --- | --- |
| D155 | Compile cache encoding | The compiled file units in `.vibranium/cache/compile.bin` are encoded with `postcard` (serde, binary), not JSON, to meet the warm budget (JSON decoding took ~120 ms of 160 ms for 1,000 files). The layout version is bumped; any other version reads as an empty cache | Cache layout 4 (JSON units) |
| D156 | Poll defaults | `poll` has no defaults: `every` and `timeout` are both required next to `until`; a missing one is V1005 | — |

Round 23 additions (Google Rust Style Guide, 4 November 2026):

| # | Decision | Choice | Supersedes |
| --- | --- | --- | --- |
| D157 | Coding style | The Google Rust Style Guide, copied unchanged to `docs/style/google-rust-style-guide.md` (CC BY 3.0, attributed in `NOTICE`), is the project's coding style. The project rules (CLAUDE.md, the LLD's gates) win where they differ, and the guide's Chrome-only rules (small crates, flat layout, no async, no threads) do not apply | — |
| D158 | Trait objects and generics | Stored, long-lived dependencies (ports, registries, anything that picks an implementation at run time) stay `Arc<dyn Trait>`. A parameter used only during the call takes a generic or `impl Trait` (`impl AsRef<str>`, `impl IntoIterator`, closures). The ports do I/O, so dynamic dispatch costs nothing measurable, and registries need mixed types | D115 "generics only where a bench proves the need" |
| D159 | Imports | One module per `use` line; no glob imports except `use super::*` in an inline test module and `pub use submodule::*` in a non-leaf module; free functions are called through their module (`check::evaluate(..)`), while types and traits are imported and used unqualified. The existing code is converted in one `refactor:` commit | Unspecified |
| D160 | Exhaustive public types | Each exported enum, and each exported struct whose fields are all public, chooses explicitly. Error enums and types no other crate builds or matches are `#[non_exhaustive]`. Types built or matched across crates in this workspace (the `vc-model` data types, shared events and results) use the guide's large-scale-change exception: every user is in this repo and a change fixes them in the same PR, so they keep struct literals and exhaustive matches, which catch a missed variant in the state machines. They carry `#[expect(clippy::exhaustive_enums, reason = "...")]` (or `exhaustive_structs`), as do the guide's other exceptions (closed sets, two-state enums, `..Default::default()` options structs) and the six types in `crates/arch-tests` | Unspecified |
| D161 | Style lints | `clippy::exhaustive_enums` and `clippy::exhaustive_structs` are added to `[workspace.lints]`, so D160 is machine-checked; the rest of the guide is a review item | — |

Round 24 additions (combined M3–M5 kick-off, 5 November 2026):

| # | Decision | Choice | Supersedes |
| --- | --- | --- | --- |
| D162 | Delivery | M2, M3, M4 and M5 ship together as one PR to `master`. M3–M5 have a single combined kick-off (this round). After it, work runs without stopping, using the documented defaults, and each default taken is logged here | D96 (a kick-off per milestone); one PR per milestone |
| D163 | Default secrets provider | Plain env/`.env` is the default `secrets.provider`; Varlock is an opt-in provider behind the same `SecretProvider` port | LLD "Varlock is the current default" |
| D164 | Notifiers | Notifiers stay in M8, as in the LLD | PRD release column for notifiers |
| D165 | Secret names | `.env.schema` declares secret names for every provider, plain env included. A `.env` value whose name is declared there is reachable only as `${secret.X}`, never as `${env.X}`, and is masked everywhere. `vc check` flags `${secret.X}` names that aren't declared | — |
| D166 | Cache encryption | Every cross-run cache entry is encrypted with chacha20poly1305. The key comes from the OS keychain or `VIB_CACHE_KEY`. With no key, the cross-run cache is off for that run, with a one-line warning (new V5 code) | "secret-derived values are simply not cached" |
| D167 | Report replay | Each run stores a self-contained snapshot in history: CTRF plus test names and source spans. Reporters take a run snapshot rather than the live `Program`/`Plan`, so `vc report <run>` replays correctly after edits. Reports are written to `.vibranium/reports/<runId>/` | Flat `.vibranium/reports/report.html` |
| D168 | Auth types in M3 | basic, bearer, apiKey, oauth2ClientCredentials, oauth2Password, flow, oauth2Refresh and oauth2AuthorizationCode (PKCE, browser and keychain). `plugin` auth ships with the plugin host in M8. The token-cache key hashes resolved vars with a keyed BLAKE3, so secret values never appear in plain hashes | PRD R14 (refresh and authorization code in v2.2) |
| D169 | HTML report assets | M3's HTML report is hand-written vanilla JS/CSS under `web/report/`, embedded at compile time, so builds need no Node. It moves to the Vite/Svelte stack with the M9 web UI | LLD "built by Vite at release time" (until M9) |
| D170 | History contents | By default, bodies are stored for failures only (`history.bodies: none\|failures\|all`), and the last 50 runs are kept, pruned at the end of each run (`history.keep: <n>\|all`), plus `vc history prune`. Everything is redacted before it is written. `--failed` reruns the failed or blocked tests of the last run with the same profiles, and exits 2 when there is none. History is SQLite in WAL mode with one writer actor fed by a channel | `history.retain` (syntax reference) |
| D171 | User redaction rules | Config `redact:` lists user rules, each a `regex`, a CEL expression or a hook function. They are applied by the redactor, after the built-in secret masking, before history, reports and logs (hook rules from M4) | — |
| D172 | Hook analysis | Hooks are parsed, type-stripped and checked for exports and imports at compile time, by a pure step using oxc (added to the crate table as a compiler dependency). The stripped JS lives in the compile cache, so `vc check` reports V2005 and bad imports. The watchdog is a reply timeout plus the QuickJS interrupt handler. New codes cover hook parse, import, budget overrun and thrown errors | — |
| D173 | Datasets | The engine reads dataset bytes and the parser parses them, using the `csv` crate (added to the table) for CSV. Columns are checked at compile time. Files outside the project root are allowed only under folders listed in config `datasets.allowRoots`, and V1012 applies otherwise. Dataset hashes count for the compile cache and `--changed` | syntax-reference "any relative path" |
| D174 | Faker and generators | Only the names in the syntax reference, through a registry mapping each Faker JS name to a `fake` call with an output type: dates are RFC 3339 strings, prices are numbers, the locale is en. `fromRegExp` uses our own generator for a bounded regex subset, with a V2 error for unsupported syntax. Every value comes from the seeded `Random` port. CEL `timestamp()`/`duration()` (D153) land in M4 | — |
| D175 | `vc demo` | A separate small app crate, `vc-demo`, with its own canned routes. The arch-tests layer table gets an entry for it (approved). `vc demo start --detach` re-launches the current executable and writes a pidfile, and `vc demo stop` reads it | Waiting-on-Sarath item "does `vc demo` reuse the mock server" |
| D176 | M6-dependent flags | `vc new api --from-curl` moves to M6. `vc new test --negative` works only when the api declares a body schema. `vc call --save-as-test` writes the status and the `--expect-field` assertions, with no inferred schema | — |
| D177 | URL form and import selection | M5 adds `vc new api <id> --url <full-url>`, which splits the URL into a base-URL var plus path and query, defaulting to GET. In M6, `vc import openapi` gains `--include <tags>`, `--only <operationId\|"METHOD /path">`, and an interactive picker on a terminal (everything when not a terminal) | — |
| D178 | `vc call` | Runs the api's registered cleanup afterwards unless `--keep`. Records the call in history as `kind=adhoc`. Resolves auth but skips `beforeRun`/`beforeAll`. Follows the same agent/MCP approval rules as `run` | — |
| D179 | `--changed` | With no ref, `--changed` compares the working tree against HEAD: uncommitted and untracked files. `--changed <ref>` compares against that ref. A changed `vibranium.config.yaml` or `.env.schema` selects every test | — |
| D180 | Defaults taken | History in SQLite WAL with one writer. `vc explain X` treats X as a code when it matches `V\d{4}`, and as an ID otherwise. `vc watch` uses notify with a 200 ms debounce over the compile cache. `vc fmt` takes key order from the shape definitions. V4 lints are warnings, with no new `lints:` key, and `check --fix` applies only certain fixes. CLI `--var` wins over the PRD's `--with`. The LLD milestone order wins over the PRD release column. Completion uses `clap_complete` plus a hidden `vc __complete` | — |

Round 25 additions (orchestrator defaults during M2 finish, 5 November 2026):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D181 | `command:` safety | A `command:` line is run by `sh -c` (unix) or `cmd /C` (Windows) in the project root, with the process environment plus `.env` and the run's vars. Each interpolated `${…}` value is quoted as one POSIX word on unix; on Windows a value holding `& \| < > ^ % ! " ( )` or a newline fails the action (V5111). Captured stdout and stderr keep the last 64 KiB each and pass through the redactor; a non-zero exit is V5110 with the exit code and the last lines. A timeout kills the whole process group | — |
| D182 | `VIB_*` names | Settings map by upper-snaking the `--set` key (`VIB_PROFILE`, `VIB_DEFAULTS_CONCURRENCY`, `VIB_DEFAULTS_PER_HOST`, `VIB_DEFAULTS_TIMEOUT`, `VIB_REPORTS_DIR`, `VIB_REPORTS_FORMATS`); vars use `VIB_VAR_<name>` with the name exactly as written (`VIB_VAR_userId`), because upper-snaking a camelCase name loses information | — |

## Open

All decisions needed for the PRD and technical design are settled. The language is decided by the spike (D6, D36, D37, D41).

- The mockup seed project (`design/mockups/generators/editor-src/seed/`) and the generated canvases predate D138–D140 (dotted full IDs, `id:`/path mismatches). Update them at the M9 kick-off, before they are used as web UI fixtures.

Carried to milestone kick-offs (Round 15):

- Default secrets provider: Varlock (a Node CLI) or environment variables with Varlock opt-in — decide at M3.
- Notifiers and alerts: PRD R21 says v2.0, the LLD places them in M8 — decide at M3.
- Whether `vc demo` reuses the mock server crate — decide at M5.

## Parking lot

- Mocks generated from `*.api.yaml` and scheduled monitoring: after the web UI (D38).
- GraphQL, gRPC and WebSocket/SSE executors: after v2.0 (D4).
- UI testing through HTML/XML parsing: later (D4).
- Distributed load generation beyond one machine: not in v2 (D8).
- OpenTelemetry spans, OpenAPI-driven fuzzing, traffic-recording to tests: v2.3+.
