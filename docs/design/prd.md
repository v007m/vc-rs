# Vibranium v2 — Product Requirements

Oct 22, 2026 · @Sarath

Vibranium v2 is an open-source, git-native API testing platform: one engine for functional, contract and load tests, driven from a CLI, TUI, web UI, code SDKs and AI agents. Decisions referenced as D1–D45 come from the Vibranium v2 Decision Log.

## Problem

Vibranium v1 proved that declarative, defaults-heavy API tests are pleasant to write, but its model does not scale past small suites.

- **Dependencies are slow.** A dependency re-runs as a full test: scripts, request and every assertion, once per repeat of the parent. A chain of four tests costs 10 executions instead of 4.
- **Caching is unsafe.** The opt-in cache ignores input values, so a parameterised dependency can return the wrong response.
- **Definitions are untyped.** Test JSON is validated loosely and assertions are JavaScript strings, so typos surface at run time.
- **Config is global.** A home-folder file points at one workspace, which breaks CI and multi-project use.
- **The stack is ageing.** It relies on the deprecated `request` library, the discontinued `vm2` sandbox and the unmaintained NeDB store.

The market has a gap. Hurl and Bruno grew because tests are plain files in git with no account. Neither offers cross-file reuse, setup data with automatic cleanup, compile-time integrity checks or a queryable run history. Postman has all the features but is cloud-first and paid for teams, and load testing lives in a separate tool such as k6.

## Goals and non-goals

The goal is for Vibranium to be the default open-source tool for API tests that live in git and run anywhere.

Goals:

1. One engine for functional, contract and load tests, shared by every interface (CLI, MCP, TUI, web UI, SDKs).
2. Setup calls apis directly, runs once per lifecycle block and cleans up what it creates; it never re-runs assertions it does not own.
3. Every reference is checked at compile time, with errors that name the file, line and column.
4. Tests, apis and config are plain YAML or JSON in the project repo, with no account required.
5. Every layer sits behind an interface, so transports, auth, secrets, storage, reporters and notifiers are swappable (D23).
6. AI agents are first-class users, with guardrails per environment (D29, D32).
7. Everything is open source under Apache-2.0, including the self-hosted server (D7, D35).

Non-goals for v2:

- Distributed load generation across machines; one machine up to about 5,000 virtual users is the target (D8).
- A hosted cloud service or paid tier.
- Running v1 JSON files unmodified; a one-shot migrator converts them instead (D34).
- A proprietary test DSL; YAML, JSON and code SDKs cover authoring.

## Users and jobs

Vibranium serves four kinds of user equally (D3); each job below must work without the others.

| User | Where tests live | Main jobs | Primary interface |
| --- | --- | --- | --- |
| Service developer | Next to the service code | Check own endpoints in PR CI; catch contract drift from OpenAPI | CLI, TypeScript or Go SDK |
| QA engineer | A standalone test repo covering many services | Regression suites across environments; data-driven cases; dashboards | CLI, TUI, web UI |
| Performance engineer | Same repo as functional tests | Reuse functional tests as load scenarios with thresholds | CLI with `*.load.yaml` |
| AI agent | Either | Generate tests from specs, run them, explain failures, call apis ad hoc | MCP server and skill |

A team lead or platform owner also uses the self-hosted server for shared history, alerts and team dashboards.

## Concepts and file model

Users write two things: an **api** says how to call an endpoint, what must always hold and how to undo what it creates, and a **test** calls apis and states what it expects. **Lifecycle blocks** (`beforeRun`, `beforeAll`, `beforeEach`, `before` and their `after` partners) prepare data by calling apis, JUnit style; where a block sits decides how long its data lives, and each api's own cleanup removes the data when that block ends, so shared setup never re-runs assertions (D67–D71). Every value is a **var**, and **profiles**, at least one of which is always active, switch values per environment without duplicating logic (D61, D65, D73). The full syntax is in the Vibranium v2 Syntax Reference.

| File | Holds | Never holds |
| --- | --- | --- |
| `vibranium.config.yaml` | Vars, profiles, auth definitions (including multi-step `flow` auth), secrets, generators, datasets, reports, history, alerts, plugins, run-level lifecycle blocks | Tests or apis |
| `*.api.yaml` | Method, path, vars (defaults), body, `expect` that always holds, `save`, cleanup, request hooks | Test-specific checks |
| `*.test.yaml` | Tests (`call` or `steps`, `vars`, `expect`, `save`, `examples`, `repeat`) and lifecycle blocks (`beforeAll`, `afterAll`, `beforeEach`, `afterEach`, `before`, `after`); per-file `profiles` | Request details owned by an api |
| `*.load.yaml` | A weighted mix of tests, user stages, thresholds | New request or check logic |

