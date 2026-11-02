# Vibranium v2 — Technical Design

Oct 22, 2026 · @Sarath

Vibranium v2 is a ports-and-adapters system: one engine compiles YAML, JSON or SDK-emitted definitions into a typed intermediate representation, plans a dependency graph and executes it, while every interface, transport, store and integration plugs in behind an interface. This design is language-neutral; the engine is written in Rust (D46), and the Vibranium v2 Low-Level Design maps it to crates, types and milestones. It implements the Vibranium v2 Product Requirements and Decision Log (D1–D45).

## Architecture

Interfaces call one Engine service, the engine runs a pure core pipeline, and the core reaches the outside world only through ports that adapters and plugins implement (D23).

&#91;embedded content: architecture · 5 interfaces, 1 engine, 6 core stages, ports, adapters\]

Because the CLI, MCP server, TUI, web UI and SDKs all call the same Engine service, a feature added to the engine reaches every interface without being re-implemented.

## Compile pipeline

Compilation turns files into a linked, immutable IR and an execution plan; any error stops the run before a request is sent.

&#91;embedded content: compile pipeline · 5 stages, diagnostics exit\]

| Stage | Input | Output | Fails when |
| --- | --- | --- | --- |
| Discover | Project root, include and exclude globs | File list with content hashes | Root config missing or invalid |
| Parse | Changed files (others come from the hash cache) | Raw nodes with source spans | Syntax errors |
| Validate | Raw nodes | IR nodes with defaults filled; templates and CEL parsed and type-checked | Unknown keys, wrong types, bad expressions |
| Link | All IR nodes | Symbol table and linked IR | Duplicate IDs, missing references, cycles, no active profile, vars used but never set |
| Plan | Linked IR and a selector (IDs, tags, files, `--changed`) | DAG of test instances and lifecycle actions, each annotated with the block that owns it | Selector matches nothing |

Diagnostics carry a stable code, a source span and, for unknown IDs, the closest matches by edit distance:

```
error[V2003]: unknown api `users.creat`
  --> services/users/users.test.yaml:14:11
   |
14 |     call: users.creat
   |           ^^^^^^^^^^^ did you mean `users.create`?
```

The compiler collects all errors in one pass instead of stopping at the first, and `vc check --json` returns them in a structured form for editors and agents.

## Intermediate representation and contracts

The IR is the single typed model every stage after parsing works on; it is defined once in protobuf (D26) and generated for the engine and every SDK, so YAML, JSON and code builders all produce the same thing.

| IR entity | Key fields | Notes |
| --- | --- | --- |
| `SourceSpan` | file, start line/column, end line/column | Attached to every node for diagnostics |
| `ApiGroup` | id, name, base URL, auth reference, headers, vars, profile overrides, OpenAPI source, request hooks, apis | One per `*.api.yaml` |
| `Api` | id, name, method, path template, headers, query, body (JSON, text, file, bytes, extend; encoding from content-type), vars, auth, expect, save, cleanup action, request hooks | Request template, checks that always hold, and how to undo a call |
| `TestFile` | id, name, tags, vars, profile overrides, lifecycle blocks (`beforeAll`, `afterAll`, `beforeEach`, `afterEach`), tests | One per `*.test.yaml` |
| `Test` | id, name, tags, vars, `before` and `after` blocks, call or steps, auth, body, expect, save, poll, examples, repeat, retry, timeout, skip, standalone | Unit of pass/fail |
| `LifecycleBlock` | owner (`RUN`, `FILE`, `INSTANCE`), phase (`BEFORE`, `AFTER`), per-test or per-file, actions | Placement fixes the lifetime of what its actions create |
| `Action` | kind (`CALL`, `TEST`, `FUNCTION`, `COMMAND`, `INLINE_REQUEST`), target, id, vars, auth, body, expect, save, poll, retry, keep, cache, loop (repeat, forEach, parallel) | One entry in a block, a test's steps or a flow auth |
| `Check` | kind (status, header, match, schema, equals, size, timing, cel, custom), operator, expected value, origin (api or test), source span | One reported result each |
| `VarScope` | level (call, test saves, file saves, test file, run saves, api, api file, config), values, profile overrides | Resolved innermost first |
| `Profile` | id, name, default, vars, agents policy | At least one; selected with `--profile`, several in order |
| `AuthDef` | id, type, vars, settings; for `flow`: steps, apply, expiresIn, refreshBefore, refreshOn, cache | Resolved once per run and set of vars |
| `RequestHook` | point (`beforeRequest`, `afterResponse`), function reference | Api file and api level |
| `LoadScenario` | id, mix (test, weight, thresholds), stages, think time, check mode, thresholds, abortOn | Load only |
| `Template` | literal parts plus references, pipes and generator calls with literal arguments; or a CEL expression for `$cel` | Parsed once, rendered per execution |

