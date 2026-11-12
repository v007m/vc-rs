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

Round 26 additions (5 November 2026):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D183 | `syn` 2 alongside `syn` 3 (Sarath) | `deny.toml` skips `syn@2`: oxc's `oxc_ast_macros` (hook analysis, D172) needs `phf_macros`, which every release builds on `syn` 2, and the `gungraun` dev-dependency uses it too. `syn` is build-time only, so the shipped binary is unaffected. Drop the skip once `phf_macros` moves to `syn` 3 | D135 (in part) |
| D184 | Mask, then cut | `Event::Exchanged` carries full transport-capped bodies; `Redactor::event` masks the URL, headers and both bodies; only the history rows, run snapshot and report formats cut a body to `history.maxBodySize`, after masking, on a UTF-8 boundary, marking it truncated. Cutting first would let a secret split by the cut, or a JSON field past it, escape masking | — |

Round 27 additions (5 November 2026):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D185 | Async redactor (Sarath) | `MaskRule`, `Redactor::event` and `RedactingSink::publish` become async, so a `function:` redact rule (D171) can call the `HookRunner` within its hook budget without blocking the runtime. The built-in secret masking stays first and synchronous; user rules run after it in config order. Hook redact rules ship in M4 | — |
| D186 | V5108 in M4 (Sarath) | V5108 ships in M4 with V5109: each dynamically saved value carries its origin action, so a read of a value that a `function:`/`test:` action saved without declaring it in `provides` emits a V5108 warning event and the run continues | — |
| D187 | Keychain backends | `OsKeychain` uses `keyring-core` with one native store per target: the macOS keychain, the Windows credential manager, and the Linux kernel keyutils session keyring (no D-Bus). Other targets have no keychain, so `VIB_CACHE_KEY` applies. `keyring` 4's default Linux store pulls in `zbus` 5, whose `zvariant`/`zbus_names` crates carry unmaintained advisories (RUSTSEC-2026-0370..0372) and add `syn` 2 users. On Linux the key lasts one login session; a lost key makes cache entries miss, never fail | LLD crate table (`keyring` 4) |
| D188 | `vc-demo` binary (Sarath) | `vc-demo` ships its own binary next to `vc`; `vc demo start` launches it by path (the directory of the current executable, then `PATH`), and `--detach` writes the pidfile that `vc demo stop` reads. `vc-cli` gains no dependency on `vc-demo`, so the app edges stay cli → {tui, mcp, server} (D110). Installers ship both binaries | D175 (in part: "re-launches the current executable") |

