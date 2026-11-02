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

Versions named below were current on 23 October 2026 and are pinned in `Cargo.lock`; the Rust toolchain is pinned in `rust-toolchain.toml` (1.97, edition 2024). The licence is Apache-2.0 (D35); the feasibility spike's MIT header does not carry over.

## Workspace and crates

One Cargo workspace builds one binary, `vc` (installed also as `vibranium`), which contains the CLI, terminal UI, MCP server and local web server. Code is split into small crates so each layer can be tested alone and the dependency direction can be checked by machine. Apps sit on the outside, the `engine` facade in the middle, pure logic underneath, and adapters plug into the ports.

```
vibranium/
  Cargo.toml                 workspace, shared lints, profiles
  rust-toolchain.toml        1.97
  proto/                     vibranium.{ir,engine,plugin,results}.v1 .proto files
  crates/
    ir/                      IDs, spans, the compiled program (pure data)
    diag/                    Diagnostic, codes, fixes, rendering
    syntax/                  YAML/JSON parsing with source spans
    template/                templates, pipes, CEL wrapper
    compiler/                discover → parse → validate → link → Program
    planner/                 selection → Plan
    ports/                   traits the core depends on (transport, auth, store, …)
    executor/                runs a Plan through the ports; frames, lifecycle, events
    load/                    load engine on top of the executor
    engine/                  facade: compile, plan, run, call, sync, query; the only crate apps import
    adapters/
      http/                  hyper + rustls transport with phase timings
      auth/                  auth providers, flow auth, token cache
      secrets/               Varlock and environment providers, redactor
      hooks/                 QuickJS thread, TypeScript stripping (oxc)
      store/                 history.db and cache.db (SQLite)
      reporters/             CTRF, JUnit, HTML (folder and single), console
      notifiers/             webhook, Slack, Teams, PagerDuty, email
      importers/             OpenAPI, HAR, cURL, Postman, Bruno, v1
      plugins/               JSON-RPC and HTTP plugin host, permissions
    proto/                   generated protobuf types (prost + pbjson)
    apps/
      cli/                   the vc binary: clap commands, console output
      tui/                   ratatui screens
      mcp/                   MCP server (rmcp)
      server/                axum: Connect-protocol API, embedded web UI, MCP over HTTP
    arch-tests/              dependency and design-rule checks
  web/                       web UI: TypeScript, svelte/sveltekit (built, then embedded)
  sdks/                      typescript/ python/ go/ java/
  bench/                     generated projects, mock server, benchmarks
  docs/                      reference docs, JSON Schemas for the file formats
```

Dependency rules, enforced by `arch-tests` in CI:

| Crate | May depend on | Must not depend on |
| --- | --- | --- |
| `ir` | std, serde, small data crates | anything else in the workspace |
| `diag`, `syntax`, `template` | `ir` | each other's internals, ports, adapters |
| `compiler`, `planner` | `ir`, `diag`, `syntax`, `template` | `ports`, `executor`, adapters, apps |
| `ports` | `ir`, `diag` | implementations of any kind |
| `executor`, `load` | `ir`, `ports`, `template` | adapters, apps, the compiler |
| `engine` | everything in the core plus adapters, wired by configuration | apps |
| `adapters/*` | `ir`, `ports`, their own third-party crates | each other, the core logic crates, apps |
| `apps/*` | `engine`, `ir`, `proto` | the core crates and adapters directly |

The web UI and the SDKs never link Rust code: they talk to the engine through the `vibranium.engine.v1` protobuf contract (Connect protocol over HTTP for the web UI, JSON-RPC over stdio or HTTP for plugins).

Third-party crates:

| Need | Crate (version) | Why this one |
| --- | --- | --- |
| Async runtime | `tokio` 1, `tokio-util` 0.7 (cancellation tokens) | The standard; used by hyper, axum, reqwest |
| HTTP client | `hyper` 1 + `hyper-util` 0.1 + `rustls` | Direct control of the connection, so DNS, connect, TLS, first-byte and download times can be measured; `reqwest` 0.13 for simple internal fetches (OpenAPI URLs) |
| YAML with positions | `saphyr` 0.2 | YAML 1.2 with a marked tree (line and column for every node), needed for diagnostics and minimal-diff edits |
| JSON | `serde` 1, `serde_json` 1 | Standard |
| Template and pipe parsing | `winnow` 1 | Small, fast parser combinators with good error positions |
| CEL | `cel-interpreter` 0.10 | The maintained Rust CEL implementation; wrapped so it can be swapped |
| JSON Schema | `jsonschema` 0.60 | Body schema checks and file-format validation |
| Fake data | `fake` 5 | Faker-style generators; names mapped to the Faker JS catalogue used in templates |
| Diagnostics rendering | `miette` 7 | Rustc-style rendering with labels, help and codes; also emits JSON |
| Diffs | `similar` 3 | Line diffs for `--fix`, sync previews and the editor |
| Interning, maps | `lasso` 0.7, `indexmap` 2, `smol_str` 0.3 | Cheap IDs, ordered maps, small strings |
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
| Testing | `insta` 1, `proptest` 1 | Golden files for diagnostics and reports; property tests for the planner |
| Packaging | `cargo-dist` 0.32 | Release binaries, install scripts, Homebrew tap |