Templates are parsed at compile time into literal parts and typed references, so rendering never evaluates code and every reference is linked. CEL expressions are parsed and type-checked at compile time too, then evaluated per execution.

Protobuf packages:

- `vibranium.ir.v1`: the entities above.
- `vibranium.engine.v1`: the Engine service (compile, plan, run, call, query history) and its streamed run events.
- `vibranium.plugin.v1`: one service per plugin kind (transport, auth, secrets, reporter, notifier, executor, hook, importer).
- `vibranium.results.v1`: run, test and exchange records; maps to and from CTRF JSON.

Compatibility: fields are only ever added; breaking changes go to a new `v2` package with a converter, and the project file carries `version: 2`.

## Execution model

The executor walks the planned graph and runs each test instance as soon as the lifecycle actions it needs are ready; a shared action runs at most once per memo key, which is what removes v1's repeated dependency work.

### Scheduler

- Ready queue of test instances (test × data row × repeat index), bounded by a run-wide limit and a per-host limit.
- Each instance runs `beforeEach` and `before`, renders and sends each call through the transport, checks the api's `expect` merged with the test's, then runs `after`, `afterEach` and its cleanup stack.
- Events (`run.started`, `test.started`, `action.finished`, `exchange.completed`, `expectation.evaluated`, `cleanup.finished`, `test.finished`, `run.finished`) go to one event stream that reporters, the history writer, notifiers and interfaces subscribe to.
- Cancellation (Ctrl-C, `--bail`, timeout) stops new work, waits for in-flight requests up to a grace period, then runs teardowns.

### Lifecycle blocks, cleanup and cache

| Block | Owner frame | Runs | Memo key for its actions |
| --- | --- | --- | --- |
| `beforeRun` | run | value-producing actions lazily, on first need; commands and functions at run start | action id + hash of resolved vars |
| `beforeAll` | test file | before the file's first selected instance | file + action id + hash of resolved vars |
| `beforeEach`, `before` | test instance | once for every instance (test × example row × repeat index) | instance id |

- **Owner frames:** the executor keeps a frame per owner (run, file, instance) holding saved values, a cleanup stack and a status. A frame closes when its owner ends: it runs the matching `after` block, then pops its cleanup stack.
- **Singleflight:** concurrent instances that need the same `beforeRun` or `beforeAll` action wait on one in-flight execution; its saved values are stored in the owner frame under the action's `id` and are read-only.
- **Cleanup stack:** every 2xx call whose api declares `cleanup` pushes that cleanup, with vars resolved against the response, unless the call has `keep: true`. Frames pop newest first, also after cancellation; cleanup failures are reported but do not change test results.
- **Loops:** `repeat` and `forEach` expand into iterations run through a semaphore sized by `parallel` (default 1); each iteration writes its saved values into slot `i` of a pre-sized list, so the order is deterministic under parallelism.
- **Failure:** an action retries per its policy. A failed shared action marks every dependent instance `blocked` with the action's id, the failing request and response, and the check that failed (D11). A per-instance failure blocks only that instance; its `after` blocks and cleanups still run.
- **Cache:** an action with `cache` first looks up the `CacheStore` by (api or test id, canonical hash of resolved vars, profile). A live hit runs `cache.check` if present; on success the saved values are restored and nothing is pushed to the cleanup stack. An expired entry has its recorded cleanup run before a fresh execution. Values derived from secrets are sealed with a key from the OS keychain, or `VIB_CACHE_KEY` where there is none.
- **Static checks:** the linker builds the reuse graph from `test:` actions and reports cycles; it warns on `PUT`, `PATCH` or `DELETE` calls that read shared values, and on `cache` in per-instance blocks.

