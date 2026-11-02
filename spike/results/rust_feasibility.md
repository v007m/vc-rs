# Vibranium v2 — Rust feasibility spike (hooks, borrow checker, clean-code gates, build loop)

Date: 2026-10-23. Machine: Linux VM, 2 vCPU (Xeon 2.8 GHz), 7.8 GB RAM, rustc 1.97.0, `CARGO_BUILD_JOBS=2`, default linker (mold not used, see Part D).
Code: `/home/claude/vibranium-spike/rust-feasibility/` — `parta/` (hook prototypes), `partb/` (architecture slice), `scripts/`, `logs/borrow_checker_log.md`, `raw/` (every raw measurement). Machine-readable numbers: `results/rust_feasibility.json`.

**Calibration note.** This VM is slow at allocation. Cloning the 20-item test response as a `serde_json::Value` in pure Rust takes about 8 µs, and a `to_string`+`from_str` round-trip takes 26–32 µs. Read the per-call hook numbers against that baseline, not against laptop intuition. (Swapping in mimalloc halved the allocation cost; I did not adopt it in the prototypes.)

---

## TL;DR

| Question | Answer |
|---|---|
| (a) Typed TS hooks in-process? | **Yes.** Users keep `.ts` files with `import type { BeforeRequestHook } from "@vibranium/sdk"` and type-check them with `tsc` in their own repo. At run time the engine strips the types with oxc and runs the code in an embedded JS engine. Both QuickJS and V8 work. **Recommend QuickJS (rquickjs):** it adds 1.5 MB on top of oxc (4.2 MB total over hello-world), reaches the first hook call in 3.2 ms, costs 20–36 µs per call, and has timeout, memory cap, no fs/net and TS line numbers working. V8 (deno_core) built fine but adds 53 MB, needs 25 ms to start, and was *not* faster per call for hook-sized work. |
| (b) Borrow-checker pain? | **Close to none with this design.** The 10-crate slice (~1,310 production + ~370 test code lines) compiled with **0 errors on the first `cargo check`**, and 13/13 tests passed on the first run. Counts: 0 explicit lifetimes in core crates (engine, ports, compiler, adapters-http/store, cli), 0 lifetime parameters in any crate's public API, 1 `Mutex` (the singleflight map), 0 `RwLock`, 0 `unsafe`, 0 `static`. The lifetimes that do exist (`'js`, `'static`) sit inside the private QuickJS FFI module. |
| (c) Clean code enforced by machines? | **Mostly.** Long function, long parameter list, complex types, unwrap/expect/todo/dbg, global data, mutable shared data, layer violations, token-level duplication, unused code/deps, supply chain and weak tests are all gated, and each gate was shown to fire on injected violations. **Not machine-checkable:** YAGNI/speculative generality, naming quality, shotgun surgery, semantic duplication, and "switch on type code repeated across modules". Those still need review. Two config traps: clippy's `cognitive_complexity` is **not** in pedantic/nursery (the threshold is silently ignored unless the lint is enabled explicitly), and rustc `unreachable_pub` contradicts clippy `redundant_pub_crate`. |
| Build loop | After editing one engine file: `cargo check` 0.8 s, debug build 2.3 s, `cargo test --workspace` 4.6 s, release build 47 s. Cold builds: 120 s debug, 250 s release (2 vCPU). All gates take 17 s warm. |
| Verdict | **Rust is viable for this project, with conditions:** keep the design rules below (they are what made the borrow checker a non-issue), make `gates.sh` (in `partb/`) a required CI check that agents must pass, and keep a human review checklist for the smells no tool catches. |

---

## Part A — typed TypeScript hooks running in-process