## Design rules and quality gates

The feasibility spike showed that Rust's borrow checker stays out of the way when data flows in one direction and shared data never changes. These rules make that the default, and the gates make breaking them fail CI rather than depend on review.

### Rules for the code

1. **The compiled program is read-only.** The compiler produces one `Program`; after that it is wrapped in `Arc` and never changes. Everything that varies during a run (saved values, cleanup stacks, statuses) lives in executor frames owned by one task each.
2. **Refer by ID, not by reference.** IR nodes point at each other with typed IDs (`ApiId`, `TestId`, `ActionId`) looked up in the program's tables, never with Rust references or pointers between nodes. No lifetimes appear in any public API.
3. **Talk through queues.** Long-lived parts (the history writer, the hook engine, reporters, the UI event stream) run as their own tasks and receive messages; nothing else touches their state.
4. **One lock only where shared work is coalesced.** Singleflight maps for shared lifecycle actions and auth tokens are the only shared mutable state; each is one small type with its own tests.
5. **No globals.** No `static mut`, no global registries; dependencies are passed in through constructors (the `engine` crate wires them).
6. **No panics in library code.** `unwrap`, `expect`, `panic!`, `todo!` and `dbg!` are denied by lint; errors are values (`Result`) with a diagnostic code where a user can act on them.
7. **Kinds are registries, not `match` on strings.** Matchers, pipes, auth types, reporters, importers and notifiers register by name in a table built at start-up, so adding one touches one file plus one registry line (the *Refactoring* "switch statements" and "shotgun surgery" smells).
8. **Distinct types for distinct things.** IDs, durations, sizes and file paths are newtypes, not bare strings or numbers.
9. **Small functions.** At most 40 lines, cognitive complexity at most 10, at most 3 parameters (group the rest into a context struct).

### Gates (`scripts/gates.sh`, required in CI)

| Gate | Fails when | Tool |
| --- | --- | --- |
| Lints | Any warning from clippy `pedantic` and `nursery`, plus the denied lints above; `cognitive_complexity` is switched on explicitly because current Clippy leaves it out of pedantic | `cargo clippy --all-targets -- -D warnings` |
| Function size | A function over 40 lines or complexity over 10 | `clippy.toml` thresholds |
| Duplication | A copied block of 10 lines or more | `jscpd` |
| Architecture | A crate imports something the dependency table forbids, or a rule above is broken (e.g. `static` state, an `Arc<Mutex<_>>` outside the allowed types) | `arch-tests` (cargo metadata plus a syntax scan) |
| Dependencies | A licence outside the allow-list, a known vulnerability, a duplicate major version, an unused dependency | `cargo-deny`, `cargo-machete` |
| Formatting | Unformatted code | `cargo fmt --check` |
| Tests | Any failing test | `cargo nextest run --workspace` |
| Mutation score | Below 85% caught mutants in `executor`, `planner` and `template` (weekly job, and on PRs that touch them) | `cargo-mutants` |

Coding agents may not edit `clippy.toml`, the `[workspace.lints]` table, `deny.toml`, `scripts/gates.sh` or `arch-tests`; a CODEOWNERS rule routes those files to Sarath. Smells no tool catches (naming, speculative generality, duplicated logic written differently) are covered by a ten-line review checklist in `CONTRIBUTING.md` that every PR description answers.

## Orchestration and design patterns

The execution workflow is run by an **orchestrator**: one component decides what happens next and holds the workflow's state, while workers only carry out the step they are given and report the result. Nothing in the system chains work by calling the next part directly (choreography), so the whole flow of a run can be read in one place, tested as a state machine, and changed without touching the workers.

&#91;embedded content: orchestration · orchestrators, strategies, events, ports\]

Orchestration is hierarchical, so each level stays small. Orchestrators and workflows decide; strategies and adapters act. All of them depend on interfaces (traits in `ports`), never on each other's concrete types.

### The orchestrators and their state machines

| Level | Owns | States | Decides |
| --- | --- | --- | --- |
| `RunOrchestrator` (one per run) | the run's state, the shared `beforeRun` slots, the root cancellation token, the run cleanup list | `Planned → Preparing → Running → Draining → CleaningUp → Reporting → Finished(Passed \| Failed \| Cancelled)` | when files start, when to stop (bail, Ctrl-C, abortOn), the final status |
| `FileOrchestrator` (one per test file) | `beforeAll` slots, the file's cleanup list, the file's instance queue | `Waiting → Preparing → Running → CleaningUp → Done` | which instance starts next (under the concurrency limit), when `afterAll` runs |
| `InstanceWorkflow` (one per test instance) | the instance frame: saved values, cleanup stack, results | `Pending → AwaitingShared → Before → Body → After → Cleanup → Done(Passed \| Failed \| Blocked \| Flaky)` | retries, stop on the first failed step, blocked when a shared action failed |