### Results per test instance

One of `passed`, `failed`, `blocked`, `skipped` or `flaky` (passed after a retry). Every expectation result carries its source span, expected value, actual value and a diff for structured values.

### Load engine

- Runs `LoadScenario` stages as a closed model: a target number of virtual users, each looping through its tests with optional think time.
- `beforeRun` and the `beforeAll` of every file in the mix run once before the first stage; `beforeEach` and `before` run per iteration and are reported apart from the mix.
- Load mode skips per-exchange history rows and records latency histograms (HDR) per second per test, plus error counts.
- Latency is measured from the scheduled start of each iteration, not the actual send, so a slow tool cannot hide server slowness (coordinated omission).
- Thresholds (`p99`, `errorRate`) are evaluated live and at the end; a breach fails the run and triggers alerts.

## Ports and first adapters

The core depends only on these interfaces; adapters are registered by kind and name (for example `reporter:junit`) and selected through config (D23).

| Port | Responsibility | First adapters (release) |
| --- | --- | --- |
| `SourceLoader` | Find and read definition files; report changes | Filesystem with hash cache (v2.0) |
| `FormatParser` | Turn a file into raw nodes with source spans | YAML, JSON (v2.0); SDK IR over the plugin protocol (v2.1) |
| `Importer` | Convert external specs to api groups; compute sync diffs | OpenAPI 3.x, v1 (v2.0); Postman, Bruno, `.http` (v2.2) |
| `Transport` | Send one request; return response with phase timings | HTTP/1.1 and HTTP/2 (v2.0); GraphQL (v2.3); gRPC, WebSocket, SSE (v2.4+) |
| `AuthProvider` | Produce and refresh credentials for a request | Basic, bearer, API key, OAuth 2.0 client credentials (v2.0); PKCE, refresh, plugins (v2.2) |
| `SecretProvider` | Resolve `${secret.*}` references | Varlock-based layer, environment variables (v2.0) |
| `ExpressionEvaluator` | Compile and evaluate CEL | CEL (v2.0) |
| `HookRunner` | Run user code hooks | Plugin protocol (v2.1) |
| `DataGenerator` | Fake values for `${fake.*}` | Faker-based (v2.0) |
| `ResultStore` | Persist and query runs | SQLite (v2.0); Postgres (v2.3) |
| `Reporter` | Turn the event stream or a stored run into output | CTRF, JUnit, console spec (v2.0); HTML with widgets (v2.1) |
| `Notifier` | Send alerts for trigger rules | Webhook, Slack (v2.0); Teams, PagerDuty, email (v2.2) |
| `Clock`, `FileSystem`, `Random` | Time, files and randomness | System adapters; fakes in tests |
| CacheStore | Keep the saved values and pending cleanup of cached actions across runs, per profile; seal secret-derived values | SQLite file .vibranium/cache.db with an OS keychain key (v2.0) |

`Clock`, `FileSystem` and `Random` are ports so that the compiler, planner and executor can be tested deterministically without touching the network or disk.

## Plugin system

First-party plugins are compiled into the binary; third-party plugins run as separate processes or services and speak the `vibranium.plugin.v1` protobuf contract over the transport that suits them (D19, D25).

| Transport | For | Why |
| --- | --- | --- |
| gRPC (streaming) | Protocol executors and anything called per request under load; engine ↔ server | Binary, multiplexed, streaming batches |
| JSON-RPC 2.0 over stdio | Most third-party plugins: reporters, notifiers, auth, secrets, hooks, importers | A plugin can be a short script in any language; easy to debug |
| REST (JSON over HTTP) | Plugins running as services | Works across machines and firewalls |
| Webhooks | Outbound integrations that only receive events | No process to manage |

