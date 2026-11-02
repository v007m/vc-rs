# Vibranium v2 — Low-Level Design (Rust)

Oct 23, 2026 · @Sarath

This is the build plan for the Vibranium v2 engine and its interfaces in Rust. It turns the Technical Design (architecture), the Syntax Reference (file formats) and the CLI and Developer Workflow doc (commands) into crates, types, algorithms and milestones precise enough for coding agents to implement one milestone at a time. Where this document and those disagree, those win on behaviour and this one wins on structure.

**Who reads it.** The coding agents that write the code, and Sarath, who reviews it without reading Rust day to day. Every section opens with what the part does in plain words; Rust appears only where the exact shape matters (types, trait signatures, schema), and each snippet is annotated.

**Rust for a TypeScript reader.** The snippets use a small set of ideas:

| Rust | Read it as (TypeScript) |
| --- | --- |
| `struct Api { id: ApiId, path: Template }` | `interface Api { id: ApiId; path: Template }` (a plain data shape) |
| `enum Body { Json(Value), Text(String), None }` | A tagged union: `{ kind: "json", value } \| { kind: "text", text } \| { kind: "none" }` |
| `Option<T>` | `T \| undefined` |
| `Result<T, E>` | A return value that is either `T` or an error `E`; no exceptions are thrown |
| `Vec<T>` / `IndexMap<K, V>` | `T[]` / a `Map` that keeps insertion order |
| `Arc<T>` | A shared, read-only reference to `T` (like passing an object around in JS, but immutable) |
| `trait Transport { async fn send(&self, r: Request) -> Result<Response> }` | `interface Transport { send(r: Request): Promise<Response> }` |
| `pub struct UserId(Sym)` (a "newtype") | A branded type: `type UserId = string & { __brand: "UserId" }` |
| `mpsc::Sender<Event>` | One end of a queue another task reads from (like posting to a worker) |
| `#[derive(Serialize)]` | Generated JSON conversion |

Versions named below were current on 23 October 2026 and are pinned in `Cargo.lock`; the Rust toolchain is installed with rustup and pinned in `rust-toolchain.toml` (1.99, edition 2024) (D98). The licence is Apache-2.0 (D35); the feasibility spike's MIT header does not carry over.

## Workspace and crates

One Cargo workspace builds one binary, `vc` (installed also as `vibranium`), which contains the CLI, terminal UI, MCP server and local web server. Code is split into small crates so each layer can be tested alone and the dependency direction can be checked by machine. Apps sit on the outside, the `engine` facade in the middle, pure logic underneath, and adapters plug into the interfaces. Each crate's package name is `vc-<dir>` (`crates/model` is `vc-model`); adapters are `vc-adapter-<x>` (D102).

```
vibranium/
  Cargo.toml                 workspace, shared lints, profiles
  rust-toolchain.toml        1.99 (D98)
  mise.toml                  pinned tools (node, pnpm, cargo tools, jscpd, buf) and tasks: gates, test, bench, mock (D99)
  package.json               pnpm workspace (with pnpm-workspace.yaml) for web/, sdks/* and jscpd (D99)
  proto/                     hand-written vibranium.{ir,engine,plugin,results}.v1 .proto files: the wire contract (D101)
  crates/
    model/                   IDs, spans, the compiled program, Plan and Event types (pure data) (D103)
    diagnostics/             Diagnostic, codes, fixes, rendering
    parser/                  YAML/JSON parsing with source spans; minimal TextEdits (D107)
    template/                templates, pipes, CEL wrapper
    compiler/                parse → validate → link → Program (pure; the engine supplies the sources, D132)
    planner/                 selection → Plan (pure)
    interfaces/              traits the core depends on (transport, auth, store, …)
    executor/                runs a Plan through the interfaces; frames, lifecycle, events, redactor (D106)
    loadtest/                load engine on top of the executor
    engine/                  facade: compile, plan, run, call, sync, query; the only crate apps import
    testkit/                 fakes, fixture builders, one contract suite per interface trait; dev-dependency only (D112)
    adapters/
      system/                discovery and file system, clock, random, environment, command runner, git (`Vcs`), config sources (D133)
      http/                  hyper + rustls transport with phase timings
      auth/                  auth providers, flow auth, token cache
      secrets/               Varlock and environment providers; the default is decided at M3 (they supply values; the redactor is in the executor, D106)
      hooks/                 QuickJS thread, TypeScript stripping (oxc)
      store/                 history.db and cache.db (SQLite)
      reporters/             CTRF, JUnit, HTML (folder and single), console
      notifiers/             webhook, Slack, Teams, PagerDuty, email
      importers/             OpenAPI, HAR, cURL, Postman, Bruno, v1
      plugins/               JSON-RPC and HTTP plugin host, permissions
    contracts/               protobuf types generated from proto/ (prost + pbjson) (D101)
    apps/
      cli/                   the vc binary: owns main and dispatches tui, mcp and ui (D110); clap commands, console output
      tui/                   ratatui screens
      mcp/                   MCP server (rmcp)
      server/                axum: Connect-protocol API, embedded web UI, MCP over HTTP
    arch-tests/              dependency and design-rule checks
  web/                       web UI: TypeScript, svelte/sveltekit (built, then embedded)
  sdks/                      typescript/ python/ go/ java/
  bench/
    mock/                    vc-mock-server: axum mock server; a library tests start in-process on port 0, plus a bin (D100)
                             also generated projects and benchmarks
  docs/                      reference docs, JSON Schemas for the file formats
```

Dependency rules: the crates form a layered graph with no cycles, enforced by `arch-tests` in CI.

| Layer | Crate | May depend on |
| --- | --- | --- |
| L0 | `vc-model` | std plus the D111 allow-list (below); nothing in the workspace |
| L1 | `vc-diagnostics` | model |
| L2 | `vc-parser`, `vc-template` | model, diagnostics (D105) |
| L2 | `vc-interfaces` | model, diagnostics |
| L2 | `vc-contracts` | model (generated; the `From` conversions live here because of the orphan rule) |
| L3 | `vc-compiler` | model, diagnostics, parser, template (pure, D132) |
| L3 | `vc-planner` | model, diagnostics (pure) |
| L3 | `vc-executor` | model, diagnostics, interfaces, template |
| L3 | `vc-adapter-*` | model, diagnostics, interfaces, their own third-party crates (plus contracts for `vc-adapter-plugins` only) |
| L4 | `vc-loadtest` | model, interfaces, executor (D104) |
| L5 | `vc-engine` | L0–L4; the only crate that names adapters (`EngineBuilder` is the composition root) |
| L6 | `vc-tui`, `vc-mcp`, `vc-server` | engine, which re-exports the model types they need (plus contracts for mcp and server) (D108) |
| L7 | `vc-cli` (bin `vc`) | engine, tui, mcp, server: the only app→app edges (D110) |
| — | `vc-testkit` | model, interfaces; used only as a dev-dependency (D112) |
| — | `vc-arch-tests`, `vc-mock-server` | no workspace crates |

Every edge points to a strictly lower layer; adapters never depend on each other or on the core logic crates. `arch-tests` asserts the whole table (crates for later milestones included, so adding them does not touch gate files), rejects unknown crates, and checks dev-dependency edges for cycles. Public items in `vc-model` and `vc-interfaces` may name only types from std, serde/serde_json, indexmap, smol_str, bytes and `http` (D111).

The web UI and the SDKs never link Rust code: they talk to the engine through the `vibranium.engine.v1` protobuf contract (Connect protocol over HTTP for the web UI, JSON-RPC over stdio or HTTP for plugins).

Third-party crates:

| Need | Crate (version) | Why this one |
| --- | --- | --- |
| Async runtime | `tokio` 1, `tokio-util` 0.7 (cancellation tokens) | The standard; used by hyper, axum, reqwest |
| HTTP client | `hyper` 1 + `hyper-util` 0.1 + `rustls` | Direct control of the connection, so DNS, connect, TLS, first-byte and download times can be measured; `reqwest` 0.13 for simple internal fetches (OpenAPI URLs) |
| YAML with positions | `saphyr-parser` 0.3 | YAML 1.2 events with a span each (line and column in characters); `parser` builds its own `Node` tree from them, keeping scalar styles and key spans, which diagnostics and minimal-diff edits need. JSON goes through the same parser until M1 D adds a dedicated JSON reader (D148) |
| JSON | `serde` 1, `serde_json` 1 | Standard |
| Template and pipe parsing | `winnow` 1 | Small, fast parser combinators with good error positions |
| CEL | `cel-interpreter` 0.10 | The maintained Rust CEL implementation; wrapped so it can be swapped |
| JSON Schema | `jsonschema` 0.60 | Body schema checks and file-format validation |
| Fake data | `fake` 5 | Faker-style generators; names mapped to the Faker JS catalogue used in templates |
| Diagnostics rendering | `miette` 7 | Rustc-style rendering with labels, help and codes; also emits JSON |
| Diffs | `similar` 3 | Line diffs for `--fix`, sync previews and the editor |
| Interning, maps | `indexmap` 2, `smol_str` 0.3; interning is hand-written (`StringTable` in `vc-model`) | Cheap IDs, ordered maps, small strings; `lasso` 0.7 was dropped because it pins `hashbrown` 0.14 next to indexmap's 0.17, which `multiple-versions = deny` rejects |
| Graphs | `petgraph` 0.8 | Cycle detection and topological order in the planner |
| Hashing | `blake3` 1 | File-hash cache, memo keys, body deduplication |
| File walking and watching | `ignore` 0.4, `notify` 9 | `.gitignore`-aware discovery; `vc watch` |
| SQLite | `rusqlite` 0.40 (bundled) | history.db and cache.db; no system dependency |
| Keychain and encryption | `keyring` 4, `chacha20poly1305` 0.11 | Cache encryption key in the OS keychain |
| Secret redaction | `aho-corasick` 1 | Fast multi-string replacement over every output |
| TypeScript hooks | `rquickjs` 0.14, `oxc` 0.155 | Measured in the spike: 4.2 MB, about 20 µs per call |
| OpenAPI | `oas3` 0.22 (3.1) and `openapiv3` 2 (3.0) | Both versions; Swagger 2.0 converted first |
| HAR | `har` 0.9 | HAR 1.2 parsing |
| Load statistics | `hdrhistogram` 7 | Latency histograms without coordinated-omission bias |
| CLI | `clap` 4 | Derive-based commands, shell completion |
| Terminal UI | `ratatui` 0.30, `crossterm` 0.29 | The standard Rust TUI stack |
| Web server | `axum` 0.8 | Serves the Connect API, the web UI and MCP over HTTP |
| Protobuf | `prost` 0.14, `pbjson` 0.9, `tonic` 0.14 (plugin gRPC only) | Generated types with proto3 JSON mapping |
| MCP | `rmcp` 3 | The official Rust MCP SDK |
| Logging | `tracing` 0.1 | Structured logs; `--log` level per crate |
| Errors | `thiserror` 2 (library crates), `anyhow` 1 (`vc-cli` only) | One typed error enum per library crate; context-rich errors only at the top (D136) |
| Async traits | `async-trait` 0.1 | Async methods on `Arc<dyn Trait>` until native dyn-async is stable (D115, D136) |
| Request middleware | `tower` 0.5 | The request pipeline as composable layers (D134, D136) |
| DNS and certificates | `hickory-resolver`; `rustls-platform-verifier` when first used | Timed DNS with the system resolver as fallback; the OS certificate store, so corporate CAs work (D136) |
| Regular expressions | `regex` 1 | The `regex` matcher and CEL `matches()` (D136) |
| Small helpers | `smallvec` 1, `camino` 1, `strsim` 0.11 | Scope levels without allocation; UTF-8 paths; "did you mean" suggestions (D136) |
| Lock-free swap | `arc-swap` 1 | The redactor's automaton is replaced without blocking readers (D113, D136) |
| Embedding, compression, secret memory | `rust-embed`, `zstd`, `zeroize`, each when first used | Web UI in the binary; stored bodies and `zstd` responses; secrets zeroed on drop (D136) |
| Testing | `insta` 1, `proptest` 1, `assert_cmd` 2 | Golden files for diagnostics and reports; property tests for the planner; e2e tests on the real `vc` binary (D136) |
| Benchmarks | `gungraun`, `criterion` | Instruction-count benches on PRs; wall-clock benches nightly (D120, D136) |
| Packaging | `cargo-dist` 0.32 | Release binaries, install scripts, Homebrew tap |