Each state machine is an `enum` of states and a single `transition(state, input) -> (state, commands)` function. It is pure: no I/O, no clock. The orchestrator task feeds it inputs (results from workers, timer ticks, cancellation) and carries out the commands it returns (start an action, start a file, emit an event). Illegal transitions cannot be written, because each state only accepts the inputs that make sense for it. The transition functions are tested exhaustively with table tests and `proptest`.

```rust
// The instance workflow, simplified. Pure: inputs in, next state and commands out.
pub enum InstanceState { Pending, AwaitingShared(Needs), Before(Cursor), Body(Cursor), After(Cursor), Cleanup(Cursor), Done(Status) }
pub enum InstanceInput { SharedReady, SharedFailed(Failure), ActionDone(ActionOutcome), Cancelled }
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
| **Ports and adapters** (hexagonal) | Every outside dependency is a trait in `ports`: `Transport`, `AuthProvider`, `SecretProvider`, `HookRunner`, `HistoryWriter`, `HistoryReader`, `CacheStore`, `Reporter`, `Notifier`, `Importer`, `Clock`, `Random`, `FileSystem` | The core never knows which library sits behind it |
| **Strategy** | Action executors (call, test reuse, function, command); matchers; pipes; body encoders (JSON, form, multipart, file); retry and loop policies (sequential, parallel); the load mix selector; sync merge rules | Swap behaviour by kind without branching in the caller |
| **Factory and registry** | One registry per kind of strategy or adapter, filled at start-up: `ActionExecutorFactory` (by action kind), `AuthProviderFactory` (by auth `type`), `ReporterFactory`, `NotifierFactory`, `ImporterFactory` (by source), `TransportFactory` (by URL scheme, ready for gRPC and WebSocket), `MatcherRegistry`, `PipeRegistry`, `LintRegistry` | Adding a kind is one new file plus one registration line; nothing else changes |
| **Builder** | `EngineBuilder` (the composition root that wires adapters into the engine); `ProgramBuilder` (the compiler accumulates, then `freeze()` gives the immutable `Program`); `RequestBuilder` (renders a request from an api, scope and auth); `PlanBuilder`; `DiagnosticBuilder`; test fixtures (`ProjectBuilder`, `ProgramBuilder::for_tests`); the SDKs' fluent builders | Objects with many optional parts are assembled step by step and checked once; required parts are enforced by typed builder states |
| **Facade** | The `engine` crate: `compile`, `plan`, `run`, `call`, `sync`, `query` | Apps depend on one small surface |
| **Observer (publish/subscribe)** | The event bus and its subscribers: history writer, reporters, console, TUI, web UI, MCP | Producers never know who listens |
| **Chain of responsibility (middleware)** | The request pipeline as `tower` layers: auth → `beforeRequest` hooks → timeout → retry on `refreshOn` → per-host limit → timing → transport | Each concern is one layer, composable and testable alone |
| **Decorator** | `CachingAuthProvider` (the token cache around any provider); `RedactingSink` around every outward stream; `RecordingTransport` and `FaultyTransport` in tests | Add behaviour without changing the wrapped type |
| **Command** | Engine API operations; `syntax::edit` changes as `EditCommand`s that produce `TextEdit`s and can be inverted (undo in the web editor) | Operations become values: logged, previewed, undone |
| **Interpreter and composite** | Templates and pipes; CEL behind `Expr`; matchers composed with `allOf`, `anyOf`, `not`, `every` | Small languages evaluated without running user code |
| **Visitor** | Walks over the `Program` for linking, lints and the planner's needs analysis (`Visit` trait with default no-op methods) | New analyses without editing the IR types |
| **Specification** | Run selection: IDs, globs, tags, grep, changed and failed as composable predicates | Selectors combine freely ("or" for positional, "and" for flags) |
| **Repository** | `HistoryReader`/`HistoryWriter`, `CacheStore` with SQLite and Postgres implementations | Storage swappable; queries named by intent |
| **Null object** | `NoAuth`, `NoopReporter`, `NoHooks` | No `Option` checks scattered through callers |
| **Singleflight (memoization)** | Shared lifecycle slots and the token cache | Concurrent callers share one execution |
| **Flyweight** | Interned strings (`Sym`) for every ID and name | Small, cheap-to-copy IDs across a large program |

### Rules that keep coupling loose

- **Depend on interfaces.** Core crates take `Arc<dyn Port>` (or a generic `T: Port` on hot paths) through constructors; only `EngineBuilder` names concrete adapter types. `arch-tests` fails if a core crate's public signature mentions an adapter type.
- **Small interfaces.** Traits are split by who uses them (`HistoryWriter` for the writer task, `HistoryReader` for queries; `Transport` knows nothing of auth), so no consumer depends on methods it doesn't call.
- **Data crosses boundaries, behaviour doesn't.** Between crates only `ir` types, events and port traits are shared; no crate reaches into another's internals (`pub(crate)` by default).
- **Tell, don't ask, and no train wrecks.** Workers receive the data they need in the command; a function that reaches through `a.b().c().d()` gets that value passed in instead (Law of Demeter, enforced in review).
- **Composition over inheritance.** Behaviour is shared by composing strategies and decorators, never by deep type hierarchies or default-method templates.
- **Refactoring before extending.** When a change needs a `match` on a kind in a second place, it becomes a strategy with a registry ("replace conditional with polymorphism"). When a function grows past the limits, extract functions or a parameter object first, then add the feature.

## Core types

The `ir` crate holds the compiled program: plain data, no behaviour beyond lookups. Every other part reads it. It is also the source of the `vibranium.ir.v1` protobuf messages, which are generated from these types (by a build step) so that the SDKs, plugins and web UI see the same shapes.

### IDs and positions

```rust
/// An interned string: a 4-byte handle into the program's string table.
/// Cheap to copy and compare; turned back into text with `program.str(sym)`.
pub struct Sym(lasso::Spur);