All four are served from the same protobuf definitions using Connect RPC, which speaks gRPC, gRPC-Web and JSON over HTTP from one service definition; the JSON-RPC stdio bridge maps method names one-to-one. Ready-made client libraries for plugin authors ship for TypeScript, Python, Go and Java/Kotlin with the SDKs.

Lifecycle:

1. **Install:** `vc plugin add <source>` reads the plugin's manifest and shows the permissions it requests; the user approves (D42).
2. **Handshake:** on first use the engine starts the plugin, checks the protocol version and receives its capabilities and config schema.
3. **Configure:** plugin config from `vibranium.config.yaml` is validated against that schema before the run starts.
4. **Invoke:** calls carry a deadline; executors receive requests in streamed batches to amortise the process boundary.
5. **Shutdown:** plugins get a stop signal at run end; a crashed plugin fails only the work that used it.

Permissions manifest:

```yaml
# vibranium-plugin.yaml
name: acme-auth
kind: auth
protocol: vibranium.plugin.v1
entry: { command: ["python3", "-m", "acme_auth"] }
permissions:
  network: ["login.acme.example"]
  secrets: ["ACME_*"]
  filesystem: none
```

The engine passes a plugin only the secrets its manifest names and redacts them in everything the plugin returns. Network and filesystem limits are enforced where the operating system allows and always declared to the user.

## Config, secrets and redaction

Config is found at the project root and merged in fixed layers, and secrets are references that resolve lazily and are masked everywhere they could leak.

Discovery walks up from the working directory to the first `vibranium.config.yaml` or `vibranium.config.json`; `--project <path>` overrides it. Layers, lowest to highest:

1. Built-in defaults
2. User-global `~/.config/vibranium/config.yaml` (preferences only: theme, editor, telemetry opt-in)
3. Project `vibranium.config.yaml`
4. The active profiles (`--profile`), in order
5. `.env` and `.env.<name>` files, which should be git-ignored
6. `VIB_*` environment variables
7. CLI flags and `--set key=value`

The merged config is validated against its schema once; `vc config explain <key>` prints the value and its layer.

Secrets:

- References: `${secret.NAME}` in any string; `${env.NAME}` for plain environment variables.
- Resolution goes through the `SecretProvider` port. The default adapter is built on Varlock, so its providers (1Password, Vault, AWS, Azure, GCP, Bitwarden, Infisical) are available, while Vibranium keeps its own reference syntax (D21).
- Secrets resolve on first use, are cached for the run only, and are never written to disk by Vibranium.

Redaction: every resolved secret, every `Authorization` and `Cookie` header, and any field matching configured patterns is registered with one redactor. Console output, reporters, the history store, alerts, plugin responses and MCP results all pass through it, so a value is masked even if it appears inside a response body.

## Results, history, reports and alerts

Every run produces a CTRF document and a row set in the history store; reports and alerts are derived from those, never from engine internals (D43).

### History store

The default `ResultStore` adapter is SQLite in WAL mode at `.vibranium/history.db`. Tests never write directly: the event stream feeds one writer that commits in batches, so concurrent tests and readers (TUI, web UI, MCP) do not block each other (D24). A second `vc` process on the same file queues on a busy timeout.

| Table | One row per | Main columns |
| --- | --- | --- |
| `runs` | Run | id, started/finished, profiles, selector, git commit and branch, totals by status, CTRF path |
| `test_results` | Test instance | run id, test id, data row, repeat index, status, duration, blocked-by action, retry count |
| `expectations` | Evaluated expectation | test result id, kind, path, expected, actual, passed, source span |
| `exchanges` | Request and response | test result id, api id, method, URL, status, phase timings, headers and body per policy |
| `load_intervals` | Test × second in a load run | run id, test id, second, count, errors, HDR histogram blob |

- Body policy (D22): headers plus the first 4 KB of the body on failure by default; `none`, `failures` or `all` configurable; always redacted.
- Retention: keep the last N runs or N days, configurable; `vc history prune` on demand.
- Indexes on test id and start time keep the common queries (last failures, flaky tests, trend for one test) fast.

### Reports