Round 28 additions (9 November 2026):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D189 | Params rule and foreign traits (Sarath) | ARCH-PARAMS skips functions in an `impl <Trait> for X` whose trait is declared outside the workspace, because the upstream crate fixes the signature (rquickjs `Resolver::resolve` takes 4). Impls of workspace traits and inherent functions still count | D109 (in part) |
| D190 | `notify` and CC0 (Sarath) | `deny.toml` allows `CC0-1.0` (a public-domain dedication, used by `notify`). `vc watch` uses `notify` 8.2.1 behind the `FileWatcher` port in `vc-adapter-system`, as D180 says; its other dependencies add no duplicate versions | — |
| D191 | When an api's `save` runs (Sarath) | An api-level `save` runs only when the response status matches the api's `saveOn:` status matcher (same syntax as `expect.status`: `201`, `2xx`, `[200, 201]`). `saveOn` defaults to the api's own `expect.status`, or `2xx` when it has none. A test that expects another status (such as a 400) therefore skips the api's saves, the same way cleanup is not registered. Test-level and step-level `save` are unaffected | D62 (in part: "applies to every caller") |
| D192 | Action `timeout` (Sarath) | `timeout` on an action is that action's own limit: a `command`'s process timeout, a `function`'s run budget, a `call`'s per-attempt request timeout (it wins over the api's and `defaults.timeout`), and a reused `test`'s whole-test limit. Without it, each kind keeps its current source | — |
| D193 | Short-lived tokens | A cached credential counts as stale when its remaining life is under `min(refreshBefore, lifetime / 2)`, so a token that lives no longer than `refreshBefore` is not refetched at once. The request that fetched a credential always uses it | — |

Round 29 additions (9 November 2026, M6–M7 kick-off):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D194 | v1 migrator name (Sarath) | The v1 → v2 migrator is `vc migrate v1 <path>`, built on the importer and draft pipeline. `vc migrate` with no argument stays the `syntaxVersion` upgrade. cli-workflow's `vc import v1` is renamed | — |
| D195 | Agent policy semantics (Sarath) | The staging example requires approval for `PUT`, `PATCH`, `DELETE`, and `writeDefinitions: ask` goes through the same approval flow. A profile with no `agents` block allows only reads (list, search, get, check, query history). With several active profiles, the most restrictive rule wins for each field. `run_tests` is gated by `run` plus the methods its planned tests send, with one approval per run. "Always allow" (D92) is kept in history.db per profile, tool and method, and never edits YAML | — |
| D196 | Load and `perHost` (Sarath) | A load run ignores `defaults.perHost` unless the load file sets its own | — |
| D197 | MCP build (Sarath) | `vc-mcp` uses rmcp (features `server` and `transport-io` only) with hand-written tool schemas taken from engine types. The vc-contracts proto waits for its first consumer, M9's web API (D101). M7 ships stdio only; `/mcp` over HTTP comes with the web server in M9 | — |
| D198 | M7 scope edges | M7 load output is the live summary, `load_intervals` and a JSON summary; the HTML load report comes with the web UI (M9). The published agent skill (PRD R26) moves to M8. `hdrhistogram` is used without default features. HAR and OpenAPI parsing use maintained crates only if they pass cargo-deny; otherwise hand-written serde structs over vc-parser output, with the choice logged when the spike ends | — |

Round 30 additions (9 November 2026, M6–M7 dependency and v1 spikes):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D199 | Import and load crates (spike result, per D198) | All candidates pass cargo-deny with no new duplicate versions and no deny.toml change. Used: `har` 0.9, `oas3` 0.22 with `default-features = false` (its default `preserve-order` would switch `serde_json::Map` to IndexMap across the workspace and change JSON key order), `hdrhistogram` 7.6 without default features, `rmcp` per D197. `openapiv3` is not used: OpenAPI 3.0 is up-converted to 3.1 (like Swagger 2.0) and read with `oas3`, so there is one OpenAPI model. YAML specs go vc-parser → `serde_json::Value` → `oas3` | LLD crate table (`openapiv3` for 3.0) |
| D200 | `vc migrate v1` conversions (spike of the v1 source) | Anything with no v2 form is left out of the output, listed in the migrate report, and marked with a `# TODO(vc migrate)` comment that keeps the v1 text: JavaScript asserts beyond the common shapes (equality → `body.match`, `.length`, `.every`/`.filter(...).length >= 1` → `every`/`some`), and all `scripts`. Built-in v1 datasets become Faker calls with a note (names → `person.fullName`, countries → `location.country`, the rest → `lorem.word`); their contents are not copied. `{lorem_N}` (N characters) becomes a named `fromRegExp` generator `[a-z]{N}`. A string is a generator only if it has regex metacharacters; `[A\|B]` becomes `arrayElement(A, B)` with a note that v1 read it as a character class. `repeat-until` becomes `poll` with `every` = `repeat-delay` or 1s and `timeout` = repeat × every, or 30s. `delay`, `repeat-delay` on a plain repeat, `async` and `timing.wait` are dropped with a note. `cache: true` becomes a `beforeAll` call with `save` (same run, as in v1). Account `variables` become file vars on the test files that use that account | — |
| D201 | JSON Schema validator (Sarath) | `body.schema` uses `jsonschema` 0.61 with `default-features = false` (no remote or file `$ref` resolution; `$schema` picks draft-04 to 2020-12, 2020-12 when absent). deny.toml gains a scoped `[[licenses.exceptions]]` allowing MIT-0 for `borrow-or-share` only, and skips, each with a reason, for the duplicates it brings (`getrandom@0.3`, the older `r-efi`, `bit-set@0.8`, `bit-vec@0.8`, `num-bigint@0.4`), in the commit that adds the crate | — |

Round 31 additions (9 November 2026, M7 agent policy review):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D202 | Agent runs that execute code (Sarath) | An agent's run (`run_tests`, and `call_api` when its chain reaches one) whose plan contains any `command:` or `function:` action always asks for approval, even when `run` and every method are allowed; a standing "always allow" rule never skips this ask. The approval request names each such action and its command or function. Runs started by a person are unchanged. Closes the path where `writeDefinitions` plus `run` would let an agent write and run a shell command | D195 (`run_tests` gated by `run` plus methods only) |
| D203 | Request hooks and the agent policy (Sarath) | D202 covers `command:` and `function:` actions only. Request hooks (`function:` entries on an api file or api that run around each request) do not force an ask when an agent's call or run reaches them: they are written by a person, run in the sandboxed QuickJS engine with no file, process or shell access, and an agent cannot write files under `hooks/` | — (clarifies D202) |
| D204 | Agent calls that fail before the policy gate (Sarath) | An agent call that fails before the policy gate (a bad selection, a compile error) still gets an `agent_actions` row, with outcome `failed: <reason>`, so the log records every call an agent made | — |

Round 32 additions (9 November 2026, CI for batch PRs):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D205 | Mutation testing on batch PRs (Sarath) | On a PR update (`synchronize`), CI's mutation step measures only the commits pushed since the PR's previous head (`github.event.before`); a PR's first run, or a rewritten history, measures against the base branch. A batch PR's whole diff (2,748 mutants on PR #11) does not fit in GitHub's 6-hour job limit. Commits in a push whose run was cancelled by a newer push are covered by the author's local `GATES_MUTANTS=1` run, which every lane does before landing | D119 (PR mutants on the whole diff against the base) |

Round 33 additions (10 November 2026, M6 import gaps; decided by the orchestrator while Sarath was away, none changes an earlier row):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D206 | Which file a recorded host goes to (orchestrator; Sarath away) | `vc import har` sends each recorded host to the api file whose `baseUrl`, rendered under `--profile` (or the defaults), has the same origin. If several match, the one with the longest base path covering all of the host's paths wins; if two still tie, the last one found wins. A host that no file matches gets a new file. Its file ID is the host's first label that is not `www` or `api`; for an IP address it is `recorded`. If that file ID is already used by a file pointing at another host, the import is refused, with help suggesting `--profile` or `vc rename`. Third-party hosts are those whose registrable domain (the last two labels) differs from every profile's base URL; they are dropped unless `--include-host` names them. Static assets are dropped unless `--keep-static` is given | — |
| D207 | Reusing apis and handling secrets on HAR import (orchestrator; Sarath away) | A recorded request reuses an api in its file with the same method and path shape (var names ignored), and `--flow` steps rename path vars by position. New apis get the next free ID (`addItem2`). Credential headers and cookies are dropped from the recorded requests and their values are never written; each name goes on the summary's "secrets" line, and it is appended to `.env.schema` when that file exists (otherwise only printed). Each host line names the file's auth, or suggests `vc new auth <file>Auth --type bearer`. The e2e test greps every written file and all output for the recorded values | — |
| D208 | `vc migrate v1` output (orchestrator; Sarath away) | Migration goes through the same compile guard as every engine edit (D107): if the converted project does not compile, nothing is written. A v1 construct with no v2 form becomes a `TODO` comment above the item's `- id:` line (or above `tests:`/`apis:` for a file-wide one), and the command lists every TODO. A v1 schema is inlined as `body.schema`; a test's ID is its api's ID in the third person; `timing.wait`, `delay`, `async` and a repeat's delay become notes; a var only a script set becomes `""` with a TODO. The command refuses (exit 2) when a config already exists or the v1 folder is outside the project. Auth secret names go to `.env.schema`; no secret value is written | — |
| D209 | Bruno import input (orchestrator; Sarath away) | `vc import bruno <file>` reads Bruno's JSON collection export (one file, like every importer). Post-response vars become `save:`; secret environment vars become `${secret.X}` references. Given a folder of `.bru` files, it says to export the collection as JSON from Bruno. Reading `.bru` folders directly is parked for Sarath | — |
| D210 | Postman import (orchestrator; Sarath away) | `vc import postman <file>` accepts v2.1 collections only (other versions error). A request whose base URL has no value goes to `http://localhost` under the fallback file ID, with a note. Non-credential headers are noted and not imported (api files have no headers). Auth blocks are dropped and mapped to the file's auth (or a `vc new auth` hint); an apikey in the query becomes a `${secret.X}` reference; credential headers are dropped with a note. Scripts are not imported except test-script saves: `pm.*.set`/`postman.set*` of a response-body field (also through a `pm.response.json()` alias) becomes a step `save:`; pre-request scripts get a note. `{{$random*}}` maps to fake generators through one table; other dynamic vars stay as written, with a note. The api ID is camel(request name); same method and path stay separate apis (no HAR-style merge). A `{{var}}` with no value is `""` with a note, unless a script saves it, in which case it's a flow step var. Formdata/file bodies are dropped with a note; urlencoded becomes a form body; GraphQL becomes JSON `{query, variables}`. Secret-like fields and high-entropy values become `${secret.NAME}`, proposed for `.env.schema` as in D207 | — |
| D211 | `.http` import (orchestrator; Sarath away) | `vc import http <file>` follows the curl rules: one origin per file, and a request to another origin is skipped with a note. The api name is the `###` title, else `# @name`. A file variable named like a credential is never substituted, so its value is never written. An undefined variable becomes the var `""` with a note; at the start of a URL it sends the request to `http://localhost` with a note to set the base-URL var. A response variable (`{{login.response.body.$.id}}`) becomes a var (`loginId`) with a note. `$guid`, `$uuid` and `$random.uuid` become `${fake.string.uuid}`, and `$randomInt a b` becomes `${fake.number.int(a, b)}`; other dynamic vars are kept as written, with a note. A body without Content-Type is JSON if it parses, else text. Response handlers (`>`) and file bodies (`<`) are noted and not imported. A missing method means GET. A request line that is not a full URL, a path or variable-led, or a file with no request, makes the file invalid | — |
| D212 | Bruno import details (orchestrator; Sarath away) | Per D209, a folder exits 2 ("it is a folder, not a file") with help to export the collection as JSON; the help comes from `Importer::source_help()`, and a file that isn't a Bruno export is invalid with the same hint. Vars are the collection's root vars, overridden by the first environment's enabled values (other environments get a note). Env vars marked `secret`, `{{process.env.X}}` and credential-named vars become `${secret.<FILE>_<NAME>}` references, and their values are never written. A post-response var reading `res.body…`/`res.getBody()…` becomes the step's `save:`; other post-response vars, scripts, tests, assertions and request vars are noted, not imported. Items follow `seq`; grpc/ws items are skipped with a note. Auth inherits request → folder → collection (`none` sends none); an apikey in the query becomes a query secret reference, other auth is dropped with a note. Folder and collection headers are noted, not imported. Body modes: json/text/xml/sparql raw with their content type, formUrlEncoded as a form, graphql supported, multipart/file left out with a note. Query comes from `query` params (else the URL); path values from `path` params | — |

Round 34 additions (10 November 2026, PR #11 review fixes; decided by the orchestrator while Sarath was away, none changes an earlier row):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D213 | What an importer takes for a secret value (orchestrator; Sarath away) | A word is a secret when it matches either rule. (a) 24 or more characters with lower case, upper case and digits, at least 16 different characters, and not shaped like an ID. (b) 32 or more ASCII letters and digits, with both letters and digits present, letters of one case only (hex included), and at least 10 different characters; the ID-shape exclusion does not apply. A secret becomes a `${secret.NAME}` reference and its value is never written. Trade-off: a 32-or-more-character hex ID (a UUID without dashes, a SHA-1) is taken for a secret, even in a path; a 24-character ObjectId, a dashed UUID, a prefixed ID (`ord_…`) and slugs are not. Writing a real key into git is worse than a reference the user has to undo | — |
| D214 | OpenAPI import IDs and `--include` (orchestrator; Sarath away) | IDs are given to every operation in spec order before `--include`/`--only` apply, so a filter never changes an ID. Clashes are numbered by the shared free-name rule (`fetch`, `fetch2`, `fetch3`). An `--include` path prefix matches by whole segments: `/user` keeps `/user` and `/user/{id}` but not `/users`, and a prefix ending in `/` keeps only what is under it | — |
| D215 | Failed logins in the token cache (orchestrator; Sarath away) | A failed fetch goes to the requests already waiting on it. Then a failure that may pass (`AuthError::may_pass`: unreachable, timeout, 408, 429, 5xx) is dropped, so the next request fetches again; any other failure is kept for the run, so tests don't each retry a login and lock the account. That covers a refusal (`AuthError::refused`: 400, 401, 403) and every failed `flow` login, as the syntax reference already said. A kept refresh token is forgotten only on a refusal. Treating a flow step's timeout or transport error (V5104/V5105) as one that may pass is left for Sarath | — |

Round 35 additions (10 November 2026, M8 alerts, plugins and the run feed; decided by the orchestrator while Sarath was away, none changes an earlier row):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D216 | Alerts and notifiers (orchestrator; Sarath away) | An alert rule defaults to `on: [failure]`, and empty filters match every run. Its key is the run's profiles joined by `+` (`default` without one); PagerDuty's `dedup_key` is `<project>#<key>`, where the project is the config's `project` or else the root folder's name, and its severity defaults to `error` (one of `critical`, `error`, `warning`, `info`). Payloads: Slack Block Kit, a Teams Adaptive Card, PagerDuty Events v2, and the CTRF summary JSON for a webhook. One notice per rule, by trigger precedence, sent through the run's transport once the reporters finish. `NotifierFactory` gets the resolved secrets. A channel that can't be built or sent to is a V5117 warning in the run's notices, with the exit code unchanged and no URL or key in the message; `vc check` also warns V5117 at a `notify` key that names no known channel. The email notifier, `vc alerts test` and a `blocked` trigger are left for Sarath | — |
| D217 | Plugin install and protocol (orchestrator; Sarath away) | `vc plugin add <source> [--yes]` takes `npm:` or `path:` sources only, stages, shows the manifest's (`vibranium-plugin.yaml`) permissions, and on approval writes them to `vibranium.lock` (YAML: id, source, resolved, integrity, permissions) and installs to `.vibranium/plugins/<id>/`. Without a terminal it needs `--yes`, else it exits 2. The protocol is JSON-RPC 2.0 over stdio with LSP `Content-Length` framing: `initialize`/`shutdown`, `reporter/onEvent`, `reporter/finish`, `auth/credentials`, and `secrets/get` from plugin to host (an unknown method is −32601). One lazy process per plugin per run; `initialize` gets 5 s and a call 30 s. A plugin error is V5118 (−32001 permission, −32002 deadline) and fails only the work that used it. A run refuses a manifest that asks for more than the lock grants. Permissions are declared and checked, not sandboxed: Landlock enforcement is left for Sarath (it needs a re-exec launcher under `unsafe_code = "forbid"`, and it filters ports, not hosts) | — |
| D218 | Plugin reporters, auth and config (orchestrator; Sarath away) | `reports.formats: [<pluginId>]` resolves through the reporter registry to a plugin reporter; any other unknown format stays V2302. Auth `type: plugin` asks the plugin for credentials (`AuthError::Plugin`, V5114). `plugins[].config` takes literals only, and a `${…}` in it makes the plugin unusable for the run, with a notice. An auth item's `config` under `type: plugin` is rendered, `${secret.x}` included: the author names that secret for that plugin, so it goes to the plugin whatever the lock grants, masked by the redactor like every run secret. **Flagged for Sarath**: he may want the lock to gate this too | — |
| D219 | The live run feed (orchestrator; Sarath away) | Apps watch a run through `RunRequest::watch`. `RunFeed` (non-exhaustive) is `Started(Names)`, `Event(Sequenced)`, `Missed { count }` or `Done(RunReport)`; watching never blocks or fails the run, a full channel drops events and reports how many, and `Done` comes once the reporters have finished. A watched run prints nothing to stdout. `vc_model::Feed` is unchanged. `Names` moves from the reporters adapter to `vc-model`, so every app labels events the same way | — |
| D220 | History for the screens (orchestrator; Sarath away) | `HistoryReader::trends(window)` sums up each test over the newest `window` runs of kind `run`: results that ran, failed or blocked, flaky, and mean wall time. `HistoryReport::runs` and `StoredRun::run` hand screens typed rows (`RunRow`, `TestRow`, `RunId`, `TestTrend`, re-exported by `vc_engine`) | — |

Round 36 additions (10 November 2026, M9 lane C: the Connect layer, the local web server and `vc ui`; decided by the orchestrator while Sarath was away, none changes an earlier row):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D221 | Connect wire format (orchestrator; Sarath away) | `vc-server` speaks Connect with binary protobuf only: `application/proto` for unary calls and `application/connect+proto` for streams, at `/api/vibranium.engine.v1.EngineService/<Method>`. Errors are Connect error JSON with the 16-code table and never echo the request, and unary bodies and stream envelopes are capped at 4 MiB. JSON bodies wait on pbjson, which is parked; the web client sets `useBinaryFormat: true` | — |
| D222 | The local server's token and guard (orchestrator; Sarath away) | `vc ui` binds `127.0.0.1` only. Each process makes one 256-bit CSPRNG token (43 base64url characters) and compares it in constant time. `GET /?token=…` sets the `vc_token` cookie (`HttpOnly; SameSite=Strict; Path=/`, no `Secure` on plain-HTTP loopback) and answers 303 to `/`; a `token` query anywhere else is 400. Every other request carries the cookie or `Authorization: Bearer` (401 without). `Host` must be `127.0.0.1:<port>` or `localhost:<port>`, and a write from another `Origin` is 403. There is no CORS. Every response has `no-store`, `nosniff` and `no-referrer`, and the SPA fallback sits behind the guard. vc reads no token from the environment; `VC_UI_TOKEN` is read only by the Vite dev proxy | — |
| D223 | The web UI's CSP (orchestrator; Sarath away) | The server builds the `Content-Security-Policy` header itself: `default-src 'self'; script-src 'self' <hashes>; base-uri 'none'; form-action 'self'; frame-ancestors 'none'`. Only the `'sha256-…'` sources of the embedded `index.html`'s CSP meta are copied in, so a meta that allows more cannot widen the policy. Without hashes it is `default-src 'self'; frame-ancestors 'none'` | — |
| D224 | `vc ui` (orchestrator; Sarath away) | `vc ui [--port 7420] [--no-open] [--profile] [--set]` exists only in a vc built with vc-cli's `web-ui` feature, which is off by default; other builds exit 2 with "built without UI". The files in `web/build` are embedded with `rust-embed` (`debug-embed`, so debug builds read nothing from disk), and a build made before `web/build` exists warns and serves only `/api` and `/mcp`. The URL with the token is printed in one stdout line, `vc ui: http://127.0.0.1:<port>/?token=…`, whether or not a browser opens, and the token appears in no log, error, report or history. A port that can't be listened on exits 2 with a `--port` hint, and Ctrl-C exits 0. CI lints and tests the feature in a separate step, which also builds `vc-demo` because vc-cli's tests run that binary | — |
| D225 | MCP at `/mcp` (orchestrator; Sarath away) | `vc ui` mounts vc-mcp's streamable HTTP service (rmcp `transport-streamable-http-server` behind vc-mcp's `http` feature, session mode with the local session manager) at `/mcp`, behind the same guard as `/api`. An agent sends `Authorization: Bearer <token>` and needs no `Origin`. The tools and the profiles' `agents` rules are those of `vc mcp` (D195, D197). Stopping `vc ui` cancels the open sessions and streams | — |
| D226 | The web UI's M9b engine calls (orchestrator; Sarath away) | `EngineService` adds `Call`, `SaveCallAsTest`, `Import`, `ListRuns`, `GetRun` and `Trends`, each a thin adapter over the engine facade, and the `Run` stream sends `RunPlanned` with the plan's instances right after `RunStarted`. `ListRuns` and `GetRun` reuse the history's `results.v1.Run` and `Test`. `Import` takes the spec or recording as text, because the browser has no path; that is a new source for the importer, and D207 still holds. There is no per-test history RPC, since the history reader has none. An agent action has no outcome or approval ID, and an approval has no reason or rule, because the engine stores none of these | — |
| D227 | The web UI editor's files (orchestrator; Sarath away) | `ReadFile` and `WriteFile` open only `vibranium.config.yaml` and the api, test and load files outside hidden folders, so `.env`, `.git/` and `.vibranium/` are never read or written through the UI. A file's digest is its BLAKE3 hash in lowercase hex; a save whose file changed, appeared or was removed since it was opened writes nothing and fails `Aborted`. A save is not guarded (D107): it keeps text with errors and returns the project's diagnostics | — |
| D228 | The run stream's start (orchestrator; Sarath away) | `RunStarted.run` is unset, since the history gives a run its ID only when it is stored, and `started_ms` comes from the server's clock | — |
| D229 | `ExplainConfig` (orchestrator; Sarath away) | The web UI's effective settings are masked by the redactor a run would use: a declared secret's value wherever it appears, a var named like a declared secret or a `redact.fields` entry whole, then `redact.rules`, in every layer including those replaced. When the secrets cannot be loaded, every value but the built-in ones is masked and a notice says so | — |
| D230 | A pasted import's name (orchestrator; Sarath away) | An `Import` given text names its snapshot after the spec's or recording's title, since the browser gives no file name | — |
| D231 | Links into hidden files (orchestrator; Sarath away) | The file source refuses a path that resolves out of the project or, through a link, into a hidden file or folder the path does not name. This holds for every command, so `vc check`, `vc run` and authoring refuse such links as well as the editor | — |
| D232 | `vc config explain` masks secrets (orchestrator; Sarath away; **flagged for Sarath**, a security fix) | `vc config explain` masks every value as `ExplainConfig` does (D229): a declared secret's value wherever it appears, in the replaced layers too, and a var named like a secret whole; when the secrets cannot be loaded, every value but the built-in ones is masked and a warning says so. It used to print them raw | — |
| D233 | The web UI's agent name (orchestrator; Sarath away) | A `Call` from the web UI goes through the profiles' agent policy and the agent log as agent `web-ui` (D178) | — |
| D234 | `vc ui`'s layers under a request's (orchestrator; Sarath away) | A request that names no profile uses the profiles `vc ui` was started with (the config's defaults when none); the server's `--set` values apply first and the request's after them, so the request's win | — |
| D235 | The default trend window (orchestrator; Sarath away) | `Trends` with window 0 reads the last 30 runs, the middle of the Insights screen's 10, 30 and 100 | — |
| D236 | All runs by default (orchestrator; Sarath away) | `ListRuns` with limit 0 returns every stored run | — |
| D237 | Imports are pasted text only (orchestrator; Sarath away) | `Import` reads only the text the request carries, never a URL or a path; its source is named `pasted`, so relative references resolve from the project root. The snapshot's name follows D230 | — |

Round 37 additions (10 November 2026, Sarath's answers to the questions parked during M8 and M9):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D238 | Two more duplicate crates (Sarath) | deny.toml's `[bans]` skip list adds `compact_str@0.9` (ratatui-core 0.1, `vc tui`) and `itertools@0.14` (ratatui 0.30 and prost-derive 0.14 cap it there; oxc is on 0.15), each with its reason. `multiple-versions = "deny"` stays | — |
| D239 | Two tokens for `vc ui` (Sarath; a security fix) | Each `vc ui` process makes a launch token and a separate agent token, both 256-bit CSPRNG values compared in constant time. The agent token is accepted only on `/mcp`, as `Authorization: Bearer`; `/api` accepts only the browser's session cookie, so a Bearer token on `/api` is 401 and an agent cannot reach `DecideApproval`, `WriteFile`, `Import`, `Run` or any other `EngineService` call. The agent token is shown in the web UI's Connect panel and printed in a second stdout line, `vc ui: mcp http://127.0.0.1:<port>/mcp token …`; like the launch token it appears in no log, error, report or history | D222 (Bearer on `/api`), D225 (the launch token on `/mcp`) |
| D240 | The session cookie (Sarath) | The launch link `GET /?token=…` works once: it is exchanged for a fresh 256-bit session token in the cookie `vc_token_<port>` (`HttpOnly; SameSite=Strict; Path=/`), so two `vc ui` processes on different ports keep separate sessions, and the launch token is dead after that. Opening a used launch link is 401 with a hint to use the open tab or restart `vc ui`. The Vite dev proxy makes the exchange itself with `VC_UI_TOKEN` and forwards the cookie | D222 (the `vc_token` cookie holding the reusable launch token) |
| D241 | Who a `vc ui` request is (Sarath) | A request on `/api`, which carries the session cookie, is the person at the browser: it skips the profiles' agent policy and is not written to the agent log. A request on `/mcp`, which carries the agent token, always goes through the agent policy and the agent log, as `vc mcp` does (D178, D195, D197) | D233 |
| D242 | Earlier defaults confirmed (Sarath) | D227 (the editor's save guard: unguarded saves, digest check) and D232 (`vc config explain` masks secrets, and masks everything but the built-in values when the secrets cannot be loaded) stay as written | — |
| D243 | Connect stays binary-only (Sarath) | D221 holds: no JSON bodies, and pbjson stays parked, since it would bring duplicate `base64` and `itertools` versions | — |
| D244 | Plugin permissions are declared, not enforced (Sarath) | A plugin's permissions are declared in its manifest and shown at install. The OS does not enforce them in v2.0; Landlock or Seatbelt sandboxing is parked, because `unsafe_code = "forbid"` needs a `pre_exec` or re-exec launcher and Landlock cannot filter hosts | — |
| D245 | The batch after M9 (Sarath) | The next batch takes M10 plus three parked groups: the TUI follow-ups with inline editing in Explore (A5), the notifier extras (an email notifier, `vc alerts test`, the `blocked` alert trigger), and the M9b mockup extras (ad-hoc editing in Client with save to the api, the timing waterfall, more Insights, an editable Settings form). A dark theme and the mockups' fonts stay parked | — |

Round 38 additions (10 November 2026, gate fixes; approved by Sarath for the gate-fix PR, none changes an earlier row):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D246 | More gates: JS licences, the wire contract, wider mutation, narrower test files | `GATE-JSLICENSE` runs `pnpm licenses list --prod --json --recursive` and fails on a production JS dependency whose licence is outside `deny.toml`'s `[licenses] allow` list, read from deny.toml so Rust and JS share one allow-list; SPDX `OR`, `AND` and `WITH` are read as cargo-deny reads them (`scripts/js-licenses.mjs`). `GATE-PROTO` runs `buf lint` on `proto/` and `buf breaking` against `proto/` at `GATES_PROTO_BASE` (default `origin/master`; its own variable, because `GATES_BASE` is the commit of the PR's previous run, D205), taken with `git archive` since buf's `.git#ref=` input cannot read a linked worktree; the breaking check is skipped with a notice when the base has no `proto/`. The diff mutation run also covers `vc-server`, `vc-contracts` (excluding its generated `src/gen/`) and every `vc-adapter-*`; the nightly full run keeps the four floor crates. commit-check's test files are `*_test(s).rs`, `tests.rs`, `*.test.*`/`*.spec.*` JS or TS, `e2e/`, and a `tests/` folder outside `src/` (`web/src/lib/tests` is the Tests page's components). Each new gate has a bad-patch fixture (D121). | — |

Round 39 additions (10 November 2026, Sarath's answers to the batch-4 questions):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D247 | Settings edits `.env.schema` (Sarath) | The web UI's Settings form (and `ReadFile`/`WriteFile`) may read and write `.env.schema`, which declares variable names, types and sources, never values. `.env` itself stays out of reach of the editor and every agent tool | D227 (in part: `.env.schema` was out of the editor's reach) |
| D248 | Allowed failures (Sarath: build it, but not under the name "quarantine") | A test or test file may declare `allowFailure: true`, or `allowFailure: { reason, until }` with the same shape as `skip`. Such a test still runs and is reported; when it fails, its state is `allowedFailure` (shown as ⚠ "allowed to fail"), counted separately in the summary, CTRF and JUnit (as a skipped-with-message case), and it does not change the exit code. After `until`, it counts as a normal failure and V4xxx warns that the date has passed. `--strict` treats every allowed failure as a failure | — |
| D249 | Recently changed tests (Sarath) | The `Vcs` port gains "files changed in the last N days" (git log); `vc list --changed-since <duration>`, the TUI and the web UI's Tests filter "changed recently" use it | — |
| D250 | Live request phases (Sarath) | The live feed carries each in-flight request's phases (dns, connect, tls, send, wait, receive) as they finish, so the run views can show a live waterfall for a running request | — |
| D251 | The Client flow editor (Sarath) | In scope: the web UI's Client edits multi-step flows (stages, `repeat`, `forEach`, `poll`) and saves them through the guarded `EditFile` | — |
| D252 | Email notifier (Sarath: build it) | `email` is a notifier registry entry built on `lettre` with rustls (no OpenSSL). Config: `host`, `port` (default 587), `starttls` (`required` by default, `none` only for localhost), `username`, `password` (a secret reference, never a literal), `from`, `to` (list). The message is plain text plus HTML, and holds no request bodies or secrets | — |
| D253 | The team server stores history in SQLite (Sarath) | `vc serve` (M10) keeps history in SQLite (WAL, the same `HistoryStore` and migrations as local history, with one writer task) instead of Postgres. Postgres stays possible later behind the same trait, if a team outgrows one node | lld.md M10 (Postgres history) |
| D254 | Unparked work (Sarath) | After the M10 batch: Bruno `.bru` collection import (an importer registry entry), JSON Connect bodies through pbjson (with the needed `[bans]` skips), and OS-enforced plugin permissions (Landlock on Linux, Seatbelt on macOS) | D243, D244; D209 and D212 (in part: a Bruno folder is now imported, not refused with exit 2) |
| D255 | User documentation site (Sarath) | An Astro Starlight site in `site/` (a private pnpm package), deployed to GitHub Pages at `/vc-rs/` by `.github/workflows/pages.yml`. It has a page per command, keyword, matcher, pipe and diagnostic; Pagefind search on Cmd/Ctrl+K; a YAML playground that validates against the exported JSON Schemas; an agents section with MCP setup and a `skills/vibranium` agent skill; and llms.txt. Writing it is also a test round of the tool: each documented feature is run, and mismatches are filed as issues | — |

Round 40 additions (10 November 2026, defaults for the items D216 left open and D245 unparked; Sarath may override):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D256 | The `blocked` alert trigger | `on: [blocked]` fires when one of the rule's tests (after its tags filter) ended blocked. `failure` still means "failed or blocked" and the default `on: [failure]` is unchanged. Precedence: failure > blocked > threshold > flaky > recovery, so a rule listing both sends the failure notice. Its notice is titled "tests blocked" and lists each blocked test with the action that blocked it | — |
| D257 | `vc alerts test` | `vc alerts test [--profile p]...` sends one sample notice (`Notice.test`, title prefixed "[test]", a fixed 1-passed/1-failed summary) through every channel of every alert rule whose `profiles` match, ignoring tags and `on`. Secrets resolve as in a run; nothing runs and nothing is recorded. PagerDuty gets severity info, followed at once by a resolve with the same dedup key. It prints one line per channel; errors never show a URL or key. Exit 0 when all channels sent, 1 when any failed, 2 when no rule matches. `Engine::test_alerts` backs a later Settings button | — |
| D258 | JSON Schemas hosted with the docs (Sarath) | The four schemas (`config`, `api`, `test`, `load`) are committed at the repo root in `schema/`, which is the single source, and the docs site publishes them at `https://v007m.github.io/vc-rs/schema/<kind>.schema.json` next to an index page. That Pages URL becomes each schema's `$id` and the `yaml-language-server` modeline `vc init`/`vc new` write. The raw.githubusercontent URL keeps working. A test fails when `schema/` differs from `vc schema export` | HANDOVER "Docs and JSON Schema hosting" (raw URLs) |

Round 41 (11 November 2026; Sarath asked the orchestrator to decide the parked questions: "decide on the right, sensible, secure answers ... without breaking functionality"):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D259 | JS licences of the shipped web UI | `svelte` and `@sveltejs/kit` move to `web/`'s `dependencies`, so GATE-JSLICENSE checks the runtime that is bundled into the binary. MPL-2.0 joins the `deny.toml` licence allow list. MPL-2.0 is file-level copyleft and OSI-approved; we ship its files unmodified, and their source stays public. Today it reaches us only through `lightningcss`, the build-time CSS minifier vite brings in. The gate's logic does not change | — |
| D260 | Plugins that cannot be confined | On macOS, when `sandbox-exec` cannot start (for example when `vc` itself runs sandboxed), a plugin fails to start with a V5 diagnostic that says why and how to opt out. The opt-out is `plugins.allowUnconfined: true` in the user config (`~/.config/vibranium`) or `VIB_PLUGINS_ALLOW_UNCONFINED=1`. It is never read from `vibranium.config.yaml` or a profile, so a cloned project cannot switch confinement off. On Linux, Landlock stays best effort, so older kernels keep working. A plugin that runs unconfined (no Landlock, or the opt-out) prints one warning per run naming the plugin, and the run's report records it | — |
| D261 | Team server (M10) defaults | Auth: OIDC authorization code with PKCE through the `openidconnect` crate (rustls), with discovery and ID-token checks. An account is keyed by (issuer, subject) and has the role `viewer` or `admin`. Sign-in is closed until `serve.auth.allow` names email domains or groups; the first admin comes from `vc serve admin add <email>` on the host. Sessions are HttpOnly, Secure, SameSite=Lax cookies. Ingestion: per-project tokens of 256 random bits, shown once, stored as a hash, revocable and with an optional expiry, sent as `Authorization: Bearer`. GitHub Actions OIDC tokens are accepted as an option, matched on the `repository` and `ref` claims, so no stored secret is needed. Ingested reports have a size cap and are redacted again on the server. `vc serve` binds to loopback by default. It refuses a non-loopback bind without TLS, unless `--behind-proxy` names the trusted proxy | lld.md M10 open items |

Round 42 (11 November 2026; gaps found by the docs test round and by the request-body work. The orchestrator proposed each answer, and Sarath approved them all):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D262 | `vc new test` for an api with path vars | The created test sets each path var: a sample value from the spec when there is one, otherwise a typed placeholder, with a `# replace` comment. The new file passes `vc check`, as every creator's output must | cli-workflow `vc new test` (path vars left unset) |
| D263 | `.env` in the `.gitignore` that `vc init` writes | `vc init` lists `.env` next to `.vibranium/` in the `.gitignore` it writes or extends, so secrets are not committed by accident | — |
| D264 | `vc run --json` | `vc run --json` is an alias for `--reporter json`, and both are kept. This makes "every command takes `--json`" true | — |
| D265 | `--set` precedence | `--set` wins over every config layer of D129, `.env` and `VIB_*` included. It never replaces the value of a secret. The help text says this | D129 (clarified) |
| D266 | `run --out` into a folder that is not empty | vc overwrites only the files it writes and deletes nothing else. When the folder already held other files, it prints one note | — |
| D267 | `console.log` in a hook | It goes to the run log, tagged with the hook's name, and shows with `-v` | syntax-reference hooks (output unspecified) |
| D268 | Path vars are percent-encoded | Each path var value is encoded as one path segment (RFC 3986), so `a/b` is sent as `a%2Fb` and a value cannot change the path's shape | — |
| D269 | Loops on `command:` actions | `repeat`/`forEach` are allowed on `command:` actions, as the syntax reference says, and the check that rejected them is fixed | — |
| D270 | A reused test's own `expect` | A `test:` reuse runs the reused test as written, its own test-level `expect` included. The D124 step defaults apply to the reusing test's own steps only | D124 (clarified) |
| D271 | Case of diagnostic codes | `vc explain` and `--allow`-style flags accept codes in any case (`v2101` = `V2101`). Output always prints them upper case | D180 (clarified) |
| D272 | `$file` request bodies | `$file` paths must be literal. They resolve against the folder of the file that names them, are checked by V2004, and their bytes are read by the engine before the run (like data files), so the executor does no I/O. Binary files are supported. A templated `$file` path fails with V5111. The content type comes from a small built-in extension table (json, csv, txt, xml, html, yaml, png, jpg, gif, pdf, zip), else `application/octet-stream`. There is no new dependency | — |
| D273 | Form-urlencoded bodies | When the content type's media type (parameters and case ignored) is `application/x-www-form-urlencoded`, a map body is WHATWG form-encoded (space as `+`). Scalars are rendered as text, a list repeats the key, and a nested object is V5111. The header is sent as written | — |
| D274 | Multipart bodies | When the media type is `multipart/form-data`, a map body becomes parts. The boundary is `vc-` plus 24 hex characters from the run's seeded `Random`, unless the user's header names one; the header is rewritten to carry it. Text parts are UTF-8 with no part content type (RFC 7578). A `$file` part has the basename as filename and the D272 content type. A list repeats the part; a nested object becomes an `application/json` part. Built in the executor with no new dependency | — |

Round 43 (11 November 2026; gaps found by the WP4 triage and review. Orchestrator defaults for Sarath to confirm):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D275 | A timing phase that did not happen | `dns`, `connect` and `tls` read 0 ms in timing checks and in `${response.timing.*}` when the phase did not happen: a reused connection (no `dns` or `connect`), an IP literal (no `dns`), plain HTTP (no `tls`). So `connect: {lt: 5ms}` passes on a reused connection and `connect: {gt: 0ms}` fails, as in curl. The exchange, reports and history keep the phase as not measured (`None`, `NULL`) | — |
| D276 | A literal object that looks like a misspelt matcher | A map in a literal position (`expect.body`, a header, a `[*]` path) with no matcher's name as a key is a literal object, as before. When its one key has five characters or more and is close to a matcher's name, case counted (`startWith`, `lenght`, `Length`, `exist`, `types`), it still compiles as that literal but warns V3208, a new lint code: "fix the name to use the matcher, or write `eq:` to keep it as a literal". It is never an error. A status map (`expect.status`, `saveOn`) is never a literal, so an unknown key there stays V1004. Orchestrator default; Sarath to confirm | — |

Round 44 (11 November 2026; gaps found by the WP7 import and sync triage. Orchestrator defaults for Sarath to confirm):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D277 | `source.only` | `vc import openapi --only <operationId>` is kept in the api file as `source.only`, beside `source.include` (D79, D177). `vc sync` drafts the spec with both, so an operation the spec adds outside them is not added to the file; an operation must match both when both are set. Removing the keys takes the whole spec again | D79, D177 (extended) |

Round 45 (11 November 2026; orchestrator; Sarath away):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D278 | How a `path` is encoded | A `path`'s literal text is sent as written. Each `${…}` value is percent-encoded as one segment (RFC 3986 `pchar`: `/`, `?`, `#`, `%` and a space are encoded), and a lone `.` or `..` is sent as `%2E` or `%2E%2E`, so a value cannot change the path's shape. A value before the path's first `/` is sent as written only when it renders to an absolute `http://` or `https://` URL (`${var.usersUrl}/users`); any other leading value is one segment like the rest, so `${var.id}/orders` with `id: ../admin` sends `..%2Fadmin/orders`. A value after a literal `?` is encoded as a query value. A value whose last pipe is `urlEncode` is sent as the pipe left it, not encoded twice, and `vc check` warns with V4007, whose certain fix removes the pipe. The `baseUrl` and a flow auth's inline URL are sent as written. `vc explain` shows a path's values before they are encoded | D268 (clarified) |

Round 46 (12 November 2026; Sarath approved the orchestrator's recommended answers to the parked questions):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D279 | A failing teardown fails the run | A failed check or action in `afterAll` or `afterRun` makes the run exit 1, and the summary adds a "teardown failed" line naming the block. The tests' own verdicts don't change | D127 (adds a case to exit 1) |
| D280 | Only the secrets a selection reads are required | The V5114 secrets check before a run covers only the secrets that the selected tests, their apis, auth and lifecycle blocks read. `# @optional` in `.env.schema` marks a secret that may be unset (Varlock's convention), and `vc check` doesn't require it. The schema still never holds values (D247) | S1 (narrows it) |
| D281 | No example rows means skipped | A test whose `examples` give no rows is planned as skipped, with the warning "examples gave no rows". A run whose only tests were skipped this way exits 0 | — |
| D282 | `test:` saves count as declared | The compiler records the saves of a `test:` action that it resolves as implicit `provides`, so V5108 fires only for `function:` saves and for `test:` saves it can't see | D128, D186 (amends) |
| D283 | A flaky load iteration is an error | A load iteration that passes only on a retry counts as an error, blamed on the call whose last attempt failed | — |
| D284 | HAR import keeps dropping third-party hosts | `vc import har` keeps D206: an entry whose host matches no profile base URL is dropped, the message names each dropped host, and `--include-host <host>` keeps one | — (confirms D206) |
| D285 | `vc-demo` stays a second binary | Installers ship `vc` and `vc-demo` side by side. A missing `vc-demo` exits 2 with an install hint | — (confirms D188) |
| D286 | `vc init` ignores `.env` | The `.gitignore` that `vc init` writes lists `.env` as well as `.vibranium/` | — |
| D287 | CLI help wording | Help and docs say that `vc run` takes `--reporter json` (not `--json`), and that `--set` overrides every config layer (D129), not every input | — |
| D288 | Seeds stay JSON numbers | A seed above 2^53 may lose precision in JSON output. It stays a number. This is an accepted limitation, and the console prints it exactly | — |
| D289 | Hooks have no console | `console.log` in a hook stays V5115. A run log is left for later (D267) | — |
| D290 | The gate self-test has its own target dir | `scripts/gates-selftest.sh` builds in its own `CARGO_TARGET_DIR`, so path-baked test binaries can't break the next `mise run gates` | — |
| D291 | Generated test IDs | An awkward API name such as `legaciesSearch` is left as `vc_model::third_person` produces it. There is no verb dictionary | — (confirms D208) |
| D292 | No email alerts | The `email` notifier is dropped from the MVP, and `deny.toml` is unchanged | D252 |
| D293 | The playground runs tests in the browser (Sarath: in the MVP) | The docs playground gets a Run button. It compiles the example project in the browser with the real compiler, so it shows all `vc check` diagnostics across files, and it plans and runs the tests with the real executor against the demo service, also in the browser. It prints what `vc run` prints. The engine is compiled to WebAssembly. A browser adapter supplies the transport (it calls the demo service in process), the clock, randomness and the sources. Sarath approved the needed arch-tests change (a new crate and its layer edges). Nothing leaves the browser | — |

D276 (literal matcher keys), D277 (`source.only`) and D278 (path encoding) are confirmed as written.

Round 47 (12 November 2026; orchestrator, under Sarath's delegation):

| # | Topic | Decision | Supersedes |
| --- | --- | --- | --- |
| D294 | Allowed failures in the pass rate | History's pass rate counts an allowed failure as a test that ran and did not pass. A run made up only of allowed failures shows 0% | — (fills a gap in D248) |
| D295 | Testing an auth from Settings | `TestAuth` obtains an auth's credentials under a profile through the run's token cache, as a call would, without sending the call. It returns only the api it obtained them for, the header, query and cookie names and how long they hold; the client chooses the profile and the response does not echo it. The token is never returned, logged or kept, and every failure comes back masked by the run's redactor (V5114 for the credentials) | — |
| D296 | Which api an auth test may name | The senders of an auth are the apis whose own call uses it and the tests, steps and actions whose own `auth` is it, each as a call of its api in that call's scope. By default `TestAuth` goes through the first sender by api ID. A named api that does not use the auth only picks the sender: the first whose resolved origin equals the api's, and the auth renders in that sender's scope, never the named api's, so a test never takes credentials (or a token request's client secret) to a host a run would not send them to | — (narrows D295) |
| D297 | `vc demo start` takes turns through a file lock | Concurrent `vc demo start`s on one port take turns through an OS file lock (`File::try_lock`) on `<port>.lock` in the private `vibranium-demo-<uid>` folder. The lock file is checked as the pidfile's folder is, on the opened handle: owned by the user, not a link (its device and inode match the path's, read without following a link). D113's allow-list is for in-process locks and shared mutable memory (`Mutex`, `RwLock`, `arc-swap`); a lock on a file that coordinates separate `vc` processes is I/O in the CLI layer, so it is outside that list and ARCH-LOCK and the arch-tests are unchanged | — (clarifies D113) |
| D302 | A blank secret is unset, and code reads every secret | (a) A secret whose value is blank (empty or only whitespace) is unset everywhere: the provider's load leaves it out, so the pre-run check refuses a required one, and a `${secret.X}` of an optional one fails its action with V5114 at run time. `# @optional` only lifts the pre-run refusal. (b) When the selection runs code (a `function:` action, a `beforeRequest` or `afterResponse` hook, a `function` check, a `redact` `function` rule) or the project loads a plugin, the pre-run check treats every declared secret as read, since code can call `secret()` with any name. A pure-YAML selection keeps D280's narrowing. The check follows the vars of the active profiles only; the api cleanups the cache's sweep can run are read under every profile, because each entry runs under the profiles it was made with | D280 (narrows and clarifies) |
| D303 | Strict `@optional` | A name is optional only when a comment line whose whole trimmed text is `# @optional` is in the comment block directly above it (a blank line or any other line ends the block), or when the comment after its value is only `# @optional`. A `#` inside a quoted value is part of the value. Prose that mentions `@optional`, `# @optional=true` and `# @optional @sensitive` mark nothing | D280 (clarifies) |
| D304 | Teardown failures are machine-readable | The json reporter's `summary` line and the HTML report's summary carry `teardownFailures: [{block, file, message}]` (`file` is null for `afterRun`), CTRF carries the same list under `results.extra.teardownFailures`, and JUnit adds one `<system-err>` per failed block to `<testsuites>`. The HTML report and an alert that fires show the console's `teardown failed: …` line (`teardownFailed` in a webhook's or PagerDuty's details). Messages are masked, and no count changes. An `afterAll` or `afterRun` action whose worker stops before it reports (a panic or an abort) is a teardown failure unless the run was cancelled, which stays exit 130 | D279 (adds) |
| D305 | Test durations | A test's duration is measured by the executor, around the instance, and carried on its result and event; reporters and the store use it and never time by when they read an event | — |

## Open

All decisions needed for the PRD and technical design are settled. The language is decided by the spike (D6, D36, D37, D41).

- The mockup seed project (`design/mockups/generators/editor-src/seed/`) and the generated canvases predate D138–D140 (dotted full IDs, `id:`/path mismatches). Update them at the M9 kick-off, before they are used as web UI fixtures.

The items carried to milestone kick-offs in Round 15 are settled: the default secrets provider by D163, notifiers by D164, and `vc demo` by D175 and D188.

## Parking lot

- Mocks generated from `*.api.yaml` and scheduled monitoring: after the web UI (D38).
- GraphQL, gRPC and WebSocket/SSE executors: after v2.0 (D4).
- UI testing through HTML/XML parsing: later (D4).
- Distributed load generation beyond one machine: not in v2 (D8).
- OpenTelemetry spans, OpenAPI-driven fuzzing, traffic-recording to tests: v2.3+.