// One newtype per kind of ID, so an ApiId can never be passed where a TestId is expected.
pub struct FileId(Sym);      // "users.crud"
pub struct ApiId(Sym);       // "users.create"          (file id + item id)
pub struct TestId(Sym);      // "users.crud.createsMember"
pub struct ActionId(u32);    // index into program.actions
pub struct AuthId(Sym);      // "shopOauth"
pub struct ProfileId(Sym);   // "staging"
pub struct VarName(Sym);     // "userId"

/// Where something came from, for every diagnostic and report line.
pub struct SourceSpan {
    pub file: SourceFileId,  // index into program.files
    pub start: Pos,          // line and column, 1-based, plus byte offset
    pub end: Pos,
}

pub struct Duration(std::time::Duration); // parsed from "250ms", "2s", "1m"
pub struct Size(u64);                       // bytes, parsed from "4kb", "1mb"
```

### The program

```rust
/// Everything `vc check` produced, frozen and shared read-only (Arc<Program>).
pub struct Program {
    pub strings: lasso::RodeoReader,                 // the interned strings
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
    pub expect: Expect,                   // applied to the call, or to the last step
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
/// The merged checks for one call: api checks first, then the test's, with matching keys replaced.
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
    pub ns: Namespace,                    // Var, Env, Secret, Response, Step(Sym), Example, Item, Iteration, Fake, Dataset, Run
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

The compiler turns the project's files into a `Program` or a list of diagnostics. It runs on `vc check`, at the start of every run, on every keystroke in the editors (through the engine's `Check` call) and before any command writes a file. It collects every problem in one pass instead of stopping at the first.

### Stages

| Stage | Input | Output | Notes |
| --- | --- | --- | --- |
| Discover | project root, `paths` globs | file list with BLAKE3 content hashes | `ignore` walker (respects `.gitignore`); file kind from the suffix |
| Parse | changed files only | a marked tree per file (`syntax::Node`, every node with its span) | `saphyr`; JSON goes through the same tree type |
| Shape-check | marked trees | raw items per file kind, with unknown keys, wrong types and missing required keys reported | shapes are declared once (below) |
| Compile parts | raw items | templates, pipes, CEL programs, matchers, durations, sizes | `template` crate; CEL type-checked against the namespaces available at that spot |
| Link | all raw items | the `Program`: ID tables, references resolved, merged `expect`, var reachability | the only stage that sees every file at once |
| Lint | the `Program` | warnings: naming conventions, expired skips, shared-value writes, cache in per-test blocks | each lint is a registered rule with a V-code |

**Incremental.** `.vibranium/cache/compile.bin` keeps, per file hash, its parsed and shape-checked items. On the next run only changed files are parsed again; linking always runs over everything but is cheap (target: 1,000 files in under 100 ms warm, under 500 ms cold). The editor and `vc watch` keep the cache in memory.

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