CTRF JSON is written for every run. JUnit XML, console spec output and the HTML dashboard are reporters that read the CTRF document or the stored run, so `vc report <run>` can produce any of them later. The HTML dashboard is assembled from widgets (summary, failures, slowest tests, flaky tests, trends, load charts); a widget is a plugin that receives run data and returns HTML.

#### Keeping the HTML report fast at any size

v1 wrote every error, payload and response body into one HTML file, which became very slow to open; splitting it into several HTML files helped but made navigation awkward. v2 treats the report as a small app over data files that load only when needed:

| File | Holds | Loaded |
| --- | --- | --- |
| `report/index.html` | The app shell (one small script and stylesheet) | On open |
| `report/data/summary.js` | Run metadata, totals, root causes, the per-file breakdown, coverage | On open |
| `report/data/index.js` | One compact row per test instance: ID, status, duration, file, tags | On open; about 100 bytes per row, so 10,000 tests is about 1 MB |
| `report/data/tests/<shard>.js` | A test's checks, steps and exchange metadata (method, URL, status, timings, headers) | When its row is expanded or its page opened |
| `report/data/bodies/<hash>.js` | One request or response body, deduplicated by hash | When a Body tab is opened |

- **Works from disk.** The data files are JavaScript (`VIB.load("tests/0042", {...})`) rather than JSON, because browsers block `fetch` on `file://` but allow `<script>` tags. A report opened by double-clicking, from a CI artifact or from an emailed zip works the same way.
- **Only visible rows are drawn.** The tests table is virtualized and search runs over the in-memory index, so filtering 10,000 rows stays instant.
- **Direct links.** Every test has its own address (`index.html#/test/orders.checkout.placesOrder`), which loads only that test's shard, so links from CI comments and alerts open quickly.
- **Bodies follow the history policy.** Bodies follow `history.bodies` (default: failures only) and are truncated at `history.maxBodySize`, with the size and a note when cut. Load runs store per-second aggregates and a sample of errors (the first few per api and status), never every exchange.
- **Single-file option.** `reports.html.layout: single` produces one file for emailing. Each test's details are gzip-compressed and base64-encoded inside it and only decompressed (with the browser's `DecompressionStream`) when expanded, so even a single file opens fast. Above 20 MB, `vc` warns and suggests the folder layout.
- **One renderer.** The report and the web UI's run view share the same renderer, and `vc report <run>` can rebuild any report later from `history.db`.

Budgets checked in CI against a generated 10,000-test run: first paint under 1 second, an index under 2 MB, and opening any test under 100 ms.

### Alerts

```yaml
alerts:
  - when: [failure, recovery, flaky, threshold]
    notify: [slack:qa-channel, webhook:incident-bot]
    profiles: [staging, prod]
```

The alert evaluator compares a run with the previous run for the same selector and profiles to detect recoveries and new failures, then calls each notifier once per run with a summary and links.

## Agents, server and web UI

The MCP server, the self-hosted server and the web UI are adapters over the same Engine service as the CLI; none of them has its own execution logic.

### MCP server and guardrails

- `vc mcp` serves the tools listed in the PRD over stdio; the server also exposes them over HTTP for remote agents.
- Every tool call passes through a policy check before the engine sees it. Policy lives in each profile:

```yaml
profiles:
  - id: prod
    vars:
      ordersUrl: https://orders.shop.example
    agents:
      run: true
      call: [GET, HEAD]
      approve: [POST, PUT, PATCH, DELETE]
      writeDefinitions: false
```

- A denied call returns a structured refusal naming the rule; an approval-required call pauses until a human approves it through the client's confirmation prompt or the web UI.
- `write_definition` compiles the change before saving and returns diagnostics instead of writing invalid YAML.
- All agent output passes through the redactor, and every agent action is recorded in the history store with the tool name and arguments.

### Self-hosted server

- `vc serve` runs the Engine service over Connect RPC plus the web UI, with Postgres as its `ResultStore` adapter.
- CLIs and CI jobs push finished runs (`vc run --publish <server>`), so team history is shared without running tests on the server.
- Accounts are local or OIDC; a project maps to a git repository, and the server reads definitions from a checkout of it.
- Later releases add scheduled monitoring runs and mocks generated from api definitions (D38).

### Web UI