```yaml
# services/users/crud.test.yaml
id: users.crud
name: User accounts

beforeAll:
  - id: admin
    call: users#create
    vars:
      role: admin

tests:
  - id: createsMember
    name: Creates a member account
    call: users#create

  - id: getsCreatedUser
    name: Returns the user that was just created
    before:
      - call: users#create
    repeat: 5
    call: users#get
    expect:
      status: 200
      body:
        match:
          id: ${var.userId}
```

Here the admin account is created once for the whole file and read as `${var.admin.userId}`. Each of the five `getsCreatedUser` instances creates its own user in `before`; `users#create` saves its ID as `userId` and declares `users#delete` as its cleanup, so every user is deleted when the block that made it ends, and no test mentions deletion. If a lifecycle action fails, it retries, then every test that needs it is reported as **blocked** with the exact cause: which action, which request, which check, and the response that broke it (D11).

## Requirements

Requirements are grouped by area; the release column follows the plan below (v2.0 core, v2.1 agents and load, v2.2 TUI and plugins, v2.3 server and web UI, v2.4+ protocols and platform).

| ID | Area | Requirement | Release |
| --- | --- | --- | --- |
| R1 | Config | Discover `vibranium.``config.yaml (or .json)` by walking up from the working directory; minimal user-global config for preferences only | v2.0 |
| R2 | Config | Layering: defaults, user-global, project, active profiles, `.env`, `VIB_*` variables, CLI flags; `config explain <key>` shows the winning layer | v2.0 |
| R3 | Secrets | `${secret.*}` references resolved through a Varlock-based provider layer; every resolved value is redacted from logs, reports, history and agent output | v2.0 |
| R4 | Compile | Parse YAML and JSON with source positions; validate against typed schemas; publish a JSON Schema for editor autocomplete | v2.0 |
| R5 | Compile | Link by ID: uniqueness, missing references with "did you mean" suggestions, cycle detection that prints the cycle, a warning when a `PUT`, `PATCH` or `DELETE` uses a shared value from `beforeRun` or `beforeAll`, and an error when no profile is active | v2.0 |
| R6 | Compile | Incremental compile from a file-hash cache, so unchanged files are not re-parsed | v2.0 |
| R7 | Execute | Plan a DAG for the selected tests; run with concurrency limits per run and per host | v2.0 |
| R8 | Execute | Lifecycle blocks (`beforeRun`, `beforeAll`, `beforeEach`, `before` and their `after` partners) run actions once per block; value-producing `beforeRun` actions run lazily with singleflight; api-declared cleanups run newest first when the owning block ends, also on Ctrl-C; `repeat` and `forEach` loops on actions, sequential unless `parallel` is set, collect saved values into lists | v2.0 |
| R9 | Execute | Lifecycle failure: configurable retries, then dependents reported as blocked with the full cause; `after`, `afterEach` and cleanups still run | v2.0 |
| R10 | Execute | `repeat`, `examples rows, ordered steps, polling`, timeouts, retries for flaky checks marked as flaky | v2.0 |
| R11 | Assert | Status, headers, JSONPath matchers (`some`, `every`, length, comparisons, regex), JSON Schema, CEL expressions, per-phase timing (DNS, connect, TLS, first byte, total) | v2.0 |
| R12 | Assert | Hooks at run, file, test and api level, and custom check functions; TypeScript in-process, other languages through the plugin protocol | v2.1 |
| R13 | Auth | Basic, bearer, API key, OAuth 2.0 client credentials, and multi-step flow auth; tokens shared for the run (optionally cached across runs), refreshed before expiry and on 401 | v2.0 |
| R14 | Auth | OAuth 2.0 authorization code with PKCE, refresh tokens, custom auth plugins | v2.2 |
| R15 | Import | OpenAPI 3.x (YAML/JSON) to `*.api.yaml`, then `sync` with a three-way merge and conflict report | v2.0 |
| R16 | Import | v1 migrator; Postman, Bruno/OpenCollection and `.http` importers | v2.0 (v1), v2.2 (others) |
| R17 | Results | CTRF JSON as the canonical result; JUnit XML and console spec output derived from it | v2.0 |
| R18 | Results | HTML dashboard with pluggable widgets and default templates | v2.1 |
| R19 | History | Every run stored in SQLite (WAL, single batched writer) behind a storage interface; bodies stored per policy, redacted | v2.0 |
| R20 | History | Queries: flaky tests, latency trends, first failing run, compare two runs | v2.0 (basic), v2.1 (trends) |
| R21 | Alerts | Webhook and Slack notifiers on failure, recovery, flaky and threshold breach | v2.0 |
| R22 | Alerts | Microsoft Teams, PagerDuty and email notifiers | v2.2 |
| R23 | Load | `*.load.yaml` with virtual-user stages, think time, thresholds; live metrics; latency histograms stored per interval | v2.1 |
| R24 | Agents | MCP server: list, search, read history, run, explain failures, create and edit tests, call apis ad hoc | v2.1 |
| R25 | Agents | Guardrail policy per environment: allowed methods and writes; human approval for writes where configured | v2.1 |
| R26 | Agents | A published agent skill that teaches the file formats and workflows | v2.1 |
| R27 | SDKs | Protobuf-generated types for TypeScript, Python, Go and Java/Kotlin | v2.1 |
| R28 | SDKs | Builder APIs and test-framework helpers in order: TypeScript (v2.1), Python (v2.2), Go (v2.3), Java/Kotlin (v2.4) | v2.1–v2.4 |
| R29 | Plugins | Third-party plugins in any language over JSON-RPC, REST or webhooks with ready-made clients; gRPC for hot-path executors; permissions manifest approved on install | v2.2 |
| R30 | TUI | Browse tests and history, run selections, view failures, edit tests with write-back to YAML | v2.2 |
| R31 | Server | Self-hosted server: shared history (Postgres), team dashboards, run ingestion from CLI and CI | v2.3 |
| R32 | Web UI | Browse, run and edit tests (writes YAML), dashboards, history | v2.3 |
| R33 | Protocols | GraphQL executor | v2.3 |
| R34 | Protocols | gRPC and WebSocket/SSE executors | v2.4+ |
| R35 | Platform | Mocks generated from `*.api.yaml`; scheduled monitoring runs on the server | v2.4+ |
| R36 | Platform | UI testing through HTML/XML parsing | Later |
| R37 | Distribution | Single binaries for macOS, Linux and Windows (x64, arm64); scoped npm packages; Homebrew tap; Docker image; GitHub Action | v2.0 |
| R38 | Execute | Cross-run cache: cache.for and cache.check on a lifecycle action; stored per profile in .vibranium/cache.db, keyed by api or test, vars and profile; cleanup on expiry or vc cache clear; secret-derived values encrypted with an OS keychain key | v2.0 |
| R39 | Templates | Payload reuse with $extend and $remove; type-conversion pipes (number, int, bool, string, json, jsonString, base64, fromBase64, lower, upper, trim, urlEncode, default); computed values with $cel | v2.0 |
| R40 | Config | At least one profile; vc init creates a default profile; each api routes to its own file's base URL and auth, so one test can span several services | v2.0 |
| R41 | Import | HAR import: filter noise, map hosts to api files, derive conventional IDs and path vars, merge duplicates, match existing apis, strip secrets into auth and ${secret} references, optionally write a flow test replaying the session; --dry-run with diffs. CLI first, then TUI and web UI | v2.0 (CLI), v2.2 (TUI), v2.3 (web) |
| R42 | Compile | Diagnostics with code, location, underlined source, explanation, help and certain fixes; vc check --fix with diff preview; vc explain \<code>; naming-convention lint with rename across references (vc rename) | v2.0 |
| R43 | Web UI | API client like Postman: browse every api, build and send requests per profile, inspect body, headers, timing and checks, chain multi-stage flows with values passed forward, history, save as test or flow test, import HAR, OpenAPI, cURL and Postman | v2.3 |
| R44 | Web UI | Editor for every file kind (config, api, test, load, dataset, schema, hook) in form, YAML or split view over one source of truth; autocomplete of IDs, vars, matchers; live diagnostics with quick fixes; diff against the saved file; save blocked on errors; creators that apply the naming conventions | v2.3 |