## Design rules and quality gates

The feasibility spike showed that Rust's borrow checker stays out of the way when data flows in one direction and shared data never changes. These rules make that the default, and the gates make breaking them fail CI rather than depend on review.

### Rules for the code

1. **The compiled program is read-only.** The compiler produces one `Program`; after that it is wrapped in `Arc` and never changes. Everything that varies during a run (saved values, cleanup stacks, statuses) lives in executor frames owned by one task each.
2. **Refer by ID, not by reference.** IR nodes point at each other with typed IDs (`ApiId`, `TestId`, `ActionId`) looked up in the program's tables, never with Rust references or pointers between nodes. No lifetimes appear in any public API.
3. **Talk through queues.** Long-lived parts (the history writer, the hook engine, reporters, the UI event stream) run as their own tasks and receive messages; nothing else touches their state.
4. **Locks only on the allow-list.** Exactly two files may hold a lock: the `TokenCache` (a mutex map of slots, never held across an `await`) and the `Redactor` (`arc-swap`). Shared lifecycle slots are `OnceCell`s, not locks (D113).
5. **No globals.** No `static mut`, no global registries; dependencies are passed in through constructors (the `engine` crate wires them).
6. **No panics in library code.** `unwrap`, `expect`, `panic!`, `todo!` and `dbg!` are denied by lint; errors are values (`Result`) with a diagnostic code where a user can act on them.
7. **Kinds are registries, not `match` on strings.** Matchers, pipes, auth types, reporters, importers and notifiers register by name in a table built at start-up, so adding one touches one file plus one registry line (the *Refactoring* "switch statements" and "shotgun surgery" smells).
8. **Distinct types for distinct things.** IDs, durations, sizes and file paths are newtypes, not bare strings or numbers.
9. **Small functions.** At most 40 lines, cognitive complexity at most 10, at most 3 parameters excluding `self` (group the rest into a context struct) (D109).
10. **Small files and traits.** A source file has at most 400 lines and a trait at most 7 methods; generated and test files are exempt (D114).
11. **A facade per crate.** Library crates keep their modules private and `pub use` the public items in `lib.rs`; no `pub mod`, except in the generated `vc-contracts`.
12. **Inject trait objects.** Dependencies are passed as `Arc<dyn Trait>`; async methods use `async-trait`, because `async fn` in a trait is still not dyn-compatible on Rust 1.99 (checked at M0); generics only where a bench proves the need (D115).
13. **Errors per crate.** Each library crate has its own `thiserror` enum; `anyhow` is used only in `vc-cli`.
14. **Contract suites.** Every interface trait has one contract suite in `vc-testkit`, and every adapter or fake that implements the trait runs it (D112).
15. **Conventional commits, tidy first.** `refactor:` commits stay apart from behaviour changes and never change test assertions; CI fails one that does (D116).

### Gates (`scripts/gates.sh`, required in CI)

| Gate | Fails when | Tool |
| --- | --- | --- |
| Lints | Any warning from clippy `pedantic` and `nursery`, plus the denied lints above; `cognitive_complexity` is switched on explicitly because current Clippy leaves it out of pedantic | `cargo clippy --all-targets -- -D warnings` |
| Function size | A function over 40 lines or complexity over 10, or more than 3 parameters (`too-many-arguments-threshold = 4`) (D109) | `clippy.toml` thresholds, `arch-tests` |
| Duplication | A copied block of 10 lines or more (D135) | `jscpd` |
| Architecture | A crate imports something the layer table forbids, or a rule above is broken: `static` state, a lock outside the D113 allow-list, `pub mod` in a library crate, more than 3 parameters, a file over 400 lines, a trait over 7 methods, an external type outside the D111 allow-list, `anyhow` outside `vc-cli`, `vc-testkit` as a normal dependency, an interface impl without its contract suite | `arch-tests` (cargo metadata plus a syntax scan); each failure prints a stable rule ID |
| Dependencies | A licence outside the allow-list, a known vulnerability, a duplicate version (`multiple-versions = deny`, with a skip list), a banned crate (`openssl-sys`, `v8`, `deno_core`), an unknown registry or git source, an unused dependency (D135) | `cargo-deny`, `cargo-machete` |
| Formatting | Unformatted code | `cargo fmt --check` |
| Tests | Any failing test or doctest (D135) | `cargo nextest run --workspace` plus `cargo test --doc` |
| Mutation score | Below 85% caught mutants in `executor`, `planner`, `template` and `compiler`: on the diff in PRs, the full run nightly (D119) | `cargo-mutants` |
| Performance | An instruction-count bench regresses by more than 5% on a PR (D120) | `gungraun` |
| Commits | A commit message that is not a conventional commit, or a `refactor:` commit that changes test assertions (D116) | `scripts/commit-check.sh` |

CI (D119): Ubuntu runs every gate; macOS and Windows build and run the tests; a nightly job runs full mutation testing and the wall-clock budgets. `scripts/gates-selftest.sh` applies one bad patch per gate and asserts that the expected gate rejects it, so a gate cannot silently stop working (D121). From the end of M2, `cargo public-api` snapshots for `vc-model`, `vc-interfaces` and `vc-engine` make any public API change visible in review (D117).

Coding agents may not edit `clippy.toml`, the `[workspace.lints]` table, `deny.toml`, `scripts/gates.sh` or `arch-tests`; a CODEOWNERS rule routes those files to Sarath. `master` is protected: required gates and cross-OS test checks, CODEOWNERS review with admin bypass, linear history, no force-push (D97, D118). Smells no tool catches (naming, speculative generality, duplicated logic written differently) are covered by a ten-line review checklist in `CONTRIBUTING.md` that every PR description answers.

## Orchestration and design patterns

The execution workflow is run by an **orchestrator**: one component decides what happens next and holds the workflow's state, while workers only carry out the step they are given and report the result. Nothing in the system chains work by calling the next part directly (choreography), so the whole flow of a run can be read in one place, tested as a state machine, and changed without touching the workers.

&#91;embedded content: orchestration · orchestrators, strategies, events, ports\]

Orchestration is hierarchical, so each level stays small. Orchestrators and workflows decide; strategies and adapters act. All of them depend on interfaces (traits in `vc-interfaces`), never on each other's concrete types.

### The orchestrators and their state machines

| Level | Owns | States | Decides |
| --- | --- | --- | --- |
| `RunOrchestrator` (one per run) | the run's state, the shared `beforeRun` slots, the root cancellation token, the run cleanup list | `Planned → Preparing → Running → Draining → CleaningUp → Reporting → Finished(Passed \| Failed \| Cancelled)` | when files start, when to stop (bail, Ctrl-C, abortOn), the final status |
| `FileOrchestrator` (one per test file) | `beforeAll` slots, the file's cleanup list, the file's instance queue | `Waiting → Preparing → Running → CleaningUp → Done` | which instance starts next (under the concurrency limit), when `afterAll` runs |
| `InstanceWorkflow` (one per test instance) | the instance frame: saved values, cleanup stack, results | `Pending → Before → Body → After → Cleanup → Done(Passed \| Failed \| Blocked \| Flaky \| Skipped)` | retries of the whole instance, stop on the first failed step, blocked when a shared action failed (D126) |

Each state machine is an `enum` of states and a single `transition(state, input) -> (state, commands)` function. It is pure: no I/O, no clock. The orchestrator task feeds it inputs (results from workers, timer ticks, cancellation) and carries out the commands it returns (start an action, start a file, emit an event). Illegal transitions cannot be written, because each state only accepts the inputs that make sense for it. The transition functions are tested exhaustively with table tests and `proptest`.

```rust
// The instance workflow, simplified. Pure: inputs in, next state and commands out.
pub enum InstanceState { Pending, Before(Cursor), Body(Cursor), After(Cursor), Cleanup(Cursor), Done(Status) } // Status includes Skipped (D126)
pub enum InstanceInput { Start, Block(Failure), ActionDone(ActionOutcome), Cancelled } // the file orchestrator sends Start or Block (from the needs analysis) once beforeRun and beforeAll have finished (D122)
pub enum Command { RunAction(ActionId, OwnerKey), RunCleanup(PendingCleanup), Emit(Event), Finish(Status) }

pub fn transition(state: InstanceState, input: InstanceInput, test: &Test) -> (InstanceState, Vec<Command>);
```