The web UI is a client of the Engine service. Its editor works on the YAML files: it loads a definition, edits it through forms or a code view validated by the published JSON Schema, and saves by writing the file (or committing to a branch on the server), so files stay the source of truth (D31).

## Monorepo and engineering standards

One repository holds the contracts, engine, adapters, apps and SDKs, and automated dependency rules keep the layers apart.

```
proto/                 vibranium.{ir,engine,plugin,results}.v1 (source of all types)
core/
  ir/                  generated IR types + JSON Schema for editors
  config/  loader/  compiler/  linker/  planner/  executor/  assertions/
adapters/
  transport-http/  auth/  secrets-varlock/  cel/  faker/
  store-sqlite/  store-postgres/  reporters/  notifiers/  importers/
engine/                composes core + adapters; the only thing apps import
apps/
  cli/  mcp/  tui/  server/  web/
sdks/
  typescript/  python/  go/  java/   generated types + builders + plugin clients
bench/                 generated projects, mock server, benchmark suite
docs/
```

Dependency rules, checked in CI:

- `core/*` imports only `core/ir` and port interfaces; never an adapter or app.
- `adapters/*` import `core/ir` and the port they implement; never each other.
- `apps/*` import only `engine` and `core/ir`.
- No global mutable state anywhere; v1's module-level counters are the example to avoid.

Coding standards, taken from Fowler's *Refactoring* code smells and made checkable:

| Smell | Rule | Enforced by |
| --- | --- | --- |
| Long Function | Functions under about 40 lines; cyclomatic complexity 10 or less | Linter |
| Long Parameter List | At most 3 parameters; group the rest into a parameter object (`RunContext`) | Linter |
| Switch Statements | Kinds of matcher, reporter, auth or transport are registry entries, not `switch` blocks | Review and registry design |
| Primitive Obsession | IDs and durations are distinct types (`ApiId`, `ActionId`, `Duration`), not bare strings or numbers | Type system |
| Mutable Data | IR is immutable after linking; only the executor's run state changes | Type system and review |
| Global Data | No singletons; dependencies are passed in through constructors | Linter and review |
| Shotgun Surgery | Adding a reporter or notifier touches one package plus one registry line | Contract tests |
| Comments | Names explain intent; comments explain why, not what | Review |
| Duplicated Code | One rendering path for templates, one for expectations | Duplication check |

Testing strategy:

1. Unit tests for compiler, linker and planner as pure functions, using the `Clock`, `FileSystem` and `Random` fakes.
2. Property-based tests for graph invariants: every shared action runs at most once per memo key; every registered cleanup runs exactly once, newest first; every cycle is reported.
3. Golden-file tests for diagnostics output.
4. A shared contract test suite that every adapter of a port must pass.
5. Integration tests against the in-repo mock server, whose request counter proves the lifecycle and cleanup guarantees.
6. Benchmarks in CI; a regression over 10% fails the build.

Field descriptions written once in protobuf flow into the JSON Schema, editor hovers, MCP tool descriptions and the docs site, so documentation cannot drift from the types.

## Questions for the spike and the LLD

The language spike answers the first group; the low-level design answers the rest once the language is chosen.

For the spike:

- Which language meets the compile target (1,000 files under 500 ms) with readable code?
- How much does a CEL evaluation cost per request, and does it matter at 5,000 virtual users?
- How clean are the port and adapter interfaces in each language, and how much boilerplate do they need?
- Are the needed libraries mature: YAML with source positions, JSON Schema, CEL, protobuf with Connect RPC, MCP SDK, TUI, SQLite, OAuth 2.0?

For the LLD:

- Exact protobuf messages and services, with field numbers.
- The plan data structure and the scheduler's queueing and fairness rules.
- Memo-key hashing: canonical form of resolved vars, and handling of non-deterministic values such as `${fake.*}`.
- Three-way merge algorithm and conflict format for OpenAPI sync.
- The SQLite schema with indexes, migrations and the batching policy of the writer.
- The JSON-RPC method mapping for each plugin kind, and the handshake messages.
- Diagnostic codes, message templates and the "did you mean" ranking.