## Interfaces

Every interface is a thin adapter over the same engine, so a capability added to the engine appears everywhere; they ship in the order CLI, MCP, TUI, web UI (D30).

### CLI

The command is `vibranium`, aliased `vc` (D44). Every command accepts `--json` for machine-readable output, `--profile` to pick profiles and `--set key=value` to override config. The full command set, run selectors, OpenAPI sync, generated tests and skipping are specified in the Vibranium v2 CLI and Developer Workflow doc (D77–D82); the table below is the summary.

| Command | Does |
| --- | --- |
| `vc init` | Create `vibranium.`config.yaml and a sample api and test |
| `vc check` | Compile and link everything; print diagnostics; non-zero exit on errors |
| `vc list [apis\|tests] [--tag] [--grep]` | List and search definitions |
| `vc run [selector...]` | Run tests by ID, glob, tag, file or `--changed` (git-aware); `--repeat`, `--bail`, `--reporter` |
| `vc plan [selector...] [--dot]` | Show the execution graph without running it |
| `vc call <api-id> [--with k=v]` | Call one api ad hoc with project auth and environment |
| `vc new [api\|test]` | Scaffold from templates, interactively or by flags |
| `vc import openapi\|postman\|bruno\|http\|v1 <path>` | Import definitions |
| `vc sync [group]` | Re-sync api groups from their OpenAPI source |
| `vc load <scenario>` | Run a load scenario with live metrics |
| `vc history [query]` | Query past runs: failures, flaky tests, trends, compare |
| `vc report <run> --format html\|junit\|ctrf` | Produce a report from a stored run |
| `vc config explain <key>` | Show the effective value and the layer it came from |
| `vc plugin add\|list\|remove` | Manage plugins, showing the permissions each requests |
| `vc mcp` | Start the MCP server on stdio |
| `vc tui` | Open the terminal UI |
| `vc serve` | Start the self-hosted team server (later release); the local web UI for one project is vc ui |