**Cleanups are compensations (saga pattern).** Every successful call that changes data registers its compensating action (the api's `cleanup`). The orchestrator owning the frame runs compensations in reverse order when the frame ends, whether the work succeeded, failed or was cancelled. That is how a run leaves nothing behind without any test mentioning deletion.

### Where each pattern is used

A pattern is used where it removes a real coupling or a real `match`/`if` chain: two or more implementations exist, or a test needs a seam (YAGNI). The table is the complete list, so a reviewer can tell an intended pattern from an accidental one.

| Pattern | Used for | Why |
| --- | --- | --- |
| **Orchestrator + state machine** | `RunOrchestrator`, `FileOrchestrator`, `InstanceWorkflow`; also the load stage controller and the HAR import wizard | One place decides and holds state; workers stay simple and testable |
| **Saga (compensation)** | Api `cleanup` as compensating actions, run in reverse | Undo without tests describing it |
| **Ports and adapters** (hexagonal) | Every outside dependency is a trait in `vc-interfaces`: `Transport`, `AuthProvider`, `SecretProvider`, `HookRunner`, `HistoryWriter`, `HistoryReader`, `CacheStore`, `Reporter`, `Notifier`, `Importer`, `Clock`, `Random`, `FileSystem`, `SourceProvider`, `CommandRunner`, `Vcs`, `ConfigSource`, `ActionRunner` | The core never knows which library sits behind it |
| **Strategy** | Action executors (call, test reuse, function, command); matchers; pipes; body encoders (JSON, form, multipart, file); retry and loop policies (sequential, parallel); the load mix selector; sync merge rules | Swap behaviour by kind without branching in the caller |
| **Factory and registry** | One registry per kind of strategy or adapter, filled at start-up: `ActionExecutorFactory` (by action kind), `AuthProviderFactory` (by auth `type`), `ReporterFactory`, `NotifierFactory`, `ImporterFactory` (by source), `TransportFactory` (by URL scheme, ready for gRPC and WebSocket), `MatcherRegistry`, `PipeRegistry`, `LintRegistry` | Adding a kind is one new file plus one registration line; nothing else changes |
| **Builder** | `EngineBuilder` (the composition root that wires adapters into the engine); `ProgramBuilder` (the compiler accumulates, then `freeze()` gives the immutable `Program`); `RequestBuilder` (renders a request from an api, scope and auth); `PlanBuilder`; `Diagnostic::new(…)` with chained `label`, `help`, `fix`; test fixtures (`ProjectBuilder`, `ProgramBuilder::for_tests`); the SDKs' fluent builders | Objects with many optional parts are assembled step by step and checked once; required parts are enforced by typed builder states |
| **Facade** | The `engine` crate: `compile`, `plan`, `run`, `call`, `sync`, `query` | Apps depend on one small surface |
| **Observer (publish/subscribe)** | The event bus and its subscribers: history writer, reporters, console, TUI, web UI, MCP | Producers never know who listens |
| **Chain of responsibility (middleware)** | The request pipeline as `tower` layers: timing → retry (`refreshOn`, `http.retry`) → auth → `beforeRequest` hooks → per-host limit → timeout (per attempt) → transport (D134, D141) | Each concern is one layer, composable and testable alone |
| **Decorator** | `CachingAuthProvider` (the token cache around any provider); `RedactingSink` around every outward stream; `RecordingTransport` and `FaultyTransport` in tests | Add behaviour without changing the wrapped type |
| **Command** | Engine API operations; `parser::edit` changes as `EditCommand`s that produce `TextEdit`s and can be inverted (undo in the web editor) | Operations become values: logged, previewed, undone |
| **Interpreter and composite** | Templates and pipes; CEL behind `Expr`; matchers composed with `allOf`, `anyOf`, `not`, `every` | Small languages evaluated without running user code |
| **Visitor** | Walks over the `Program` for linking, lints and the planner's needs analysis (`Visit` trait with default no-op methods) | New analyses without editing the IR types |
| **Specification** | Run selection: IDs, globs, paths, tags, grep, changed and failed as composable predicates | Selectors combine freely ("or" for positional, "and" for flags) |
| **Repository** | `HistoryReader`/`HistoryWriter`, `CacheStore` with SQLite and Postgres implementations | Storage swappable; queries named by intent |
| **Null object** | `NoAuth`, `NoopReporter`, `NoHooks` | No `Option` checks scattered through callers |
| **Singleflight (memoization)** | Shared lifecycle slots and the token cache | Concurrent callers share one execution |
| **Flyweight** | Interned strings (`Sym`) for every ID and name | Small, cheap-to-copy IDs across a large program |

### Rules that keep coupling loose

- **Depend on interfaces.** Core crates take `Arc<dyn Trait>` through constructors, with generics only where a bench proves the need (D115); only `EngineBuilder` names concrete adapter types. `arch-tests` fails if a core crate's public signature mentions an adapter type.
- **Small interfaces.** Traits are split by who uses them (`HistoryWriter` for the writer task, `HistoryReader` for queries; `Transport` knows nothing of auth), so no consumer depends on methods it doesn't call.
- **Data crosses boundaries, behaviour doesn't.** Between crates only `vc-model` types (including `Plan` and `Event`, D103) and `vc-interfaces` traits are shared; no crate reaches into another's internals (`pub(crate)` by default).
- **Tell, don't ask, and no train wrecks.** Workers receive the data they need in the command; a function that reaches through `a.b().c().d()` gets that value passed in instead (Law of Demeter, enforced in review).
- **Composition over inheritance.** Behaviour is shared by composing strategies and decorators, never by deep type hierarchies or default-method templates.
- **Refactoring before extending.** When a change needs a `match` on a kind in a second place, it becomes a strategy with a registry ("replace conditional with polymorphism"). When a function grows past the limits, extract functions or a parameter object first, then add the feature.

## Core types

The `vc-model` crate holds the compiled program, and also the `Plan` and `Event` types (D103): plain data, no behaviour beyond lookups. Every other part reads it. Its types are written by hand; the `vibranium.ir.v1` protobuf messages are separate hand-written `.proto` files (checked by `buf lint` and `buf breaking`), generated into `vc-contracts` and TypeScript, with `From` conversions in `vc-contracts`, so that the SDKs, plugins and web UI see the same shapes. Each proto is written when its first consumer needs it (D101).

### IDs and positions

```rust
/// An interned string: a 4-byte handle into the program's string table.
/// Cheap to copy and compare; turned back into text with `program.str(sym)`.
/// Symbols order by interning order, not alphabetically.
pub struct Sym(u32);

/// The interned strings: a small hand-written interner (no `lasso`, see the crate table).
/// `intern(&mut self, &str) -> Result<Sym, ModelError>` while building;
/// `get(&self, &str) -> Option<Sym>` and `resolve(&self, Sym) -> Option<&str>` once frozen.
pub struct StringTable { strings: Vec<Arc<str>>, index: HashMap<Arc<str>, Sym> }

// One newtype per kind of ID, so an ApiId can never be passed where a TestId is expected.
// Each has `new(Sym)` and `sym()`; the index IDs have `new(u32)` and `index()`.
pub struct FileId(Sym);      // "users.crud"
pub struct ApiId(Sym);       // "users#create"          (full ID: file id # item id, D138)
pub struct TestId(Sym);      // "users.crud#createsMember"
pub struct ActionId(u32);    // index into program.actions
pub struct AuthId(Sym);      // "shopOauth"
pub struct ProfileId(Sym);   // "staging"
pub struct VarName(Sym);     // "userId"
pub struct DatasetId(Sym);   // "users.seed"
pub struct SourceFileId(u32); // index into program.files

/// Where something came from, for every diagnostic and report line.
/// Spans order by file, then start, then end: the order diagnostics are shown in.
pub struct SourceSpan {
    pub file: SourceFileId,  // index into program.files
    pub start: Pos,          // see Pos
    pub end: Pos,            // just after the last position covered
}

/// 1-based line and column (in characters) plus the 0-based byte offset.
pub struct Pos { pub line: u32, pub column: u32, pub offset: usize }

pub struct Duration(std::time::Duration); // parsed from "250ms", "2s", "1m" by the template crate
pub struct Size(u64);                       // bytes, parsed from "4kb", "1mb" by the template crate
```

### The program

```rust
/// Everything `vc check` produced, frozen and shared read-only (Arc<Program>).
pub struct Program {
    strings: StringTable,                            // the interned strings; private, reached through program.str(sym)
    pub files: Vec<SourceFile>,                      // path, kind, content hash
    pub config: Config,                              // defaults, reports, history, secrets
    pub profiles: IndexMap<ProfileId, Profile>,      // at least one; one marked default
    pub auth: IndexMap<AuthId, AuthDef>,
    pub api_groups: IndexMap<FileId, ApiGroup>,
    pub apis: IndexMap<ApiId, Api>,
    pub test_files: IndexMap<FileId, TestFile>,
    pub tests: IndexMap<TestId, Test>,
    pub actions: Vec<Action>,                        // every action anywhere, by ActionId
    pub run_blocks: Lifecycle,                       // beforeRun / afterRun
    pub loads: IndexMap<FileId, LoadScenario>,
    pub datasets: IndexMap<DatasetId, Dataset>,
    pub generators: IndexMap<Sym, GeneratorDef>,
}
```

### Apis, tests and actions

```rust
pub struct Api {
    pub id: ApiId,
    pub name: Option<String>,
    pub group: FileId,                    // its api file: base URL, auth, headers come from here
    pub method: Method,
    pub path: Template,
    pub vars: VarDefs,                    // the api's defaults
    pub query: IndexMap<String, Template>,
    pub headers: IndexMap<HeaderName, Template>,
    pub body: BodySpec,
    pub auth: AuthRef,                    // Inherit | None | Use { id, vars }
    pub timeout: Option<Duration>,
    pub expect: Expect,
    pub save: SaveSpec,
    pub cleanup: Option<ActionId>,        // a CALL action with vars templated over the response
    pub hooks: RequestHooks,
    pub deprecated: bool,
    pub span: SourceSpan,
}

pub enum BodySpec {
    None,
    Json(TemplateValue),                  // nested YAML/JSON with templates inside
    Text(Template),
    File(PathTemplate),
    Base64(Template),
    Extend { base: Template, changes: TemplateValue, remove: Vec<Path> },
    Form(IndexMap<String, TemplateValue>),       // chosen by content-type
    Multipart(IndexMap<String, PartSpec>),
}

pub struct TestFile {
    pub id: FileId,
    pub tags: Vec<Tag>,
    pub vars: VarDefs,
    pub skip: Option<Skip>,
    pub lifecycle: Lifecycle,             // beforeAll, afterAll, beforeEach, afterEach
    pub tests: Vec<TestId>,               // in file order
}

pub struct Test {
    pub id: TestId,
    pub file: FileId,
    pub name: Option<String>,
    pub tags: Vec<Tag>,
    pub before: Vec<ActionId>,
    pub after: Vec<ActionId>,
    pub body: TestBody,                   // Call(ActionId) | Steps(Vec<ActionId>)
    pub vars: VarDefs,
    pub expect: Expect,                   // applied to the call; with steps, a default for every step (D124)
    pub examples: Option<Examples>,       // rows (literal, $file or dataset)
    pub repeat: u32,
    pub retry: Option<Retry>,
    pub timeout: Option<Duration>,
    pub skip: Option<Skip>,               // reason, until
    pub standalone: bool,
    pub span: SourceSpan,
}

pub struct Lifecycle {
    pub before_all: Vec<ActionId>, pub after_all: Vec<ActionId>,
    pub before_each: Vec<ActionId>, pub after_each: Vec<ActionId>,
}

/// One entry in a lifecycle block, a test's steps, or a flow auth.
pub struct Action {
    pub id: Option<Sym>,                  // required in beforeRun/beforeAll when it saves values
    pub kind: ActionKind,
    pub owner: Owner,                     // Run | File(FileId) | Instance (each/before/steps)
    pub vars: VarDefs,
    pub auth: Option<AuthRef>,
    pub body: Option<BodySpec>,           // replaces the api's body
    pub expect: Expect,                   // merged over the api's expect at compile time
    pub save: SaveSpec,
    pub poll: Option<Poll>,
    pub retry: Option<Retry>,
    pub keep: bool,                       // skip the api's cleanup
    pub cache: Option<CacheSpec>,         // for, check (an ActionId)
    pub repeat: Option<Loop>,             // Repeat(n) | ForEach(TemplateValue), with parallel
    pub span: SourceSpan,
}

pub enum ActionKind {
    Call(ApiId),
    Test(TestId),                         // reuse another test's calls
    Function(HookRef),                    // path#export
    Command(Template),
    Inline(InlineRequest),                // flow auth steps only
}
```

### Checks, templates and vars

```rust
/// The merged checks for one call, keyed by kind + leaf path: api → test → step, more specific replaces; custom and CEL append (D123).
pub struct Expect { pub checks: Vec<Check> }

pub struct Check {
    pub kind: CheckKind,                  // Status, Header, Schema, Match, Equals, Size, Timing, Cel, Custom
    pub target: Option<Path>,             // e.g. body.items[*].price
    pub matcher: Matcher,                 // a registered matcher with its compiled argument
    pub origin: Origin,                   // Api(ApiId) | Test(TestId) | Action(ActionId)
    pub span: SourceSpan,                 // the YAML line reported when it fails
}

/// "/users/${var.userId}" is stored as parts, parsed once at compile time.
pub struct Template { pub parts: Vec<Part> }
pub enum Part { Literal(String), Ref(Reference) }
pub struct Reference {
    pub ns: Namespace,                    // Var, Env, Secret, Response, Request, Step(Sym), Example, Item, Iteration, Fake, Dataset, Run (D125)
    pub path: Path,                       // .body.items[0].id
    pub pipes: Vec<PipeCall>,             // | int | default('x')
    pub span: SourceSpan,
}

/// A value that may hold templates anywhere inside (bodies, vars, saves).
pub enum TemplateValue {
    Null, Bool(bool), Number(serde_json::Number), String(Template),
    List(Vec<TemplateValue>), Map(IndexMap<String, TemplateValue>),
    Cel(CelProgram),                      // $cel: computed value
    Extend { base: Template, changes: Box<TemplateValue>, remove: Vec<Path> },
}

/// Vars set at one level; the executor stacks levels at run time (see Vars, templates and CEL).
pub struct VarDefs { pub values: IndexMap<VarName, TemplateValue>, pub by_profile: IndexMap<ProfileId, IndexMap<VarName, TemplateValue>> }
```

The remaining entities (`ApiGroup`, `Profile`, `AuthDef`, `LoadScenario`, `Dataset`, `Config`) are direct translations of the keys tables in the Syntax Reference, one field per key, with durations and sizes already parsed and every template already compiled.

## Compiler

The compiler turns the project's files into a `Program` or a list of diagnostics. It is pure: the engine reads the files through the `SourceProvider` trait and hands the compiler plain `SourceFile { path, text, hash }` values (a `vc-model` type), so the compiler never depends on `vc-interfaces`; the compiler returns the program or the diagnostics, plus a `CompileCache` value that the engine persists (D132). It runs on `vc check`, at the start of every run, on every keystroke in the editors (through the engine's `Check` call) and before any command writes a file. It collects every problem in one pass instead of stopping at the first.