1. **Symbol tables.** Every file ID, api ID, test ID, auth ID, profile, dataset and generator goes into a table; duplicates are V2001 with both locations.
2. **References.** `call`, `test`, `cleanup.call`, `auth`, `$file`, `function`, `forEach` datasets and load `mix` entries are resolved; unknown ones are V2003 with "did you mean" from Damerau–Levenshtein distance over the IDs of the right kind, ranked with same-file IDs first.
3. **Expect merge.** For each call, the api's checks come first and the caller's checks replace any with the same key, so every check carries its origin and source line.
4. **Var reachability.** For each template, the linker walks the levels that can supply a var at that spot (call vars, earlier saves in the same test, beforeAll saves under their action id, file vars, beforeRun saves, the api's defaults, the api file, config), separately for every profile. A var that no level provides for some profile is V2101, naming the profiles where it is missing and the places it could be set.
5. **Lifecycle graph.** `test:` reuse forms a graph; cycles are V2205 with the whole chain. Shared actions without an `id` are V2210.
6. **Types.** Pipes and CEL are checked against what each reference can hold (a `| number` on a value the api's schema says is an object is V2304).

### Diagnostics and fixes

```rust
pub struct Diagnostic {
    pub code: Code,                       // V2003
    pub severity: Severity,               // Error | Warning | Info
    pub message: String,                  // unknown api "orders.plcae"
    pub primary: Label,                   // span + short label under the caret
    pub secondary: Vec<Label>,            // other places involved (e.g. the earlier duplicate)
    pub notes: Vec<String>,               // = note: …
    pub help: Vec<String>,                // = help: …
    pub fixes: Vec<Fix>,
}

pub struct Fix {
    pub title: String,                    // Replace with orders.place
    pub certain: bool,                    // true: vc check --fix may apply it without asking per fix
    pub edits: Vec<TextEdit>,             // byte-range replacements, possibly across several files
}

pub struct TextEdit { pub file: SourceFileId, pub range: Range<usize>, pub new_text: String }
```

Diagnostics render through `miette` in the terminal, as JSON with `--json`, and over the engine API to the editors and the web UI, which show the same code, text and fixes.

### Editing files without reformatting them

Everything that changes a definition file (`vc new`, `vc rename`, `--fix`, `vc sync`, `vc skip`, HAR import, the web editor's form view, agents' `write_definition`) goes through one module, `syntax::edit`. It takes the original text and its marked tree and turns structural changes (set a key, insert a list item, remove an item, rename a reference) into `TextEdit`s on the original text, keeping comments, quoting and spacing everywhere else. New text is written in the house style (`vc fmt`): block YAML, two-space indent, keys in the documented order, templates unquoted unless YAML needs quotes. After every edit the result is parsed and compiled again, and the edit is refused if it would add an error.

## Planner

The planner decides what a run will do before anything is sent: which test instances, in which files, needing which lifecycle actions. `vc plan` prints this; `vc run` prints its first line. It is a pure function, so it is tested with property tests rather than against servers.

```rust
pub fn plan(program: &Program, selection: &Selection, options: &RunOptions) -> Result<Plan, Vec<Diagnostic>>;

pub struct Selection {
    pub ids: Vec<IdPattern>,           // test ID, file ID, glob, test#row
    pub paths: Vec<Utf8PathBuf>,
    pub tags: TagExpr,                 // smoke,checkout & !slow
    pub grep: Option<String>,
    pub changed_since: Option<GitRef>, // resolved to file IDs before planning
    pub failed_in: Option<RunId>,      // from history
    pub include_skipped: bool,
}

pub struct Plan {
    pub profiles: Vec<ProfileId>,
    pub run_actions: Vec<PlannedAction>,         // beforeRun actions actually needed
    pub files: Vec<PlannedFile>,                 // in project order
    pub skipped: Vec<SkippedTest>,               // reason, until; reported, never run
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
    pub needs: BitSet,                           // indexes of shared actions this instance reads
}
```

How it works:

1. **Select.** Apply the selectors to the test table: positional selectors are joined with "or", flags filter with "and". Skipped tests and files go to `skipped` unless `include_skipped`; an expired skip is ignored, so the test runs, and the lint already warned.
2. **Expand instances.** For each selected test, create one instance per example row × repeat (dataset rows are counted at plan time; `$file` examples are read then).
3. **Find what each instance needs.** Walk the instance's templates (including those of the calls it makes and of `test:` actions it reuses) and record which shared actions (`beforeRun` and the file's `beforeAll`) it reads. Only those are planned; a `beforeRun` command or function is always planned because it may prepare the system.
4. **Order.** Files keep project order; instances keep file order. `petgraph` checks the action graph is acyclic (already guaranteed by the linker, re-asserted here). Concurrency is decided at run time by the scheduler, not by the plan.
5. **Explain.** `Plan` renders to the plan line (`12 tests in 3 files · lifecycle: beforeRun admin (cached, 3h left) · beforeAll catalog · beforeEach ×9`), to a tree for `vc plan`, and to DOT for `vc plan --dot`.

Property tests (`proptest` over generated programs): every instance's needs are planned; nothing unneeded is planned; selection is idempotent; selecting by file equals selecting its test IDs.

## Executor

The executor runs a `Plan`. It is where concurrency, lifecycle blocks, shared values, cleanup and cancellation come together, so it is designed to need no locks at all: every piece of changing state has exactly one owner task, and shared values are slots created up front and filled once.

### Tasks and who owns what

| Task | One per | Owns | Talks to |
| --- | --- | --- | --- |
| Run orchestrator | run | the run frame: shared `beforeRun` slots, the run's cleanup list, the run status | file orchestrators (spawns them), the event bus |
| File orchestrator | test file in the plan | the file frame: `beforeAll` slots, the file's cleanup list | instance workflows (spawns them under the concurrency limit) |
| Instance workflow | test instance | the instance frame: saved values, its cleanup stack, check results | transport, auth, hooks (all through ports) |
| Event writer tasks | subscriber | their own buffers | history store, reporters, console, UI stream |
| Hook thread | process | the QuickJS runtime | instance workflows, via a request queue |

This section details the workers and frames under the orchestrators described in Orchestration and design patterns. The run orchestrator starts the run's eager `beforeRun` actions (commands and functions), then starts file orchestrators in plan order; instances from different files run side by side, limited by `defaults.concurrency` (a semaphore) and by `defaults.perHost` (a semaphore per host inside the transport).

### Shared values without locks

Because the planner knows every shared action that will be needed, each frame creates its slots before any instance starts:

```rust
/// One per planned shared action (beforeRun or beforeAll), created up front.
/// The first instance that needs it runs the action; the others await the same result.
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

`OnceCell::get_or_init` gives singleflight for free: concurrent callers wait for the one running initialization. The slots live in a read-only map shared by `Arc`; a failed action stores its failure, and every instance that needs it is marked **blocked** with that failure attached. Cleanups are returned as data and run by the frame's owner when it closes, so no task ever writes into another task's state.

### One test instance

```text
run_instance(instance):
  frame = InstanceFrame::new(instance, scope = run saves + file saves (read-only) + file vars + …)
  for action in file.beforeEach ++ test.before:    run_action(action, frame)   # on failure: blocked, skip to after
  if test is Call: run_action(call, frame)
  else for step in test.steps:                     run_action(step, frame)     # stop at the first failure
  evaluate test-level expect on the last response (already merged where it applies)
  for action in test.after ++ file.afterEach:      run_action(action, frame)   # always runs
  run frame.cleanups newest first                                              # always runs, own timeout
  emit TestFinished(status, checks, timings)
```

`run_action` dispatches on the action kind:

- **Call.** Wait for any shared slots the templates read; resolve the effective auth (step, then api, then api file); render the request (method, URL from the api file's `baseUrl`, headers, query, body) against the scope; run `beforeRequest` hooks; send through the transport; run `afterResponse` hooks; evaluate the merged checks; if `poll` is set, repeat until `until` holds or its timeout passes; apply `save` (api saves, then the action's); if the status is 2xx, the api has `cleanup` and the action isn't `keep`, render the cleanup's vars against this response and push it onto the owner's cleanup stack.
- **Test** (`test:` reuse). Run the reused test's calls in order inside the current frame, with each api's checks; the reused test's own test-level checks are skipped.
- **Function / command.** Send the call to the hook thread or spawn the process (with a timeout and the run's environment); a non-zero exit or a thrown error fails the action; a returned object becomes saved values.
- **Loops.** `repeat` and `forEach` expand into iterations run in a `JoinSet` under a semaphore of size `parallel` (default 1). Each iteration writes its saved values into slot `i` of a pre-sized list, so lists come out in iteration order however the iterations finish. On failure, the action waits for the iterations already running, keeps their cleanups, and reports the first failure.
- **Retry.** A failed action (or test, with test-level `retry`) is retried after `delay`; a later pass marks it **flaky** with every attempt recorded.
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

Subscribers (history writer, reporters, console, TUI, web UI, MCP) each get their own task fed by a fan-out task, so a slow subscriber is buffered and cannot slow the run; if a buffer overflows, that subscriber is told how many events it missed and reloads from the history store. Every event passes through the redactor before it leaves the executor.

### Determinism

The executor reaches time, randomness and the file system only through the `Clock`, `Random` and `FileSystem` ports. `--seed` seeds `Random` (fake data, `random`/`shuffle` pipes, dataset picks), so a run can be repeated exactly, and tests use fakes for all three.

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

Profile overrides are applied when the program is loaded for a run, not on every lookup: for each level that has `profiles`, the active profiles' values are layered over the base values once. A lookup walks the levels and returns the first hit; the linker has already proven every reference resolves for the active profiles, so a miss at run time is an internal error, not a user error.

### Rendering

- **Type preservation.** A value that is exactly one template keeps its type (number, list, object); a template inside other text renders to a string. This is decided at compile time and stored in the template.
- **Laziness.** `${fake.*}` values are generated when rendered; a var defined as `${fake.internet.email}` is rendered once per test instance and then reused, so one value can appear in several places.
- **Paths.** After the namespace, paths walk JSON with `.key`, `[n]` (negative from the end), `[*]` (map over a list) and quoted keys; a missing path is a run-time failure of that action, with the path and the actual value shown.
- **Pipes.** Registered functions `fn(Value, &[Arg]) -> Result<Value>`: `number`, `int`, `bool`, `string`, `json`, `jsonString`, `base64`, `fromBase64`, `lower`, `upper`, `trim`, `urlEncode`, `default`, `random`, `shuffle`, `take`. A plugin can register more under a prefix.
- **`$extend` and `$remove`.** The base value is rendered, the changes are deep-merged into it (maps merge, lists and scalars replace, `null` sets null), then the `$remove` paths are deleted.
- **Secrets.** `${secret.X}` renders the real value into the request, and the value is registered with the redactor the moment it is first resolved, so it is masked in every event and output.

### CEL

`cel-interpreter` is wrapped behind a small trait so it can be replaced:

```rust
pub trait Expr: Send + Sync {
    fn eval(&self, activation: &Activation) -> Result<Value, ExprError>;
}
// Activation exposes: status, headers, body, timing, var, and each step id's response.
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

One `TokenCache` sits in front of every provider: a map from `AuthKey` to a slot like the executor's shared slots, so concurrent requests needing the same identity wait for one login. A slot records an expiry; credentials are refreshed `refreshBefore` the expiry by the next caller. On a `refreshOn` status the request is retried once with fresh credentials. With `cache: true`, credentials are also written to the cache store, encrypted, and reused across runs until they expire.

Providers by type: `basic`, `bearer`, `apiKey` (pure functions of their vars); `oauth2ClientCredentials` and `oauth2Refresh` (token endpoint calls through the same transport, so their timings show in reports); `oauth2AuthorizationCode` (opens the browser once, listens on `redirectPort`, stores the refresh token in the OS keychain); `flow` (runs its steps with the executor's action runner in a private frame, then renders `apply`); `plugin` (delegates over the plugin protocol).

### Secrets and redaction

```rust
#[async_trait]
pub trait SecretProvider: Send + Sync {
    async fn resolve(&self, name: &SecretName, profile: &ProfileId) -> Result<Secret, SecretError>;
    async fn status(&self, profile: &ProfileId) -> Result<Vec<SecretStatus>, SecretError>; // for Settings: never values
}
```

The default adapter runs Varlock: on the first secret request of a run it calls `varlock load --format json` once (with the active profile as the environment), keeps the values in memory for the run only, and never writes them to disk. `provider: env` reads environment variables. `Secret` holds its value in a type that does not implement `Debug` or `Serialize` and zeroes its memory on drop, so it cannot be logged or stored by accident.

The redactor is one `aho-corasick` automaton rebuilt whenever a secret is resolved or a token is issued. It also masks the `Authorization`, `Cookie` and `Set-Cookie` headers and any field named in `redact`. It runs on every event before it leaves the executor, on plugin responses, and on MCP results. A redacted value is shown as `•••• (secret SHOP_CLIENT_SECRET)` so the reader knows what was there.

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

All importers produce the same intermediate form and hand it to `syntax::edit`, so every import respects the naming conventions, writes the documentation header and never reformats existing text.

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
| `Edit(changes)` | Structural edits (`syntax::edit`), returning text edits and resulting diagnostics; refuses edits that add errors | web editor form view, Settings, creators, agents |
| `ApplyFixes(codes or fix IDs)` | The certain fixes as edits | `vc check --fix`, Problems panel |
| `Plan(selection)` / `Run(selection, options)` | Plan; run with a stream of `Event`s | all |
| `Call(api, vars, profile)` | One ad hoc request with checks | API client, `vc call`, agents |
| `Import(source)` / `Sync(files, mode)` | Import drafts and sync, with previews | CLI, TUI, web |
| `TestAuth(auth, profile)` / `CheckSecrets(profile)` | Run an auth definition step by step; secret status without values | Settings |
| `History(query)` / `Report(run, format)` | Query stored runs; write a report | all |
| `Approvals.List/Decide` | Pending agent requests and decisions | web UI, `vc approvals` |

### CLI (`apps/cli`)

`clap` derive commands as listed in the CLI and Developer Workflow doc; global flags `--profile`, `--json`, `--project`, `--log`. Console output is written by the console reporter (spec, dots, json) and the `miette` diagnostic renderer; colour and progress bars switch off when stdout is not a terminal. Exit codes follow that doc. `vc completion <shell>` generates static completion plus a dynamic hook (`vc __complete`) that lists project IDs.

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

Codes are stable once released: a code is never reused for a different problem, and each has a `vc explain` page with a wrong and a right example (written in `crates/diag/explain/Vxxxx.md`, checked by a test that compiles both examples). The first release ships these; new ones are added at the end of their range.

| Code | Severity | Problem | Certain fix |
| --- | --- | --- | --- |
| V1001 | error | File is not valid YAML or JSON, or its suffix is not a known kind | — |
| V1002 | error | YAML structure error (indentation, a mapping expected) | when one reading is possible |
| V1003 | error | `syntaxVersion` newer than this Vibranium, or missing | — |
| V1004 | error | Unknown key | rename when one known key is close |
| V1005 | error | Required key missing | — |
| V2001 | error | Duplicate ID | — |
| V2002 | error | Unknown auth, profile, dataset or generator | close match |
| V2003 | error | Unknown api or test | close match |
| V2004 | error | `$file` or snapshot reference not found | close path |
| V2005 | error | Hook export not found | close export |
| V2101 | error | Var used but never set for some profile | close var name |
| V2102 | error | `${item}` outside `forEach`, `${example}` without `examples` | — |
| V2103 | error | Unknown example or dataset column | close column |
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
| V3204 | warning | Unquoted value that YAML cut at `  # ` (a comment) | quote it |
| V4001 | warning | Test ID breaks the convention | rename from the name, with references |
| V4002 | warning | Api ID breaks the convention (case, service prefix) | rename with references |
| V4003 | warning | File name, file ID or folder breaks the convention | — (`vc rename` moves it) |
| V4004 | warning | Tag, profile or dataset name breaks the convention | rename with references |
| V5101 | failure | Expectation failed | — |
| V5102 | failure | CEL check false | — |
| V5103 | failure | Custom check failed or threw | — |
| V5104 | failure | Timeout (request, poll or test) | — |
| V5105 | failure | Transport error (DNS, connect, TLS, reset) | — |
| V5106 | blocked | A lifecycle action this test needs failed | — |
| V5107 | warning | Cleanup failed (reported, results unchanged) | — |

V3204 is new: the skip-reason example that silently lost `see #212` showed that an unquoted `  # ` inside a value is an easy mistake, and the parser can see it (a comment that starts mid-value on a line whose value looks cut).

## Testing, budgets, build and release

### Tests

| Layer | How | Where |
| --- | --- | --- |
| Pure logic (templates, pipes, matchers, var lookup, diff, edits) | Unit tests next to the code | each crate |
| Compiler diagnostics | Golden files: a folder of small projects, each with the expected `vc check` output (`insta` snapshots, text and JSON) | `crates/compiler/tests/cases/` |
| `syntax::edit` | Round trips: apply an edit, re-parse, compare with the expected file byte for byte; comments and formatting elsewhere unchanged | `crates/syntax/tests/` |
| Planner and executor invariants | `proptest` over generated programs with a fake transport: shared actions run at most once per key; every registered cleanup runs exactly once, newest first, also on cancellation; loops keep order under parallelism | `crates/executor/tests/` |
| Adapters | One contract suite per port that every adapter must pass (a transport suite, a store suite, …) | `crates/ports/contract/` |
| End to end | `vc` against the in-repo mock server, whose request counter proves the lifecycle guarantees (50 tests sharing a `beforeAll` send 1 request) | `bench/mock/`, `tests/e2e/` |
| Interfaces | CLI output snapshots; TUI rendered to a test backend; web UI with Playwright against `vc ui` and the mock server | `apps/*/tests/`, `web/tests/` |
| Docs | Every example YAML in the docs is extracted and compiled in CI | `docs/tests/` |

### Performance budgets (checked in CI, fail on a 10% regression)

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

- **Toolchain.** Rust 1.97 pinned; `cargo build --release` with thin LTO, 4 codegen units and stripped symbols (as in the spike). The web UI and report front ends are built with Vite first and embedded.
- **Targets.** macOS (arm64, x64), Linux (x64, arm64, statically linked with musl), Windows (x64).
- **Distribution** through `cargo-dist`: GitHub releases with checksums and build provenance, a one-line installer (`curl -fsSL https://…/install.sh | sh` and a PowerShell equivalent), a Homebrew tap, a Docker image, and an npm package `@vibranium/cli` that downloads the right binary (for JavaScript projects that want it in `devDependencies`).
- **Command names.** The installer adds `vibranium` and `vc`, and warns if another `vc` is already on `PATH`.
- **CI.** Every PR runs `gates.sh`, the test suites, the e2e tests on Linux and macOS, and the budgets; releases are cut from tags by `cargo-dist`'s workflow.

## Implementation plan

Milestones are sized for a coding agent to take one at a time, each ending in something runnable and checked by the acceptance tests listed. Each milestone's PRs must pass `gates.sh`. The release column maps them to the PRD's release plan.

| # | Milestone | Delivers | Done when | Release |
| --- | --- | --- | --- | --- |
| M0 | Workspace and gates | Crate skeletons with the dependency rules, `gates.sh`, `arch-tests`, CI, the mock server, `rust-toolchain.toml` | CI is green; a deliberately bad commit for each gate is rejected by that gate | — |
| M1 | Read and check | `syntax` (marked YAML/JSON), file shapes for all kinds, `template` (templates, pipes, CEL), the linker, diagnostics V1xxx–V3xxx, `vc check`, `vc schema export` | The diagnostics golden suite passes; the compile budget holds on a generated 1,000-file project | v2.0 |
| M2 | Run | Planner, executor (calls, steps, lifecycle blocks, shared slots, cleanup, loops, retry, poll, cancellation), HTTP transport with timings, matchers and checks, vars and profiles, console, CTRF and JUnit reporters, `vc run` selectors | The e2e suite passes against the mock server, including "50 tests, 1 setup request" and cleanup on Ctrl-C; the executor property tests pass | v2.0 |
| M3 | Identity and memory | Auth providers and token cache, Varlock secrets, redactor, history store, cache store, `vc history`, the HTML report (folder and single), `vc report` | Secrets never appear in any output in the redaction test suite; a 10,000-test report meets its budget | v2.0 |
| M4 | Code and data | Hook bridge (oxc + QuickJS), custom checks, `function:` actions, generators, datasets, `$extend`, `$cel` | Hook budgets met; a hook overrunning its time or memory is stopped and reported with the TypeScript line | v2.0 |
| M5 | Authoring | `syntax::edit`, `vc init` with the sample and `vc demo`, `vc new`, `vc rename`, `vc fmt`, `vc check --fix`, `vc explain`, `vc skip`, `vc watch`, shell completion, V4xxx lints | Edit round-trip tests pass byte for byte; a fresh `vc init && vc run` passes offline | v2.0 |
| M6 | Import and sync | OpenAPI (Swagger 2.0, 3.0, 3.1) import and three-way sync with snapshots, HAR, cURL, Postman, Bruno, the v1 migrator | Sync passes its rule-table tests; a recorded HAR replays green against the mock server | v2.0 |
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