### MCP tools for agents

| Tool | Purpose | Guarded by policy |
| --- | --- | --- |
| `list_definitions`, `search` | Find apis and tests | No |
| `get_definition` | Read one definition with its resolved references | No |
| `check` | Compile and return diagnostics | No |
| `run_tests` | Run a selection; return CTRF summary | Environment allow-list |
| `explain_failure` | Structured cause: request, response, failed checks, diff | No |
| `query_history` | Flaky tests, trends, last failures | No |
| `write_definition` | Create or edit YAML; compiled before saving | Write policy |
| `call_api` | Ad hoc request | Method and environment policy; approval for prod writes |

### TUI and web UI

Both browse tests and history, run selections, show failure detail and edit definitions, writing changes back to the YAML files (D31). The web UI adds team dashboards and shared history through the self-hosted server.

## Release plan

Releases follow the interface order CLI, MCP, TUI, web UI (D30), and each is usable without the next; dates are set after the spike.

&#91;embedded content: release plan · 1 gate, 5 releases\]

v2.0 is the runner alone: it fixes every v1 flaw and is what CI users adopt first. Load testing and agents follow in v2.1 because both reuse the v2.0 engine unchanged.

## Success metrics

These are targets to confirm once the language spike reports; the first four are checked in CI on every release.

| Metric | Target | How measured |
| --- | --- | --- |
| Compile and link 1,000 definition files | Under 500 ms cold, under 100 ms with the hash cache | Benchmark suite on a generated project |
| Redundant setup executions | Zero: a `beforeAll` call used by 50 tests or 5 repeats sends exactly 1 request | Mock server request counter |
| History query over 1,000 stored runs | Under 100 ms at p95 | Benchmark suite |
| Load on one 8-core machine | 5,000 virtual users with the tool under 70% CPU | Load benchmark against a local mock |
| Time to first green test | Under 5 minutes from `vc import openapi` on a public spec | Timed walkthrough in docs |
| Adoption | 1,000 GitHub stars and 50 external contributors within 12 months of v2.0 | GitHub insights |
| Agent usefulness | An agent generates, runs and fixes a passing suite for a sample OpenAPI spec without human edits | Scripted agent evaluation |

## Risks and dependencies

The biggest risk is scope: the full platform is the equivalent of Postman plus k6 for one maintainer, so releases must ship in order and each must be useful on its own.

| Risk | Effect | Mitigation |
| --- | --- | --- |
| Scope outruns one maintainer | Nothing ships | Strict release order; v2.0 is the runner only; AI agents work from the LLD |
| Language choice wrong for load | Rewrite of the hot path | Spike measures before code (D6); executor sits behind an interface |
| Varlock changes or stalls | Secrets layer breaks | Wrap it behind the secrets interface; keep an env-variable fallback |
| Third-party plugins abuse access | Leaked secrets in CI | Permissions manifest, approval on install, redaction (D42) |
| Agents act on production | Destructive calls | Per-environment policy and approval (D32) |
| `vc` collides with Vercel's CLI | Wrong tool runs | Installer warns; `vibranium` always works (D44) |
| OpenAPI sync conflicts confuse users | Lost edits | Three-way merge with an explicit conflict report; dry-run mode (D14) |

Dependencies: the language spike (decides runtime), Varlock (secrets), the CTRF specification (results), the MCP specification (agents) and the npm `@vibranium` organisation (package names, D45).