### Stages

| Stage | Input | Output | Notes |
| --- | --- | --- | --- |
| Discover (engine, not compiler) | project root, `paths` globs | source texts with BLAKE3 content hashes, through `SourceProvider` | `vc-adapter-system` walks with `ignore` (respects `.gitignore`); file kind from the suffix (D132, D133) |
| Parse | changed files only | a marked tree per file (`parser::Node`, every node with its span) | `parser::parse(file, text) -> (Option<Node>, Diagnostics)` over `saphyr-parser` events; maps keep key order and key spans, scalars keep their text, style and YAML 1.2 core kind; columns count characters, offsets count bytes; a node's span starts at the value, after any `&anchor` or `!tag`. The root is `None` after a scanner error (V1001; V1002 for structure; V1007 for an alias to a missing anchor) and when nesting goes deeper than 128 (V1011); an empty file gives a null root. Problems the tree survives keep it and are reported: V1006 duplicate key, V1007 alias, V1008 tag, V1009 non-text key, V1010 second document. Scanner errors are coded from saphyr's message through one table, with a test per entry. JSON goes through the same parser for now, except that surrogate-pair `\u` escapes are refused (V1001) and a trailing comma is accepted; M1 D replaces this with a dedicated span-keeping JSON reader for `.json` files into the same `Node` tree (D148) |
| Shape-check | marked trees | raw items per file kind, with unknown keys, wrong types and missing required keys reported | shapes are declared once (below) |
| Compile parts | raw items | templates, pipes, CEL programs, matchers, durations, sizes | `template` crate; CEL type-checked against the namespaces available at that spot |
| Link | all raw items | the `Program`: ID tables, references resolved, merged `expect`, test-level step defaults, var reachability | the only stage that sees every file at once |
| Lint | the `Program` | warnings: naming conventions, expired skips, shared-value writes, cache in per-test blocks | each lint is a registered rule with a V-code |

**Incremental.** The `CompileCache` keeps, per file hash, its parsed and shape-checked items; the compiler takes the previous one as input and returns the new one, and the engine reads and writes it as `.vibranium/cache/compile.bin` (D132). On the next run only changed files are parsed again; linking always runs over everything but is cheap (target: 1,000 files in under 100 ms warm, under 500 ms cold). The editor and `vc watch` keep the cache in memory.

**One declaration per file shape.** Each file kind's keys are declared once in Rust, with type, default, description and an example:

```rust
shape!(Test {
    id: Id required "camelCase reference, e.g. createsMember",
    name: Text "shown in reports",
    call: ApiRef one_of(call, steps) "the api to call",
    expect: Expect default(api) "added to the api's checks; matching keys replace them",
    skip: Skip "a reason, or reason and until",
    // … one line per key in the Syntax Reference
});
```

The same declarations generate the JSON Schemas shipped for editors (`vc schema export`), the reference tables in the docs, the hover text in editors and the web UI, and the shape-check stage, so the docs, the editors and the checker cannot drift apart.

### Linking in detail