### How it works
1. **Authoring (user's repo).** Users write hooks exactly as in the brief (`parta/hooks/sign.ts`, `checks.ts`). A type-only package `@vibranium/sdk` (`parta/sdk/node_modules/@vibranium/sdk/index.d.ts`) declares `BeforeRequestHook`, `CustomCheck`, `HttpRequest`, and the global host function `hmacSha256`. `tsc --noEmit` passes on the good hooks and **rejects** `hooks/bad_types.ts`, which puts a number in a header: `TS2322 … Type 'number' is not assignable to type 'string'`. So users get real typing, but **only through `tsc` in their repo or editor. The engine does not type-check at run time** (by design: oxc strips types, it does not check them).
2. **Loading (engine, at run time).** `tsstrip` (oxc 0.155: parser → semantic → transformer → codegen with source map) turns `sign.ts` into JS and removes `import type` lines and every annotation. Output: `export const addSignature = ({ request, secret }) => { const sig = hmacSha256(secret("UPLOAD_KEY"), request.path); … }`.
3. **Execution.** The module is evaluated in a sandboxed engine. The host provides `hmacSha256` (Rust `hmac`+`sha2`) as a global and `secret(name)` as a per-call function backed by an allow-listed secret map. Both engines produce the identical, correct signature `f76dc2cf…0e36e4`.

The engine-neutral API (`parta/crates/hookbench`, trait `HookEngine`) is the shape of the future `HookRunner` port. Both prototypes implement it, so they ran the identical measurement driver.

### Results

| | QuickJS (`rquickjs` 0.14, quickjs-ng) | V8 (`deno_core` 0.413, V8 150) |
|---|---|---|
| Builds here? | yes | **yes**. It downloads a 186 MB prebuilt `librusty_v8.a` from GitHub release assets; first build 178 s, no problems |
| Release binary (stripped, thin LTO) | 4.58 MB | 53.4 MB |
| Added over hello-world (0.34 MB) | **+4.24 MB** (oxc alone +2.75 MB; QuickJS+glue +1.5 MB) | **+53.1 MB** (V8+deno_core+tokio ≈ +50.3 MB beyond oxc) |
| Cold release build (fresh target dir) | 165 s (hello 4.7 s; oxc-only 116 s) | 237 s (incl. V8 archive download) |
| Startup → first hook call → exit (hyperfine, 30 runs, median wall) | **3.2 ms** (hello 1.9 ms, oxc-only 2.6 ms) | **24.8 ms** |
| In-process breakdown | engine 0.4 ms, strip+load 0.4 ms, 1st call 0.1 ms | isolate 20 ms, load 0.8 ms, 1st call 0.3 ms |
| Peak RSS: first call / 20k-call bench | 6.4 MB / 11 MB | 28.3 MB / 46.6 MB |
| **beforeRequest ns/call** (100k calls, median of 3) | **20,177** (direct) / 44,673 (JSON text) | 37,883 (serde_v8) / **25,417** (JSON text) |
| **customCheck ns/call** (20-item response) | **35,638** (direct) / 73,665 (JSON text) | 53,100 (serde_v8) / **39,817** (JSON text) |
| Same calls with no-op hook bodies (= marshalling floor) | 15,464 / 34,527 | 30,009 / 45,347 (serde_v8); 19,376 / 35,299 (JSON) |
| Timeout on `while(true){}` | ✅ interrupt handler; stopped at **200.3 ms** of a 200 ms budget; uncatchable `interrupted` error | ✅ watchdog thread + `TerminateExecution`; **200.6 ms** |
| Memory cap (64 MB) on an allocation loop | ✅ `out of memory` error after 160–174 ms | ✅ `heap_limits` + near-heap-limit callback that terminates (without the callback V8 aborts the **whole process**) |
| File / network access | ✅ denied by default: QuickJS core has no fs/net/process/timers; no module loader, so `import("fs")` → `could not load module 'fs'` | ✅ denied by default: deno_core ships no fs/net ops; loader refuses every import (`imports are disabled in hooks: 'fs'`). **But** deno_core exposes `globalThis.Deno.core.ops`, which I delete in a bootstrap script |
| Engine reusable after kill/OOM | ✅ | ✅ (`cancel_terminate_execution`) |
| Error surfacing with TS line numbers | ✅ via oxc source map + 40-line `remap_stack`: `at total (hooks/throws.ts:7:10)` | ✅ automatic: deno_core calls `ModuleLoader::get_source_map`: `at total (file:///…/throws.ts:7:16)` |

Probe hooks: `parta/hooks/hostile.ts`. Globals checked: `fetch, XMLHttpRequest, require, process, Deno, std, os, WebSocket`. All absent in both engines. Raw output: `raw/{qjs,deno}_{safety,errors,first_call,bench_*}.json`.

**What the per-call numbers mean.** In both engines marshalling dominates. The user code itself (spread, HMAC, Set) costs only about 1–5 µs on top of the no-op floor. Converting the ~200-value response into engine objects costs about 30 µs. V8's JIT doesn't help with this. serde_v8 turned out *slower* than QuickJS's direct conversion, and V8 only wins when it parses JSON text. Design implications:
- Convert each response **once per test** and run all checks against that same JS object.
- Use JSON-text marshalling for V8 and direct marshalling for QuickJS.
- 20–40 µs per hook call is small next to an HTTP round trip (0.2–1 ms on localhost), but it is ~100x a native Rust check. Keep built-in assertions in Rust, and keep TS hooks for custom logic.

**Caveats found.**
- QuickJS only checks the interrupt between bytecodes. A single long native builtin (huge `String.repeat`, pathological regex) can overrun the deadline before it is stopped.
- V8 ropes strings: my first memory-hog probe (`"x".repeat(1<<20)+n`) hit the timeout, not the memory cap. I switched it to number arrays.
- QuickJS (quickjs-ng) is ES2023-ish and slower on heavy compute. V8 is fully modern.
- deno_core churns its API often (0.413 already, with pinned-scope macros `scope!`/`tc_scope!` that differ from older tutorials). Its isolate must be created inside a tokio runtime context, or a V8 delayed task panics. I hit exactly that.
- rquickjs `Runtime` is `!Send`. Part B runs it on a dedicated thread behind a channel, which turned out clean.

### Exact JSON marshalling code

QuickJS (`partb/crates/adapters-hooks/src/marshal.rs`, same as `parta/crates/hook-qjs/src/marshal.rs`):
```rust
pub fn json_to_js<'js>(ctx: &Ctx<'js>, v: &Json) -> rquickjs::Result<Value<'js>> {
    Ok(match v {
        Json::Null => Value::new_null(ctx.clone()),
        Json::Bool(b) => Value::new_bool(ctx.clone(), *b),
        Json::Number(n) => n.as_i64().and_then(|i| i32::try_from(i).ok()).map_or_else(
            || Value::new_float(ctx.clone(), n.as_f64().unwrap_or(f64::NAN)),
            |i| Value::new_int(ctx.clone(), i),
        ),
        Json::String(s) => s.as_str().into_js(ctx)?,
        Json::Array(items) => {
            let arr = Array::new(ctx.clone())?;
            for (i, item) in items.iter().enumerate() { arr.set(i, json_to_js(ctx, item)?)?; }
            arr.into_value()
        }
        Json::Object(map) => {
            let obj = Object::new(ctx.clone())?;
            for (k, item) in map { obj.set(k.as_str(), json_to_js(ctx, item)?)?; }
            obj.into_value()
        }
    })
}

pub fn js_to_json<'js>(ctx: &Ctx<'js>, v: &Value<'js>) -> rquickjs::Result<Json> {
    Ok(match v.type_of() {
        Type::Uninitialized | Type::Undefined | Type::Null => Json::Null,
        Type::Bool => Json::Bool(v.as_bool().unwrap_or(false)),
        Type::Int => Json::from(v.as_int().unwrap_or(0)),
        Type::Float => Number::from_f64(v.as_float().unwrap_or(f64::NAN)).map_or(Json::Null, Json::Number),
        Type::String => Json::String(v.as_string().map(rquickjs::String::to_string).transpose()?.unwrap_or_default()),
        Type::Array => array_to_json(ctx, v)?,   // arr.iter::<Value>().map(|x| js_to_json(ctx, &x?)).collect()
        Type::Object => object_to_json(ctx, v)?, // obj.props::<String, Value>() → serde_json::Map
        other => return Err(Exception::throw_type(ctx, &format!("hook returned non-JSON value: {other:?}"))),
    })
}
// JSON-text variant: ctx.json_parse(v.to_string()) / ctx.json_stringify(value) → serde_json::from_str
```
V8 (`parta/crates/hook-deno/src/marshal.rs`):
```rust
pub fn to_v8<'s>(scope: &mut v8::PinScope<'s, '_>, v: &Json, how: Marshal) -> Result<v8::Local<'s, v8::Value>, String> {
    match how {
        Marshal::Direct => serde_v8::to_v8(scope, v).map_err(|e| e.to_string()),
        Marshal::JsonText => {
            let text = v8::String::new(scope, &v.to_string()).ok_or("string alloc failed")?;
            v8::json::parse(scope, text).ok_or_else(|| "JSON.parse failed".to_owned())
        }
    }
}
pub fn from_v8<'s>(scope: &mut v8::PinScope<'s, '_>, v: v8::Local<'s, v8::Value>, how: Marshal) -> Result<Json, String> {
    match how {
        Marshal::Direct => serde_v8::from_v8(scope, v).map_err(|e| e.to_string()),
        Marshal::JsonText => {
            let text = v8::json::stringify(scope, v).ok_or("JSON.stringify failed")?;
            serde_json::from_str(&text.to_rust_string_lossy(scope)).map_err(|e| e.to_string())
        }
    }
}
```
The hook argument is built per call as `{ request, secret }` (beforeRequest) or `{ request, response }` (check). `secret` is a host function created once and reused.

### Part A verdict
- **QuickJS: recommended.** Small, starts instantly, the sandbox is simple and synchronous, and every safety control works. The per-call cost is fine for hooks. The risk is weaker raw JS performance for compute-heavy hooks, plus the interrupt granularity caveat above.
- **V8/deno_core: works, not worth it here.** It is 12x the binary size, 8x the startup time, needs a tokio context and a watchdog thread, its API churns often, and it brings no per-call win for hook-sized payloads. Revisit only if users need full modern JS/npm-grade compatibility or heavy compute in hooks.

---

## Part B — architecture slice that avoids borrow-checker pain

Workspace `partb/` (10 crates):

| crate | role | internal deps (enforced) |
|---|---|---|
| `ir` | immutable `Program` arena; `TestId(u32)`/`FileId(u32)` indices; newtypes (`TestName`, `UrlPath`, `StatusCode`, `VarName`, `Template` …) | — |
| `ports` | traits `Transport`, `AuthProvider`, `HookRunner`, `ResultStore`, `Reporter` (+ DTOs) | ir |
| `engine` | scheduler (tokio tasks + semaphore), executor, assertions, singleflight `SetupCache` | ir, ports |
| `compiler` | stub: hard-coded 53-test demo program (50 share one file-scoped setup) | ir |
| `adapters-http` | `ReqwestTransport`, `StaticTokenAuth` | ir, ports |
| `adapters-store` | `SqliteStore`: one writer thread owns the `rusqlite::Connection`, fed by `tokio::mpsc`, batched transactions | ports |
| `adapters-hooks` | `QjsHookRunner`: the Part A QuickJS sandbox on its own thread behind mpsc + oneshot | ir, ports, tsstrip |
| `tsstrip` | oxc type stripping + source-map remapping | — |
| `cli` | composition root (the only crate naming adapters) + `vibranium` binary | all |
| `arch-tests` | architecture/design rule tests (cargo_metadata + syn) | — |

**Design rules and how they were verified**
- IR immutable and shared: `Program` has no `&mut self` methods. `ProgramBuilder::build()` validates the graph and freezes it. The engine receives `Arc<Program>`.
- Graph by IDs: `SetupRef { test: TestId, scope }`. There are no references between nodes, and lookups go through `program.test(id)`.
- Ports injected, no global state: `engine::Ports { transport: Arc<dyn Transport>, … }`. The arch test bans every `static` item in the workspace, and the clippy `disallowed-types` list covers `Mutex/RwLock/RefCell/LazyLock/OnceLock/thread_local`.
- Channels: worker tasks → `mpsc<TestResult>` → collector → `Reporter` + `ResultStore`. `ResultStore` (SQLite) → `mpsc` → single writer thread. `HookRunner` → `mpsc<Job{…, oneshot}>` → QuickJS thread.
- Singleflight: key = `(TestId, ScopeKey::File(FileId)|Run, Vars::fingerprint())`, value = `Arc<tokio::sync::OnceCell<Result<Vars, CallError>>>`. A test reused as setup runs only its HTTP call (no assertions), and failures are memoized too.
- **Proof against the Go mock server** (`cli/tests/mock_server.rs`, 53 tests, 32 concurrent, setup endpoint `POST /slow/sessions` sleeps 50 ms): mock `/__stats` = `{"POST /slow/sessions": 1, "GET /sessions/:id": 50, "POST /items": 1, "POST /uploads": 1, "GET /items/:id": 1}`, run = 53 passed, 53 rows written to SQLite. **Negative control:** with memoization bypassed, the same test sees `POST /slow/sessions: 50` and fails (`raw/partb_singleflight_negative_control.txt`). The `signed upload` test runs the real `sign.ts` hook through QuickJS.
- Engine unit tests (7, fake ports): 50→1 setup, file scope = once per file vs run scope = once per run, failed setup memoized, assertions/auth/hook applied, store failures counted, concurrency 0/1, every result reaches store and reporter.

**Counts** (`scripts/code_metrics.py`, comments/strings stripped; `raw/partb_code_metrics.json`)

| crate (production code) | code lines | explicit lifetimes | `.clone()` | `Arc::clone` | `Mutex`/`RwLock` | `Arc<Mutex>` | `unsafe` | `static` |
|---|---|---|---|---|---|---|---|---|
| ir | 271 | 2 (`Formatter<'_>` in `Display` impls) | 3 | 0 | 0 | 0 | 0 | 0 |
| ports | 85 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| engine | 203 | **0** | 11 | 4 | **1** (`Mutex` in `SetupCache`) | 0 | 0 | 0 |
| compiler | 58 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| adapters-http | 70 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| adapters-store | 78 | 0 | 1 | 0 | 0 | 0 | 0 | 0 |
| cli | 95 | 0 | 3 | 0 | 0 | 0 | 0 | 0 |
| adapters-hooks (QuickJS FFI) | 272 | 19 (`'js`, `'static`, `'_`) | 21 (10 are `ctx.clone()`, a refcount bump rquickjs requires; the rest copy ids or JS handles) | 0 | 0 | 0 | 0 | 0 |
| tsstrip | 90 | 2 | 1 | 0 | 0 | 0 | 0 | 0 |
| arch-tests | 92 | 2 (`Visit<'_>`) | 1 | 0 | 0 | 0 | 0 | 0 |
| **total** | **1,314** | **25** (0 in any crate's public API) | 41 | 4 | 1 | 0 | **0** (`unsafe_code = "forbid"`) | 0 |

The `'js` lifetimes appear on `pub fn` lines inside the *private* `marshal` module of adapters-hooks. That crate's public API (`QjsHookRunner`, `HookConfig`, `Limits`) has none. Elided borrows do appear on simple getters (`Program::test(&self, id) -> &Test`, `Vars::get`, `as_str`), but no public signature names a lifetime. The engine's 11 `.clone()`s break down as: 3 copy values out of the immutable IR into an owned request (header names, body), 4 copy `Vars`/`TestName` into per-test results, 1 clones the memoized setup result per waiter, and 3 bump handles. **None of them were added to silence a borrow error.**

**Borrow-checker log** (`logs/borrow_checker_log.md`): **0 borrow-checker errors** in Part B. Part A had 0 too; its only failures were runtime/FFI issues:
1. QuickJS assertion at exit, because Rust drops struct fields in declaration order and `Persistent` handles were declared after the `Runtime`. Fixed by reordering the fields; the compiler cannot catch this.
2. V8 isolate created outside a tokio context (panic).
3. My module loader refusing the hook module itself.

**Honest friction verdict.** With the rules above (IDs instead of references, immutable `Arc` IR, owned messages over channels, `!Send` engines on their own threads, `Arc<dyn Trait>` ports), the borrow checker never fired. Lifetimes do not leak into the architecture. What remains:
- FFI/engine embedding code (`'js` lifetimes, drop order, `!Send`). It is small, self-contained and should stay in one adapter crate.
- `async_trait` boxing on every port call (negligible next to I/O).
- Some deliberate data copying (`clone`), which is cheap and explicit.

Caveat: the slice was written by an agent that designed for the borrow checker up front. A design that shares mutable graphs by reference, or holds borrows across `.await`, would have hit errors. The rules are what make Rust easy here, and they are themselves enforced: arch tests, disallowed types, and no statics.

---

## Part C — automated clean-code gates

One command: `partb/gates.sh` (green; 17 s warm). It runs fmt (`rustfmt.toml`: max_width 120), clippy, tests (incl. arch rules + mock e2e), jscpd, cargo-deny and cargo-machete; `GATES_MUTANTS=1` adds cargo-mutants. Config: `[workspace.lints]` in `partb/Cargo.toml` + `partb/clippy.toml` + `partb/deny.toml`.

- **clippy** `--all-targets -- -D warnings -W clippy::pedantic -W clippy::nursery`, plus workspace lints `unwrap_used/expect_used/todo/unimplemented/dbg_macro/panic = deny`, `cognitive_complexity = warn`, `unsafe_code = forbid`. `clippy.toml`: `cognitive-complexity-threshold = 10`, `too-many-lines-threshold = 40`, `too-many-arguments-threshold = 4`, `type-complexity-threshold = 150`, `allow-{unwrap,expect,panic}-in-tests = true`, `disallowed-types` (Mutex, RwLock, tokio Mutex/RwLock, RefCell, LazyLock, OnceLock) and `disallowed-macros` (`thread_local!`).
  - **Violations on the first real run: 15 code findings + 30 config-conflict findings, green after 5 runs (~10 min of agent time).** Code findings: 2 `missing_const_for_fn`, 1 `or_fun_call`, 5 `doc_markdown`, 1 `option_if_let_else`, **1 `too_many_arguments` (5/4)** in the compiler stub (a real long-parameter-list smell, fixed by introducing a `call(method, path)` builder), 1 `many_single_char_names`, 4 `unwrap_used` in integration-test helper fns (`allow-unwrap-in-tests` does not cover them). Config conflict: 17 `unreachable_pub` (an extra rustc lint I had added) then 13 `redundant_pub_crate` after "fixing" them. **No visibility satisfies both lints**, so I dropped `unreachable_pub`.
  - **Trap:** in clippy 0.1.97 `cognitive_complexity` is not in pedantic or nursery. The requested command line *does not enforce* `cognitive-complexity-threshold = 10` unless the lint is enabled explicitly. Verified: the injected 11-complexity function is only flagged once `-W clippy::cognitive_complexity` / workspace lint is set. Real code passes with it enabled.
- **Gate injection test** (`raw/partc_smell_injection.txt`): a deliberately smelly module produced 26 findings, every intended one. These were `too_many_lines (44/40)`, `cognitive_complexity (11/10)`, `too_many_arguments (5/4)`, `type_complexity`, `fn_params_excessive_bools`, `match_same_arms`, `unwrap_used`, `expect_used`, `todo`, `dbg_macro`, `disallowed_types` (global `static Mutex`), `unsafe_code` (`static mut`), and `dead_code`. Plus the syn heuristic flagged an injected `Extract.request_id: String`.
- **Duplication (jscpd 4, ≥50 tokens):** Part B has **0 clones** in 22 Rust files (2,001 lines). Positive control over Part A + Part B: **8 clones / 189 lines (6.0%)**, exactly the code I copied (`tsstrip`, QuickJS marshalling, `Deadline`/hmac helpers). So jscpd works on Rust. It catches token-level copy-paste only.
- **Architecture rules** (`arch-tests`, `cargo_metadata` + `syn`): an allow-list of internal deps per crate (`engine` → only `ir, ports`; `ir` → nothing; adapters → never engine/cli; an unknown new crate fails until it is classified), plus no `static` items anywhere, plus a primitive-obsession heuristic for `ir`/`ports`. Negative control: adding `adapters-http` to engine and `tsstrip` to ir makes the test fail with `` `engine` must not depend on `adapters-http` `` / `` `ir` must not depend on `tsstrip` ``.
- **cargo-deny 0.20.2** (prebuilt binary from GitHub, installed in seconds; advisory DB fetched in ~2 s): advisories ok, bans ok, licenses ok, sources ok. Warnings: duplicate versions of `hashbrown` and `syn`.
- **cargo-machete 0.9.2**: no unused dependencies.
- **cargo-mutants 27.1.0 on `engine`** (prebuilt; 20 mutants, 44 s): **11 caught, 4 missed, 5 unviable → 73%**. The missed mutants exposed real gaps: no test for a failing `ResultStore` (`store_errors += 1`), and an untested magic `concurrency * 2` channel capacity. After adding 2 tests and removing the magic number: 18 mutants, **13/13 viable caught (100%)**, 41 s.

### Smell → gate map

| Smell | Machine gate | Strength |
|---|---|---|
| Long function | `clippy::too_many_lines` (40) + `clippy::cognitive_complexity` (10, **must be enabled explicitly**) | ✅ enforced (verified) |
| Long parameter list (bonus) | `clippy::too_many_arguments` (4), `fn_params_excessive_bools` | ✅ enforced (caught a real case) |
| Switch statements | Rust `match` is exhaustive: a new enum variant breaks every match at compile time; `match_same_arms`; optionally `wildcard_enum_match_arm` (restriction) to forbid `_ =>` | ⚠️ partial. **The same switch repeated across modules is not detected → review** |
| Primitive obsession | custom `syn` test: id/status/method/url/path/token fields in `ir`/`ports` must not be `String`/ints; `struct_excessive_bools`, `fn_params_excessive_bools` | ⚠️ heuristic only (name-based). Money-as-`f64` etc. → review |
| Mutable data | immutability by default + `&mut` exclusivity (compiler); `disallowed-types` forces each `Mutex/RwLock/RefCell` to carry `#[expect(clippy::disallowed_types, reason = …)]` (greppable: 1 in the codebase) | ✅ shared mutation enforced; local `let mut` allowed |
| Global data | `unsafe_code = "forbid"` (no `static mut`), arch test bans all `static` items (incl. atomics), `LazyLock/OnceLock/thread_local!` disallowed | ✅ enforced |
| Duplicated code | jscpd (≥50 tokens), `match_same_arms`, `if_same_then_else`, `branches_sharing_code` | ✅ copy-paste; ❌ semantic duplication → review |
| Shotgun surgery | none. Arch dependency allow-list limits blast radius; a git co-change coupling report (e.g. code-maat) would be the next step | ❌ review |
| YAGNI / speculative generality | `dead_code`/`unused_*` (warnings are errors), cargo-machete, cargo-mutants (survivors flag untested or unneeded code, e.g. the `* 2`) | ⚠️ partial. Unneeded abstractions, single-impl traits and config knobs → review |
| Naming | `many_single_char_names`, `similar_names`, `module_name_repetitions` (pedantic) | ❌ meaning → review |
| Panics / debug leftovers | `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`, `dbg_macro` = deny | ✅ enforced |

---

## Part D — build loop (Part B workspace incl. QuickJS hooks, oxc, SQLite, reqwest)

| step | median (s) | runs (s) |
|---|---|---|
| cold `cargo build` (debug, empty target dir) | **119.9** | 136.5, 119.9, 119.9 |
| cold `cargo build --release` (thin LTO, empty target dir) | **250.3** | 258.6, 250.3, 247.4 |
| incremental `cargo check` after editing `engine/src/scheduler.rs` | **0.79** | 0.73, 0.79, 0.81 |
| incremental debug `cargo build` after the same edit | **2.26** | 2.28, 2.26, 1.94 |
| `cargo test --workspace` after the edit (build + run all 17 tests incl. mock-server e2e) | **4.59** | 4.59, 4.63, 4.27 |
| `cargo test -p engine` after the edit | **0.82** | 0.88, 0.81, 0.82 |
| incremental `cargo build --release` after the edit | **47.4** | 47.4, 48.2, 46.7 |

Each incremental step makes a real code edit (appends a new `const` to `engine/src/scheduler.rs`), so `engine` and `cli` recompile. Crates.io downloads were already cached, so cold numbers exclude network time. The 2 vCPUs are the main limit: cold builds are dominated by C code (QuickJS, bundled SQLite) plus oxc and reqwest/hyper. Release incremental cost is thin LTO + `codegen-units = 4` relinking everything. For agent loops, use `cargo check` / `cargo test -p <crate>` (under 1 s) and leave release builds to CI. The full gate script (`gates.sh`: fmt, clippy, all tests, jscpd, deny, machete) takes **17 s** warm; with `GATES_MUTANTS=1`, add about 45 s for the engine mutants. For comparison, the earlier spike measured Go: cold 31 s, incremental 0.17 s.

---

## Recommendation

**Rust is viable for Vibranium v2 with a maintainer who cannot read Rust, if the review burden moves from reading code to reading gate output.** Evidence:
1. Hooks stay TypeScript. With QuickJS + oxc the cost is +4.2 MB, ~1 ms to start and 20–36 µs per call, with timeout, memory cap and no fs/net working, and errors show original TS lines.
2. The borrow checker was a non-event. The architecture rules made it so: IDs not references, immutable `Arc<Program>`, owned messages over channels, `!Send` engines on their own threads, `Arc<dyn Port>` injection. These rules are themselves machine-checked.
3. Most Fowler smells are gated mechanically, and each gate was shown to fire. The remainder needs a short human checklist: YAGNI/speculative generality, naming, shotgun surgery, semantic duplication, repeated switches.

**Conditions:**
- Require `gates.sh` (+ `GATES_MUTANTS=1` nightly) in CI. Agents must not edit `clippy.toml`, `deny.toml`, `[workspace.lints]` or `arch-tests` without explicit approval. Watch for new `#[expect(...)]`/`#[allow(...)]` attributes. They are greppable, and a CI diff check on them is cheap.
- Enable `clippy::cognitive_complexity` explicitly, and don't add `unreachable_pub`.
- Keep FFI (QuickJS) isolated in one adapter crate. That is where the remaining lifetimes and drop-order hazards live.
- Use incremental builds and `cargo check` for agent loops. Cold release builds take minutes and need a CI cache.

**Risks / what this spike did not prove:**
- The slice is ~1.3k lines; a 20–30k-line engine will produce more and subtler lint churn.
- Agent friction was low partly because the design was chosen up front by an agent that knows Rust's ownership rules.
- Async hooks (returning promises) work in both prototypes, but I didn't benchmark them.
- I didn't try mold. Rust 1.97 links with lld by default on x86_64 Linux, which already removes most of mold's advantage.
- The per-call hook numbers come from a slow VM; expect roughly 2–3x faster on a modern laptop.

## Reproduce
```
cd /home/claude/vibranium-spike/rust-feasibility
# Part A
cd parta && cargo build --release && for e in qjs deno; do for m in first-call errors safety; do target/release/hook-$e $m hooks; done; target/release/hook-$e bench hooks 100000 3 direct; done
/tmp/claude-0/tsc/node_modules/.bin/tsc -p hooks/tsconfig.json   # user-side type check
# Part B / C
cd ../partb && ./gates.sh && GATES_MUTANTS=1 ./gates.sh
python3 -I ../scripts/code_metrics.py crates
# Part D
../scripts/build_loop.sh "$PWD" ../raw/partd_build_loop.csv
```