1. **Symbol tables.** Every file ID, api ID, test ID, auth ID, profile, dataset and generator goes into a table. Each file has one ID namespace across kinds, so two items in one file with the same ID are V2001 with both locations, whatever their kinds; a reserved ID is V2006. Another file's items are referenced by full ID, `fileId#id`; a bare dotted word is always a file ID (D128, D138). File IDs are derived from paths; an explicit `id:` that differs is V2007, and two files with the same ID are V2001 (D139). The `CompileCache` keeps each file's content hash and last ID, so a moved file is recognised and V2003 can offer its new ID as a certain fix.
2. **References.** `call`, `test`, `cleanup.call`, `auth`, `$file`, `function`, `forEach` datasets and load `mix` entries are resolved; unknown ones are V2003 with "did you mean" from Damerau–Levenshtein distance (`strsim`) over the IDs of the right kind, ranked with same-file IDs first.
3. **Expect merge.** Checks are keyed by check kind plus leaf path. Levels merge api → test → step (the step wins), and the more specific level replaces a check with the same key; `custom` and CEL checks append instead. `expect: { replace: true }` drops the inherited checks. Every check carries its origin and source line (D123).
4. **Test-level step defaults.** A test's `auth`, `headers`, `query`, `body`, `poll` and `expect` are defaults for every step, and the step's own value wins. A body default that reaches a GET or HEAD step is V3205; a default no step uses is V3206. A test-level `save` reads the last step's response (D124).
5. **Var reachability.** For each template, the linker walks the levels that can supply a var at that spot (call vars, earlier saves in the same test, beforeAll saves under their action id, file vars, beforeRun saves, the api's defaults, the api file, config), separately for every profile. A `function:` or `test:` action may declare `provides: [x]`, and the linker trusts that declaration (D128). A var that no level provides for some profile is V2101, naming the profiles where it is missing and the places it could be set. A template root that is not available at that spot (D125) is V2105.
6. **Lifecycle graph.** `test:` reuse forms a graph; cycles are V2205 with the whole chain. Shared actions without an `id` are V2210.
7. **Types.** Pipes and CEL are checked against what each reference can hold (a `| number` on a value the api's schema says is an object is V2304).

### Diagnostics and fixes

```rust
pub struct Diagnostic {
    pub code: Code,                       // V2003
    pub severity: Severity,               // Info < Warning < Error; worst = max()
    pub message: String,                  // unknown api "orders#plcae"
    pub primary: Label,                   // span + short label under the caret
    pub secondary: Vec<Label>,            // other places involved (e.g. the earlier duplicate)
    pub notes: Vec<String>,               // = note: …
    pub help: Vec<String>,                // = help: …
    pub fixes: Vec<Fix>,
}

pub struct Fix {
    pub title: String,                    // Replace with orders#place
    pub certain: bool,                    // true: vc check --fix may apply it without asking per fix
    pub edits: Vec<TextEdit>,             // byte-range replacements, possibly across several files
}

pub struct TextEdit { pub file: SourceFileId, pub range: Range<usize>, pub new_text: String }
```

`Code` is a registry of associated consts (`Code::V2003`), each with its number, default severity and title, and a category (V1 to V5) derived from the number's first digit; `Code::ALL` lists them in numeric order and `"V2003".parse::<Code>()` looks one up. The table is the one under [Diagnostic codes](#diagnostic-codes); a failure or blocked result there has severity `Error`. Diagnostics are built step by step, taking the severity from the code:

```rust
Diagnostic::new(Code::V2003, span, "unknown api \"orders#plcae\"")
    .label("not defined").help("did you mean orders#place?")
    .fix(Fix::certain("Replace with orders#place", vec![TextEdit::replace(span, "orders#place")]))
```

A `Diagnostics` collection gathers them (`push`, `extend`, `has_errors`) and `sort()`s them for display by file-table order (the compiler adds files in path order), then position, then code. These types serialize with serde (camelCase keys, codes as `"V2003"`, severities in lowercase), but that is not yet the `vc check --json` shape: spans hold a `SourceFileId` index, and the JSON reporter (A2) replaces it with the file path.

Diagnostics render through `miette` in the terminal, as JSON with `--json`, and over the engine API to the editors and the web UI, which show the same code, text and fixes.

### Editing files without reformatting them

Everything that changes a definition file (`vc new`, `vc rename`, `--fix`, `vc sync`, `vc skip`, HAR import, the web editor's form view, agents' `write_definition`) goes through one module, `parser::edit`. It takes the original text and its marked tree and turns structural changes (set a key, insert a list item, remove an item, rename a reference) into minimal `TextEdit`s on the original text, keeping comments, quoting and spacing everywhere else, and checks that the result parses again. New text is written in the house style (`vc fmt`): block YAML, two-space indent, keys in the documented order, templates unquoted unless YAML needs quotes. The guard lives in the engine: it compiles before and after every edit and refuses the edit if it would add an error (D107).

## Planner

The planner decides what a run will do before anything is sent: which test instances, in which files, needing which lifecycle actions. `vc plan` prints this; `vc run` prints its first line. It is a pure function, so it is tested with property tests rather than against servers.

```rust
// Pure: `$file` examples and datasets arrive preloaded in `options` (D132); `Plan` lives in vc-model (D103).
pub fn plan(program: &Program, selection: &Selection, options: &RunOptions) -> Result<Plan, Vec<Diagnostic>>;

pub struct Selection {
    pub ids: Vec<IdPattern>,           // test ID, file ID, glob, test:row (D140)
    pub paths: Vec<Utf8PathBuf>,
    pub tags: TagExpr,                 // smoke,checkout & !slow
    pub grep: Option<String>,
    pub changed_since: Option<GitRef>, // resolved to file IDs by the engine through the `Vcs` trait before planning
    pub failed_in: Option<RunId>,      // from history; arrives in M3 with the history store (D130)
    pub include_skipped: bool,
}

pub struct Plan {
    pub profiles: Vec<ProfileId>,
    pub run_actions: Vec<PlannedAction>,         // beforeRun actions, planned when any instance is selected (D122)
    pub files: Vec<PlannedFile>,                 // in project order
    pub skipped: Vec<SkippedTest>,               // reason, until; reported with the `Skipped` state, never run (D126)
    pub instance_count: usize,
}

pub struct PlannedFile {
    pub id: FileId,
    pub before_all: Vec<PlannedAction>,
    pub instances: Vec<Instance>,
    pub after_all: Vec<ActionId>,
}

pub struct Instance {
    pub test: TestId,
    pub example_row: Option<u32>,
    pub repeat_index: u32,
    pub needs: BitSet,                           // shared actions this instance reads: decides which are blocked by a failure
}
```

How it works:

1. **Select.** Apply the selectors to the test table: positional selectors are joined with "or", flags filter with "and". Skipped tests and files go to `skipped` unless `include_skipped`; an expired skip is ignored, so the test runs, and the lint already warned.
2. **Expand instances.** For each selected test, create one instance per example row × repeat (dataset rows are counted at plan time; `$file` examples and dataset contents are preloaded by the engine and passed in, so the planner does no I/O (D132)).
3. **Plan lifecycle and find what each instance needs.** `beforeRun` is planned when the run has a selected instance, and a file's `beforeAll` when that file has one; both run eagerly and finish before any instance starts (D122). The planner still walks each instance's templates (including those of the calls it makes and of `test:` actions it reuses) and records which shared actions it reads; this needs analysis decides which instances are blocked when a shared action fails. Laziness remains only in the token cache and in cache-store hits.
4. **Order.** Files keep project order; instances keep file order. `petgraph` checks the action graph is acyclic (already guaranteed by the linker, re-asserted here). Concurrency is decided at run time by the scheduler, not by the plan.
5. **Explain.** `Plan` renders to the plan line (`12 tests in 3 files · lifecycle: beforeRun admin (cached, 3h left) · beforeAll catalog · beforeEach ×9`), to a tree for `vc plan`, and to DOT for `vc plan --dot`.

Property tests (`proptest` over generated programs): every instance's needs are planned; no lifecycle block is planned for a run or file without a selected instance; selection is idempotent; selecting by file equals selecting its test IDs.

## Executor

The executor runs a `Plan`. It is where concurrency, lifecycle blocks, shared values, cleanup and cancellation come together, so it is designed to need no locks beyond the redactor's `arc-swap` (D113): every piece of changing state has exactly one owner task, and shared values are slots created up front and filled once.

### Tasks and who owns what

| Task | One per | Owns | Talks to |
| --- | --- | --- | --- |
| Run orchestrator | run | the run frame: shared `beforeRun` slots, the run's cleanup list, the run status | file orchestrators (spawns them), the event bus |
| File orchestrator | test file in the plan | the file frame: `beforeAll` slots, the file's cleanup list | instance workflows (spawns them under the concurrency limit) |
| Instance workflow | test instance | the instance frame: saved values, its cleanup stack, check results | transport, auth, hooks (all through interface traits) |
| Event writer tasks | subscriber | their own buffers | history store, reporters, console, UI stream |
| Hook thread | process | the QuickJS runtime | instance workflows, via a request queue |

This section details the workers and frames under the orchestrators described in Orchestration and design patterns. The run orchestrator runs the planned `beforeRun` actions to completion, and each file orchestrator runs its `beforeAll` before any of its instances start (D122); file orchestrators start in plan order; instances from different files run side by side, limited by `defaults.concurrency` (a semaphore) and by `defaults.perHost` (a semaphore per host inside the transport).

### Shared values without locks

The planner lists every shared action, so each frame creates its slots up front and fills them before any instance starts (D122):

```rust
/// One per planned shared action (beforeRun or beforeAll), created up front.
/// The frame owner runs the action once; every instance reads the same result.
pub struct SharedSlot {
    action: ActionId,
    cell: tokio::sync::OnceCell<Result<Arc<SharedOutcome>, Arc<Failure>>>,
}

pub struct SharedOutcome {
    pub saved: Saved,                  // values published under the action's id
    pub cleanups: Vec<PendingCleanup>, // registered by its calls, run when the owner frame closes
    pub finished_seq: u64,             // order of completion, so cleanups run newest first
    pub from_cache: bool,
}
```

`OnceCell` makes each slot write-once, so it needs no lock. The slots live in a read-only map shared by `Arc`; a failed action stores its failure, and every instance whose needs include it is marked **blocked** with that failure attached; blocked instances are never retried (D126). Cleanups are returned as data and run by the frame's owner when it closes, so no task ever writes into another task's state.

### One test instance

```text
run_instance(instance):                           # repeated per attempt when the test has retry (D126)
  frame = InstanceFrame::new(instance, scope = run saves + file saves (read-only) + file vars + …)
  for action in file.beforeEach ++ test.before:    run_action(action, frame)   # on failure: blocked, skip to after
  if test is Call: run_action(call, frame)
  else for step in test.steps:                     run_action(step, frame)     # test-level keys are step defaults (D124); stop at the first failure
  apply test-level save to the last step's response (D124)
  for action in test.after ++ file.afterEach:      run_action(action, frame)   # always runs
  run frame.cleanups newest first                                              # always runs, own timeout, also between attempts
  emit TestFinished(status, checks, timings)                                   # passed after a failed attempt = flaky
```

`run_action` dispatches on the action kind:

- **Call.** Resolve the effective auth (step, then api, then api file); render the request (method, URL from the api file's `baseUrl`, headers, query, body) against the scope; run `beforeRequest` hooks; send through the transport; run `afterResponse` hooks; evaluate the merged checks; if `poll` is set, repeat until `until` holds or its timeout passes; apply `save` (api saves, then the action's); if the status is 2xx, the api has `cleanup` and the action isn't `keep`, render the cleanup's vars against this response and push it onto the owner's cleanup stack.
- **Test** (`test:` reuse). Run the reused test's calls in order inside the current frame, with each api's checks; the reused test's own test-level checks are skipped.
- **Function / command.** Send the call to the hook thread or spawn the process through the `CommandRunner` trait (with a timeout and the run's environment); a non-zero exit (V5110) or a thrown error fails the action; a returned object becomes saved values. A dynamically saved value that a later template uses without a `provides:` declaration is V5108; a declared one the action did not return is V5109 (D128). The `function:` executor is registered in M4; until then the planner rejects `function:` actions.
- **Loops.** `repeat` and `forEach` expand into iterations run in a `JoinSet` under a semaphore of size `parallel` (default 1). Each iteration writes its saved values into slot `i` of a pre-sized list, so lists come out in iteration order however the iterations finish. On failure, the action waits for the iterations already running, keeps their cleanups, and reports the first failure.
- **Retry.** A test's `retry` (or `defaults.retry`) re-runs the whole instance after `delay`, running its cleanups between attempts; a pass after a failure marks it **flaky** with every attempt recorded. A blocked instance is never retried. Transport-level retries are `http.retry`, in the request pipeline (D126).
- **Cache.** If the action has `cache`, look in the cache store first (see Results, history store, cache store); a live hit restores saved values and registers no cleanup.

### Cancellation and timeouts

A tree of `CancellationToken`s mirrors the frames (run → file → instance). Ctrl-C, `--bail` or a test timeout cancels the relevant subtree: no new requests start, in-flight requests get a grace period (default 5 s), then cleanups run under their own token and timeout so they are not cancelled with the work. A second Ctrl-C skips the grace period but still runs cleanups; a third exits immediately and lists the cleanups that did not run.

### Events

Everything observable is an `Event` on a bounded channel with a run-wide sequence number:

```rust
pub enum Event {
    RunStarted { plan: PlanSummary }, FileStarted { file: FileId },
    ActionStarted { action: ActionId, owner: OwnerKey }, ActionFinished { action: ActionId, outcome: ActionOutcome },
    ExchangeCompleted { exchange: ExchangeRecord },   // request, response, timings, redacted
    CheckEvaluated { check: CheckResult },             // expected, actual, pass, span
    CleanupFinished { cleanup: CleanupRecord },
    TestFinished { result: TestResult },               // passed | failed | blocked | skipped | flaky
    RunFinished { totals: Totals },
}
```

Subscribers (history writer, reporters, console, TUI, web UI, MCP) each get their own task fed by a fan-out task, so a slow subscriber is buffered and cannot slow the run; if a buffer overflows, that subscriber is told how many events it missed and reloads from the history store. Every event passes through the redactor before it leaves the executor; the redactor is executor logic, and the secrets adapter only supplies values (D106). `Event` is a `vc-model` type (D103).

### Determinism

The executor reaches time, randomness, the file system and processes only through the `Clock`, `Random`, `FileSystem` and `CommandRunner` traits (implemented in `vc-adapter-system`, D133). `--seed` seeds `Random` (fake data, `random`/`shuffle` pipes, dataset picks), so a run can be repeated exactly; the seed is printed on failure (D131), and tests use the `vc-testkit` fakes.

## Vars, templates and CEL at run time

Templates were parsed at compile time; at run time they are only rendered against a scope. Rendering never runs arbitrary code: references are looked up, pipes are applied from a registry, and `$cel` values are evaluated by the sandboxed CEL interpreter.

### The scope

```rust
/// The levels a ${var.x} lookup walks, closest first. Each level is a cheap shared map.
pub struct Scope {
    levels: SmallVec<[Level; 10]>,
}
pub enum Level {
    Call(Arc<Values>),          // vars on this call or action
    Saved(Arc<Values>),         // saved earlier in this test instance (beforeEach, before, steps)
    File(Arc<Values>),          // beforeAll saves, published under action ids
    TestFile(Arc<Values>),      // the test file's vars, with profile overrides applied
    Run(Arc<Values>),           // beforeRun saves, under action ids
    Api(Arc<Values>),           // the api's defaults
    ApiFile(Arc<Values>),       // the api file's vars
    Config(Arc<Values>),        // config vars with active profiles applied in order
}
```

Config values are layered once, lowest first: built-in defaults < user config < `vibranium.config.yaml` < profiles in the order given < `.env` < `VIB_*` variables < `--set`. Each layer is a `ConfigSource`, and every value keeps its origin, so `vc config explain` can say where it came from (D129). Profile overrides are applied when the program is loaded for a run, not on every lookup: for each level that has `profiles`, the active profiles' values are layered over the base values once. A lookup walks the levels and returns the first hit; the linker has already proven every reference resolves for the active profiles, so a miss at run time is an internal error, not a user error.

### Rendering

- **Type preservation.** A value that is exactly one template keeps its type (number, list, object); a template inside other text renders to a string. This is decided at compile time and stored in the template.
- **Fake data memo.** A var defined as `${fake.…}` is generated once per instance (or per loop iteration) and then reused, so one value can appear in several places; an inline `${fake.…}` is fresh each time it is rendered (D131).
- **Paths.** After the namespace, paths walk JSON with `.key`, `[n]` (negative from the end), `[*]` (map over a list) and quoted keys; a missing path is a run-time render failure of that action (V5111), with the path and the actual value shown.
- **Pipes.** Registered functions `fn(Value, &[Arg]) -> Result<Value>`: `number`, `int`, `bool`, `string`, `json`, `jsonString`, `base64`, `fromBase64`, `lower`, `upper`, `trim`, `urlEncode`, `default`, `random`, `shuffle`, `take`. A plugin can register more under a prefix.
- **`$extend` and `$remove`.** The base value is rendered, the changes are deep-merged into it (maps merge, lists and scalars replace, `null` sets null), then the `$remove` paths are deleted.
- **Secrets.** `${secret.X}` renders the real value into the request, and the value is registered with the redactor the moment it is first resolved, so it is masked in every event and output.

### CEL

`cel-interpreter` is wrapped behind a small trait so it can be replaced:

```rust
pub trait Expr: Send + Sync {
    fn eval(&self, activation: &Activation) -> Result<Value, ExprError>;
}
// Activation roots (D125): response (status, headers, body, timing), request, var (D53), env, item,
// iteration, example, and each step id. Inside `expect`, bare status/headers/body alias response.*.
// A root not available at that spot is a compile error (V2105).
```

Expressions are compiled and type-checked once by the compiler. At run time each evaluation gets a fresh activation built from the response and scope. Two additions to the standard library are registered: `sum()` on lists of numbers and `matches()` with the same regex engine as the `regex` matcher. Evaluation has a step limit, so a pathological expression fails instead of hanging.

## HTTP transport, auth and secrets

### Transport

```rust
#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, request: RenderedRequest, ctx: &SendContext) -> Result<Exchange, TransportError>;
}

pub struct Exchange {
    pub request: RenderedRequest,          // as sent, after hooks and auth
    pub response: Response,                // status, headers, body bytes (capped), size
    pub timing: Timing,                    // dns, connect, tls, first_byte, download, total
    pub connection: ConnectionInfo,        // reused or new, protocol (h1, h2), remote address
}
```

The HTTP adapter is built on `hyper` 1 with `hyper-util`'s pooled client and a custom connector that times each phase: DNS through `hickory-resolver` (with the system resolver as fallback), TCP connect, the `rustls` handshake (platform certificate verifier, so corporate CAs work), then first byte and download measured around the body stream. HTTP/2 is negotiated by ALPN. A semaphore per host enforces `defaults.perHost`. Response bodies are read up to a cap (`history.maxBodySize` for storage, a much larger in-memory cap for checks) and decompressed (gzip, br, zstd). Proxy settings follow `HTTPS_PROXY`/`NO_PROXY`; client certificates and custom CA bundles are per-profile config.

Around the transport, each request passes through `tower` layers in this order (D134, D141): timing → retry (on `refreshOn` and per `http.retry`, D143) → auth → `beforeRequest` hooks → per-host limit → timeout → transport. Auth sits inside retry, so a retried request gets fresh credentials. The timeout sits inside retry, so it limits each attempt and a timed-out attempt can be retried. Until M3 and M4 the auth and hook layers are null objects.

### Auth

```rust
#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// Returns what to add to the request: headers, query params or cookies.
    async fn credentials(&self, key: &AuthKey, ctx: &AuthContext) -> Result<Arc<Credentials>, AuthError>;
    /// Called when a response matches refreshOn (default 401): drop the cached credentials.
    fn invalidate(&self, key: &AuthKey);
}
// AuthKey = (AuthId, hash of the resolved vars, active profiles): one login per distinct identity.
```

One `TokenCache` sits in front of every provider: a map from `AuthKey` to a slot like the executor's shared slots, so concurrent requests needing the same identity wait for one login. It is one of the two places allowed a lock: a mutex over the map of slots, never held across an `await` (D113). A slot records an expiry; credentials are refreshed `refreshBefore` the expiry by the next caller. On a `refreshOn` status the request is retried once with fresh credentials. With `cache: true`, credentials are also written to the cache store, encrypted, and reused across runs until they expire.

Providers by type: `basic`, `bearer`, `apiKey` (pure functions of their vars); `oauth2ClientCredentials` and `oauth2Refresh` (token endpoint calls through the same transport, so their timings show in reports); `oauth2AuthorizationCode` (opens the browser once, listens on `redirectPort`, stores the refresh token in the OS keychain); `flow` (runs its steps in a private frame through the `ActionRunner` trait, which the executor implements and `EngineBuilder` injects, then renders `apply`) (D106); `plugin` (delegates over the plugin protocol).

### Secrets and redaction

```rust
#[async_trait]
pub trait SecretProvider: Send + Sync {
    async fn resolve(&self, name: &SecretName, profile: &ProfileId) -> Result<Secret, SecretError>;
    async fn status(&self, profile: &ProfileId) -> Result<Vec<SecretStatus>, SecretError>; // for Settings: never values
}
```

Two providers exist. `provider: varlock` runs Varlock: on the first secret request of a run it calls `varlock load --format json` once (with the active profile as the environment), keeps the values in memory for the run only, and never writes them to disk. `provider: env` reads environment variables. Varlock is the current default, as the syntax reference and `vc init` show; the M3 kick-off confirms or changes it (see decision-log Open). `Secret` holds its value in a type that does not implement `Debug` or `Serialize` and zeroes its memory on drop, so it cannot be logged or stored by accident.

The redactor is core logic in `vc-executor`; secret providers only supply values (D106). It is one `aho-corasick` automaton rebuilt whenever a secret is resolved or a token is issued and swapped in with `arc-swap`, so readers never wait (D113). It also masks the `Authorization`, `Cookie` and `Set-Cookie` headers and any field named in `redact`. It runs on every event before it leaves the executor, on plugin responses, and on MCP results. A redacted value is shown as `•••• (secret SHOP_CLIENT_SECRET)` so the reader knows what was there.

## Hook bridge: TypeScript in QuickJS

Hooks, lifecycle `function:` actions and custom checks are TypeScript or JavaScript functions in the project's `hooks/` folder. They run inside the `vc` binary in an embedded QuickJS engine, sandboxed, with no Node or Bun needed. Other languages run through the plugin protocol instead.

### Loading

1. **Type checking is the user's editor's and CI's job.** `@vibranium/sdk` is a types-only npm package (`BeforeRequestHook`, `AfterResponseHook`, `CustomCheck`, `LifecycleFunction`, the request and response shapes), so `tsc --noEmit` in the project checks hooks. Vibranium itself never type-checks.
2. **Strip and bundle at compile time.** For each referenced file, `oxc` parses it, strips types (including `import type`), resolves relative imports between hook files into one module per entry, and produces JavaScript plus a source map. The result is cached by content hash in `.vibranium/cache/hooks/`.
3. **Allowed imports.** Relative hook files, `@vibranium/sdk` (types only, removed by stripping) and a small built-in module `vibranium:std` (crypto HMAC and hashes, base64, URL and query helpers, time formatting). Anything else, including Node built-ins such as `node:crypto`, is a compile error with a suggestion; npm packages can be used by bundling them into the hooks folder with the user's own bundler.

### Running

```rust
/// Owned by one dedicated OS thread; everything else sends it requests through a queue.
pub struct HookEngine { requests: mpsc::Sender<HookRequest> }

pub struct HookRequest {
    pub hook: HookRef,                     // module + export name
    pub input: serde_json::Value,          // the request, response, vars … as JSON
    pub budget: Budget,                    // time (default 200 ms), memory (default 64 MB)
    pub reply: oneshot::Sender<Result<serde_json::Value, HookError>>,
}
```

The hook thread owns one QuickJS runtime with a context per hook module, created on first use. Each call converts the input JSON into a JS value, calls the export, awaits it if it returns a promise (the QuickJS job queue is drained on the same thread), and converts the result back. An interrupt handler stops a call that runs past its time budget; the memory limit is enforced by the runtime. There is no file, network, process or environment access: the only capabilities are the `vibranium:std` functions and a `secret(name)` function that returns secrets declared in .env.schema (each use is logged, and the value is redacted from everything the hook returns). Errors are mapped back through the source map, so a thrown error points to the TypeScript line.

Measured in the spike: 4.2 MB added to the binary, 3.2 ms to the first call, about 20 µs for a `beforeRequest` call and 36 µs for a custom check. JSON conversion is most of that cost, so inputs are kept small (bodies are passed lazily through a `body()` accessor). One thread serves all hooks; under load, `hooks.threads` can start more (each with its own runtime) and requests are spread across them.

Known limit from the spike: a single long built-in call, such as a pathological regular expression, can run past the time budget before the interrupt is checked. The budget is enforced again from outside by a watchdog that, if a call overruns by more than 1 second, marks the hook thread unusable, fails the call, and starts a fresh thread.

## Results, history store, cache store and reports

### History store (`.vibranium/history.db`)

SQLite in WAL mode through `rusqlite` (bundled, no system library). One writer task owns the connection, receives events, and commits in batches (every 200 ms or 500 events); readers (`vc history`, TUI, web UI, MCP, report generation) open their own read-only connections, which WAL lets run alongside the writer. A second `vc` process writing to the same file waits on SQLite's busy timeout (5 s) and then reports which process holds it.

```sql
CREATE TABLE runs (
  id INTEGER PRIMARY KEY, started_at INTEGER NOT NULL, finished_at INTEGER,
  profiles TEXT NOT NULL, selector TEXT NOT NULL, git_commit TEXT, git_branch TEXT,
  seed INTEGER, vc_version TEXT NOT NULL, config_hash TEXT NOT NULL,
  passed INTEGER, failed INTEGER, blocked INTEGER, skipped INTEGER, flaky INTEGER, kind TEXT NOT NULL -- run | load | adhoc | agent
);
CREATE TABLE test_results (
  id INTEGER PRIMARY KEY, run_id INTEGER NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
  test_id TEXT NOT NULL, example_row INTEGER, repeat_index INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL, duration_ms REAL NOT NULL, attempts INTEGER NOT NULL DEFAULT 1,
  blocked_by TEXT, skip_reason TEXT, skip_until TEXT
);
CREATE TABLE checks (
  test_result_id INTEGER NOT NULL REFERENCES test_results(id) ON DELETE CASCADE,
  kind TEXT NOT NULL, path TEXT, expected TEXT, actual TEXT, passed INTEGER NOT NULL,
  origin TEXT NOT NULL, file TEXT NOT NULL, line INTEGER NOT NULL
);
CREATE TABLE exchanges (
  id INTEGER PRIMARY KEY, test_result_id INTEGER REFERENCES test_results(id) ON DELETE CASCADE,
  run_id INTEGER NOT NULL, action TEXT NOT NULL, api_id TEXT, method TEXT NOT NULL, url TEXT NOT NULL,
  status INTEGER, dns_ms REAL, connect_ms REAL, tls_ms REAL, first_byte_ms REAL, download_ms REAL, total_ms REAL,
  request_headers TEXT, response_headers TEXT, request_body BLOB, response_body BLOB, body_truncated INTEGER
);
CREATE TABLE load_intervals (
  run_id INTEGER NOT NULL, test_id TEXT NOT NULL, second INTEGER NOT NULL,
  count INTEGER NOT NULL, errors INTEGER NOT NULL, histogram BLOB NOT NULL  -- HdrHistogram V2 encoding
);
CREATE TABLE agent_actions (
  id INTEGER PRIMARY KEY, at INTEGER NOT NULL, agent TEXT NOT NULL, tool TEXT NOT NULL,
  profile TEXT, arguments TEXT NOT NULL, result TEXT NOT NULL, approved_by TEXT
);
CREATE INDEX test_results_by_test ON test_results(test_id, run_id);
CREATE INDEX runs_by_time ON runs(started_at);
```

Bodies are stored per `history.bodies` (default: failures only), cut at `history.maxBodySize` and compressed with zstd. Retention (`history.retain`) deletes old runs at the start of a run. Schema changes go through numbered migrations stored in the file. The same `HistoryStore` trait has a Postgres adapter for the self-hosted server.

### Cache store (`.vibranium/cache.db`)

SQLite, one table:

```sql
CREATE TABLE entries (
  key BLOB PRIMARY KEY,            -- blake3(api or test id, canonical resolved vars, profiles)
  created_at INTEGER NOT NULL, expires_at INTEGER NOT NULL,
  saved BLOB NOT NULL,             -- the saved values, ChaCha20-Poly1305 encrypted
  cleanups BLOB NOT NULL           -- pending cleanups, encrypted
);
```

The encryption key is created on first use and stored in the OS keychain (`keyring`); where there is no keychain (most CI), `VIB_CACHE_KEY` supplies it, and without either, cached secret-derived values are simply not cached. The canonical vars encoding sorts keys and normalises numbers, and values from `${fake.*}` are excluded from the key so a cached admin user is found again. On a run, an expired entry's cleanups run before the action creates a fresh value; `vc cache clear` runs all cleanups and empties the table.

### Results and reports

CTRF is the canonical result: the CTRF reporter writes `ctrf.json` from the event stream, with Vibranium-specific fields (lifecycle, blocked-by, checks with source lines) under CTRF's `extra`. JUnit and console reporters derive from the same events. All reporters implement one trait and are chosen by name:

```rust
#[async_trait]
pub trait Reporter: Send {
    async fn on_event(&mut self, event: &Event) -> Result<(), ReportError>;
    async fn finish(&mut self, run: &RunSummary) -> Result<Vec<Artifact>, ReportError>;
}
```

The HTML reporter writes the folder layout from the Technical Design: `index.html` plus `data/summary.js`, `data/index.js`, `data/tests/<shard>.js` and `data/bodies/<hash>.js`, each data file a single `VIB.load(key, json)` call so it loads from `file://`. Test details are sharded by test (one file per test, or per 50 instances for very repetitive tests); bodies are deduplicated by BLAKE3 hash. The single-file layout embeds the same data as gzip and base64 per test and inflates it lazily. The report's front end is a separate small TypeScript bundle in `web/report/`, built at release time and embedded in the binary, sharing components with the web UI's run view.

## Import and sync: OpenAPI and HAR

All importers produce the same intermediate form and hand it to `parser::edit` (with the engine's compile guard, D107), so every import respects the naming conventions, writes the documentation header and never reformats existing text.

```rust
#[async_trait]
pub trait Importer: Send + Sync {
    async fn read(&self, source: &ImportSource, opts: &ImportOptions) -> Result<ImportDraft, ImportError>;
}

pub struct ImportDraft {
    pub groups: Vec<DraftGroup>,          // one per target api file (existing or new)
    pub flow: Option<DraftFlow>,          // HAR: the recorded order as steps with chained saves
    pub secrets: Vec<SecretFinding>,      // removed values and where they map to
    pub notes: Vec<Diagnostic>,           // things the user should know (skipped entries, guesses)
}
pub struct DraftApi { pub id: Sym, pub method: Method, pub path: Template, pub vars: VarDefs,
                      pub body: Option<BodySpec>, pub operation_id: Option<String>, pub matches: Option<ApiId> }
```

### OpenAPI

- **Reading.** Fetch the URL (with `source.headers`, rendered against the active profile) or read the file; detect JSON or YAML by content; detect the version from `openapi`/`swagger`. Swagger 2.0 is converted to 3.1 first (a mechanical mapping of `definitions`, `parameters`, `consumes`/`produces`); 3.0 is read with `openapiv3`, 3.1 with `oas3`. External `$ref`s are fetched and bundled.
- **Snapshot.** The bundled 3.1 document is written to `openapi/<fileId>.yaml` in a canonical form (keys sorted the way the spec orders them, YAML block style), so it diffs well.
- **Mapping.** One api per operation (filtered by `source.include` tags or path prefixes); ID from `operationId` in camelCase or from method and path; path parameters become `${var.name}`; query parameters become vars with the spec's defaults; required body fields become vars with generators matched to type and format; the success response's schema becomes a `$file` reference into the snapshot.
- **Three-way sync.** For each api matched by `operationId`, compare the fields sync owns (method, path, parameter vars, request content type) across base (the old snapshot), theirs (the new spec) and mine (the api file). The rule table in the CLI doc decides each field; conflicts produce a diagnostic with `--take spec`, `--take mine` and `--resolved` fixes. The snapshot is updated for every api that merged cleanly; a conflicted api keeps its old snapshot entry until it is resolved, so the next sync sees the same conflict. `--check` runs everything except writing and exits 4 on drift.

### HAR

- **Reading** with the `har` crate (HAR 1.2).
- **Filtering.** Static assets are found by MIME type and extension, third-party hosts by comparison with the hosts the active profile's base-URL vars resolve to, and preflights by method.
- **Host mapping.** Each remaining host maps to the api file whose rendered `baseUrl` matches it; an unknown host becomes a new api file, named by the conventions, with a base-URL var added for the active profile.
- **Path templating.** Path segments that look like identifiers (UUIDs, numeric IDs, prefixed IDs such as `ord_8Kq2Tz`, long hex) become vars named after the preceding segment (`/orders/{x}` → `orderId`). Requests that differ only in those segments are merged into one api.
- **Matching.** A request whose method and templated path equal an existing api is reused, not duplicated.
- **Body vars.** Body fields become api vars, with the recorded values as defaults.
- **Chaining.** Values that appear in a response and later in a request (an ID returned by one call and used in a later URL or body) become `save` entries and `${var.…}` references in the flow, which is how the recorded session replays with fresh data.
- **Secrets.** `Authorization` and cookies are dropped and mapped to the file's auth. Query or body fields with secret-like names, or high-entropy values, become `${secret.NAME}` references, with an entry proposed for `.env.schema`.

cURL, Postman (collection v2.1), Bruno and Vibranium v1 importers use the same draft form, each in its own module.

## Load engine

The load engine reuses the executor's action runner, so a test behaves the same in a load run as in a functional run; only scheduling and recording differ.

- **Model.** A closed model of virtual users. A stage controller adjusts the number of user tasks once per second toward the stage target (linear ramp from the previous stage's count). Each user task loops: pick a test from the mix by weight (a precomputed alias table), run one instance of it with `checks: api` or `full`, then sleep the think time.
- **Lifecycle.** `beforeRun` and each mixed file's `beforeAll` run once before the first stage, through the same shared slots as a functional run; `beforeEach`, `before` and cleanups run inside every iteration and are counted separately from the mix.
- **Measuring.** Each iteration records latency from its scheduled start, not from when the request was actually sent, so a tool that falls behind cannot hide a slow server (coordinated omission). Per test and per api, each second gets an `hdrhistogram` (1 µs to 60 s, 3 significant digits); a recorder task merges them and writes one row per test per second. Errors are counted by status and api, with the first five exchanges of each kind kept as samples.
- **Thresholds.** Thresholds are evaluated every second on a rolling window and at the end. A breach of an `abortOn` metric cancels the run through the same cancellation tree; cleanups still run.
- **Capacity.** The target is 5,000 users on one 8-core machine at under 70% tool CPU. Users are lightweight tokio tasks; connection reuse comes from the transport's pool; event detail drops to aggregates (no per-exchange events) above 50 users unless `--trace` asks otherwise.
- **Live view.** The CLI, TUI and web UI subscribe to a once-per-second summary event (users, rps, p50, p95, p99, errors, threshold state).

## Interfaces

Every interface is a thin adapter over the `engine` facade, so a capability added to the engine appears in all of them. The facade's methods are also the `vibranium.engine.v1` service that the web UI, SDKs and remote agents call.

### The engine API

| Method | Does | Used by |
| --- | --- | --- |
| `Check(paths?, overlay?)` | Compile; return diagnostics. `overlay` supplies unsaved file contents | CLI, editors, web editor on each keystroke, agents |
| `Complete(file, offset, overlay)` | Completions at a position: api IDs, vars in scope with their source, matchers, keys allowed here, `$file` paths, hook exports | web editor, future LSP |
| `Hover(file, offset)` / `Explain(id or code)` | Docs for a key, a resolved test, or a diagnostic code | editors, `vc explain` |
| `Edit(changes)` | Structural edits (`parser::edit`), returning text edits and resulting diagnostics; refuses edits that add errors (the guard is in the engine, D107) | web editor form view, Settings, creators, agents |
| `ApplyFixes(codes or fix IDs)` | The certain fixes as edits | `vc check --fix`, Problems panel |
| `Plan(selection)` / `Run(selection, options)` | Plan; run with a stream of `Event`s | all |
| `Call(api, vars, profile)` | One ad hoc request with checks | API client, `vc call`, agents |
| `Import(source)` / `Sync(files, mode)` | Import drafts and sync, with previews | CLI, TUI, web |
| `TestAuth(auth, profile)` / `CheckSecrets(profile)` | Run an auth definition step by step; secret status without values | Settings |
| `History(query)` / `Report(run, format)` | Query stored runs; write a report | all |
| `Approvals.List/Decide` | Pending agent requests and decisions | web UI, `vc approvals` |

### CLI (`apps/cli`)

`clap` derive commands as listed in the CLI and Developer Workflow doc; global flags `--profile`, `--json`, `--project`, `--log`. Console output is written by the console reporter (spec, dots, json) and the `miette` diagnostic renderer; colour and progress bars switch off when stdout is not a terminal. Exit codes follow that doc (0 pass, 1 failed or blocked, 2 nothing ran, 3 load threshold, 4 sync drift), plus 130 when cancelled and 70 for an internal error, which prints a report hint and writes a redacted debug bundle (D127). `vc-cli` owns `main` and dispatches `vc tui`, `vc mcp` and `vc ui` to their app crates (D110). `vc completion <shell>` generates static completion plus a dynamic hook (`vc __complete`) that lists project IDs.

### TUI (`apps/tui`)

`ratatui` with `crossterm`, one screen per tab (Run, Explore, History, Load, Import). It subscribes to the same event stream as the console reporter and uses `Edit`/`Check` for in-place editing; it never blocks the UI thread on I/O (the engine runs on the tokio runtime and sends redraw messages).

### MCP (`apps/mcp`)

`rmcp` server exposed as `vc mcp` (stdio, started by the agent) and at `/mcp` on the local web server (streamable HTTP). Tools map to engine methods: `list_definitions`, `search`, `get_definition`, `check`, `run_tests`, `explain_failure`, `query_history`, `call_api`, `write_definition`.

Every tool call goes through a policy check, done in the engine so that every route is covered. The check reads the active profile's `agents` rules, and it either:

- allows the call;
- refuses it with the rule that applied;
- or creates an approval request in `history.db` and waits for a decision (default timeout 10 minutes).

Decisions are made in the web UI's AI agents page, or with `vc approvals` / `vc approve <id>` in any terminal. Because both processes share `history.db`, a stdio agent and a separate `vc ui` see the same queue; `notify` on the database file wakes the waiting call. Every call is logged to `agent_actions`, with arguments redacted.

### Web server and web UI (`apps/server`, `web/`)

`vc ui` starts `axum` on `localhost:7420` (loopback only by default; a random token in the opened URL protects it from other local sites). It serves:

- the embedded web UI, built with Vite and embedded with `rust-embed`;
- the engine API over the Connect protocol: unary and server-streaming calls as HTTP POST with JSON or binary protobuf bodies, implemented as a small handler layer over the `prost`/`pbjson` types;
- MCP over HTTP.

The web UI is TypeScript and Svelte (SvelteKit with its static adapter, so it builds to plain files the binary embeds), with clients generated by `@bufbuild/protobuf` and `@connectrpc/connect-web`. Its editor, API client, Settings, run view and report share one component library styled per the design mockups. The self-hosted team server (`vc serve`) is the same binary with Postgres history, accounts (OIDC) and run ingestion added; it is a later release.

### Plugins (`adapters/plugins`)

Third-party plugins are separate processes speaking JSON-RPC 2.0 over stdio, with LSP-style `Content-Length` framing; HTTP services and gRPC (for executors on the hot path) use the same method set.

- **Start-up.** The host starts a plugin on first use, checks its manifest (`vibranium-plugin.yaml`: kind, entry, permissions), and exchanges `initialize` (protocol version, capabilities, config schema).
- **Calls.** The host calls the methods for the plugin's kind: `auth/credentials`, `reporter/onEvent`, `importer/read`, `executor/send`, and so on.
- **Secrets.** Only the secrets the manifest lists are ever sent, and everything a plugin returns passes through the redactor.
- **Network and files.** Permissions are declared and shown at install. Where the OS allows (Landlock on Linux), file and network limits are also enforced on the plugin process.
- **Failures.** A plugin that exceeds its deadline or crashes fails only the work that used it.

## Diagnostic codes

Codes are stable once released: a code is never reused for a different problem, and each has a `vc explain` page with a wrong and a right example (written in `crates/diagnostics/explain/Vxxxx.md`, checked by a test that compiles both examples). The first release ships these; new ones are added at the end of their range.

| Code | Severity | Problem | Certain fix |
| --- | --- | --- | --- |
| V1001 | error | File is not valid YAML or JSON, or its suffix is not a known kind | — |
| V1002 | error | YAML structure error (indentation, a mapping expected) | when one reading is possible |
| V1003 | error | `syntaxVersion` newer than this Vibranium, or missing | — |
| V1004 | error | Unknown key | rename when one known key is close |
| V1005 | error | Required key missing | — |
| V1006 | error | Duplicate key in a map; labels the first one (D149) | — |
| V1007 | error | YAML alias or anchor not supported: an alias (`*x`), including one naming a missing anchor; an anchor alone is ignored (D147, D149) | — |
| V1008 | error | YAML tag not supported (`!name`, `!!str`) (D149) | — |
| V1009 | error | Map key is not text (a list or map) (D149) | — |
| V1010 | error | More than one YAML document in a file (D149) | — |
| V1011 | error | Nesting deeper than 128 levels (D149) | — |
| V2001 | error | Duplicate ID | — |
| V2002 | error | Unknown auth, profile, dataset or generator | close match |
| V2003 | error | Unknown api or test | close match; after a file move, the moved file's new ID (D139) |
| V2004 | error | `$file` or snapshot reference not found | close path |
| V2005 | error | Hook export not found | close export |
| V2006 | error | Reserved ID (D137) | — |
| V2007 | error | File ID doesn't match its path (D139) | rename the ID and its references |
| V2101 | error | Var used but never set for some profile | close var name |
| V2102 | error | `${item}` outside `forEach`, `${example}` without `examples`, `${iteration}` outside a loop (D137) | — |
| V2103 | error | Unknown example or dataset column | close column |
| V2105 | error | Template or CEL root not available at that spot (D125, D137) | — |
| V2201 | error | Exactly one of `call` or `steps` needed | — |
| V2205 | error | Cycle through `test:` reuse | — |
| V2210 | error | Shared action saves values but has no `id` | add an id derived from the api |
| V2301 | error | Invalid duration or size (`500` instead of `500ms`) | add the unit when unambiguous |
| V2302 | error | Wrong value type (`"3"` for an integer) | convert |
| V2303 | error | Invalid status code or class | — |
| V2304 | error | Pipe or path cannot apply to that value's type | — |
| V2305 | error | CEL does not parse or type-check | — |
| V2306 | error | Matcher argument invalid (bad regex, `between` with one value) | — |
| V3101 | error | No profile defined, or no active profile for this run | add `default: true` to the only profile |
| V3102 | error | More than one default profile | — |
| V3103 | error | `skip` without a reason | — |
| V3104 | warning | Skip expired; the test runs again | remove the skip |
| V3201 | warning | `PUT`, `PATCH` or `DELETE` uses a shared value from `beforeRun` or `beforeAll` | — |
| V3202 | warning | `cache` on a per-test action | — |
| V3203 | warning | Call to a `deprecated` api | — |
| V3204 | warning | Unquoted text value that YAML cut at `  # ` (a comment) | — (suggested: quote it) |
| V3205 | warning | A test-level `body` default reaches a GET or HEAD step (D124) | — |
| V3206 | warning | A test-level default that no step uses (D124) | — |
| V4001 | warning | Test ID breaks the convention | rename from the name, with references |
| V4002 | warning | Api ID breaks the convention (case, service prefix) | rename with references |
| V4003 | warning | File name or folder breaks the convention | — (`vc rename` moves it) |
| V4004 | warning | Tag, profile or dataset name breaks the convention | rename with references |
| V5101 | failure | Expectation failed | — |
| V5102 | failure | CEL check false | — |
| V5103 | failure | Custom check failed or threw | — |
| V5104 | failure | Timeout (request, poll or test) | — |
| V5105 | failure | Transport error (DNS, connect, TLS, reset) | — |
| V5106 | blocked | A lifecycle action this test needs failed | — |
| V5107 | warning | Cleanup failed (reported, results unchanged) | — |
| V5108 | failure | A dynamically saved value is used but its `function:` or `test:` does not declare it in `provides` (D128) | — |
| V5109 | failure | A value declared in `provides` was not saved (D128) | — |
| V5110 | failure | Command exited non-zero | — |
| V5111 | failure | Template failed to render at run time (missing path, wrong type) | — |

V3204 is new: the skip-reason example that silently lost `see #212` showed that an unquoted `  # ` inside a value is an easy mistake, and the parser can see it (a comment that starts mid-value on a line whose value looks cut). The parser flags a one-line plain value that resolves to text, followed by `  #` when the `#` is directly followed by a non-space (`#212`) or the value ends with a comma; typed values (`retries: 3 #max`, `enabled: true #flag`) are never flagged. The fix double-quotes the value and the comment together; it is a suggestion, not a certain fix, because the `#` may really start a comment.

## Testing, budgets, build and release

### Tests

| Layer | How | Where |
| --- | --- | --- |
| Pure logic (templates, pipes, matchers, var lookup, diff, edits) | Unit tests next to the code | each crate |
| Compiler diagnostics | Golden files: a folder of small projects, each with the expected `vc check` output (`insta` snapshots, text and JSON) | `crates/compiler/tests/cases/` |
| `parser::edit` | Round trips: apply an edit, re-parse, compare with the expected file byte for byte; comments and formatting elsewhere unchanged | `crates/parser/tests/` |
| Planner and executor invariants | `proptest` over generated programs with a fake transport: shared actions run at most once per key; every registered cleanup runs exactly once, newest first, also on cancellation; loops keep order under parallelism | `crates/executor/tests/` |
| Adapters | One contract suite per interface trait that every adapter and test fake must pass (a transport suite, a store suite, …) (D112) | `crates/testkit` |
| End to end | `vc` run with `assert_cmd` against the in-repo mock server (`vc-mock-server`, started in-process on port 0 so each test has its own), whose request counter proves the lifecycle guarantees (50 tests sharing a `beforeAll` send 1 request) (D100) | `bench/mock/`, `tests/e2e/` |
| Interfaces | CLI output snapshots; TUI rendered to a test backend; web UI with Playwright against `vc ui` and the mock server | `apps/*/tests/`, `web/tests/` |
| Docs | Every example YAML in the docs is extracted and compiled in CI | `docs/tests/` |
| Public API | `cargo public-api` snapshots for `vc-model`, `vc-interfaces` and `vc-engine`, from the end of M2 (D117) | each crate |

### Performance budgets

Budgets are enforced in two ways (D120): instruction-count benches (`gungraun`) run on every PR and fail at a regression over 5%; wall-clock benches (`criterion`) run nightly against the targets below and open an issue on a breach.

| Budget | Target |
| --- | --- |
| Compile and link 1,000 definition files | under 500 ms cold, under 100 ms warm |
| Overhead per request outside the network (render, checks, events) | under 50 µs at p50 |
| Hook call | under 50 µs at p50 for a `beforeRequest` hook |
| History write | 10,000 test results committed in under 1 s |
| HTML report for 10,000 tests | first paint under 1 s, any test opened in under 100 ms |
| Load | 5,000 virtual users on 8 cores at under 70% tool CPU |
| Binary size | under 30 MB per platform |
| Start-up | `vc --version` under 20 ms; `vc check` on the sample under 100 ms |

### Build and release

- **Toolchain.** Rust 1.99 pinned through rustup and `rust-toolchain.toml` (D98); other tools pinned in `mise.toml` (D99); `cargo build --release` with thin LTO, 4 codegen units and debug info stripped; symbol names are kept so the exit-70 debug bundle has readable backtraces (D127). The web UI and report front ends are built with Vite first and embedded.
- **Targets.** macOS (arm64, x64), Linux (x64, arm64, statically linked with musl), Windows (x64).
- **Distribution** through `cargo-dist`: GitHub releases with checksums and build provenance, a one-line installer (`curl -fsSL https://…/install.sh | sh` and a PowerShell equivalent), a Homebrew tap, a Docker image, and an npm package `@vibranium/cli` that downloads the right binary (for JavaScript projects that want it in `devDependencies`).
- **Command names.** The installer adds `vibranium` and `vc`, and warns if another `vc` is already on `PATH`.
- **CI.** Every PR runs `gates.sh` and the instruction-count budgets on Ubuntu, and builds and runs the tests (including e2e) on macOS and Windows; a nightly job runs full mutation testing and the wall-clock budgets (D119, D120). PRs merge into `master` (D97, D118); releases are cut from tags by `cargo-dist`'s workflow.

## Implementation plan

Milestones are sized for a coding agent to take one at a time, each ending in something runnable and checked by the acceptance tests listed. Each milestone's PRs must pass `gates.sh`. The release column maps them to the PRD's release plan.

| # | Milestone | Delivers | Done when | Release |
| --- | --- | --- | --- | --- |
| M0 | Workspace and gates | Crate skeletons for M0–M2 with the layer rules (including `vc-testkit` and a `vc-adapter-system` skeleton), `gates.sh`, `arch-tests`, the gates self-test (D121), CI on three OSes, CODEOWNERS and branch protection (D118), the mock server in `bench/mock`, `rust-toolchain.toml`, `mise.toml` | CI is green on all three OSes; `gates-selftest.sh` shows every bad patch rejected by its gate; branch protection is active; `vc --version` works | — |
| M1 | Read and check | `parser` (marked YAML/JSON), file shapes for all kinds, `template` (templates, pipes, CEL), a pure compiler fed `SourceFile` values that the engine reads through its `SourceProvider`, returning a `CompileCache` (D132), the linker, diagnostics V1xxx–V3xxx, `vc check` (exit 0, 2 or 70), `vc schema export` | The diagnostics golden suite passes; the compile budget holds on a generated 1,000-file project | v2.0 |
| M2 | Run | Planner, executor (calls, steps, lifecycle blocks, shared slots, cleanup, loops, retry, poll, cancellation), HTTP transport with timings, matchers and checks, vars and profiles with config layering (D129), console, CTRF and JUnit reporters, `vc run` selectors including `--changed`, `vc plan`, `vc list`, exit codes with 130 and 70 (D127, D130) | The e2e suite passes against the mock server, including "50 tests, 1 setup request" and cleanup on Ctrl-C; the executor property tests pass | v2.0 |
| M3 | Identity and memory | Auth providers and token cache, Varlock secrets, redactor (body and value masking; headers since M2), history store, cache store, `vc history`, `--failed` (D130), the HTML report (folder and single), `vc report` | Secrets never appear in any output in the redaction test suite; a 10,000-test report meets its budget | v2.0 |
| M4 | Code and data | Hook bridge (oxc + QuickJS), custom checks, `function:` actions, generators, datasets, `$extend`, `$cel` | Hook budgets met; a hook overrunning its time or memory is stopped and reported with the TypeScript line | v2.0 |
| M5 | Authoring | `parser::edit` with the engine's compile guard (D107), `vc init` with the sample and `vc demo`, `vc new` (including `new auth` and `new file`), `vc rename`, `vc fmt`, `vc check --fix`, `vc explain`, `vc skip` and `unskip`, `vc call`, `vc config explain`, `vc watch`, shell completion, V4xxx lints (D130) | Edit round-trip tests pass byte for byte; a fresh `vc init && vc run` passes offline | v2.0 |
| M6 | Import and sync | OpenAPI (Swagger 2.0, 3.0, 3.1) import and three-way sync with snapshots, HAR, cURL, Postman, Bruno, `vc import http` (D130), the v1 migrator | Sync passes its rule-table tests; a recorded HAR replays green against the mock server | v2.0 |
| M7 | Agents and load | MCP server, agent policy and approvals (`vc approvals`), the load engine, `vc load` | An agent session in the e2e suite needs approval exactly where the policy says; 5,000 users within the CPU budget | v2.1 |
| M8 | Terminal UI and plugins | `vc tui` (Run, Explore, History, Load, Import), the plugin host with permissions, the remaining auth types and notifiers | TUI snapshot tests; a sample Python reporter plugin works and is refused a secret it did not declare | v2.2 |
| M9 | Web UI | The engine API over Connect, `vc ui`, the web UI (tests, API client with flows and HAR import, editor, runs, insights, AI agents, Settings) | Playwright suite over every screen in the mockups; editor round-trips keep files byte-identical | v2.3 |
| M10 | Team server | `vc serve` with Postgres history, accounts (OIDC), run ingestion from CI | Two CI jobs publish to one server and the web UI shows both | v2.3+ |

**How an agent works on a milestone.** It starts from this document, the Syntax Reference and the CLI doc, writes the acceptance tests first, implements in small PRs (each under about 400 changed lines) that pass `gates.sh`, and updates the docs it touched in the same PR. Anything that would change a decision in the Decision Log is raised as a question instead of decided in code.

## Open questions

None of these block M0–M2; each has a default the plan assumes until decided.

| Question | Default assumed | Decide by |
| --- | --- | --- |
| Varlock's exact CLI contract for loading values as JSON per environment (flag names, exit codes) | Call `varlock load` with JSON output once per run; wrap it behind `SecretProvider` so a change stays in one adapter | M3, after testing against the current Varlock release |
| `cel-interpreter` gaps (macros, timestamps, error positions) compared with cel-go | Use it behind the `Expr` trait; add missing functions locally; contribute fixes upstream | M1 |
| Faker catalogue parity: the Rust `fake` crate's generators versus the Faker JS names used in templates | A mapping table from Faker JS names to `fake` generators; unmapped names are a V2002 error listing what exists | M4 |
| Web UI framework | Decided by Sarath: Svelte with SvelteKit (static adapter), built by Vite | M9 kickoff |
| Where the documentation site and JSON Schemas are hosted | The GitHub repository (raw URLs) until a domain exists; SchemaStore entries point there | v2.0 release |
| Windows file-system enforcement for plugin permissions | Declared and shown only; enforced on Linux (Landlock) | M8 |
| npm scope `@vibranium` availability | Claim it before v2.0; fall back to `@vibranium-cli` | v2.0 release |
