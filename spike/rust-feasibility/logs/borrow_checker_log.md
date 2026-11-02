# Borrow-checker / compiler-friction log (kept while writing)

Format: where | compiler error | resolution | minutes lost (approx)

## Part A (hook prototypes)
- tsstrip, hook-qjs, hook-deno: 0 borrow-checker errors on first compile of each crate.
- hook-qjs | not a compile error: runtime assertion `JS_FreeRuntime: list_empty(&rt->gc_obj_list)` at exit | Rust drops struct fields in declaration order; `Persistent<..>` JS handles were declared after `Runtime`. Reordered fields (handles first). Compiler cannot catch this FFI drop-order rule. | 3
- hook-deno | runtime panic "V8 posted a delayed task, but this isolate was created outside of a tokio runtime" | create tokio runtime first and `enter()` it while constructing JsRuntime | 2
- hook-deno | loader rejected the hook module itself (resolve() is called for side modules too) | allow only specifiers the engine registered | 2

## Part B (architecture slice)
- First `cargo check --workspace --all-targets` of all 10 crates (~1,300 production + ~340 test code lines at that point): **0 errors, 0 borrow-checker errors**, 22 warnings (21 `unreachable_pub`, 1 unused import).
- First `cargo test --workspace`: 13/13 passed (incl. the mock-server singleflight proof) without code changes.
- No lifetime, `Send`, or `'static` errors from `tokio::spawn`: tasks capture only `Arc<RunContext>`, a `TestId` (Copy) and channel handles, so nothing borrowed crosses a spawn.
- Places where the design *avoided* friction rather than fought it (decided up front, not after an error):
  - Graph edges are `TestId(u32)` indices into `Program.tests`, never `&Test`.
  - `Program` is never mutated after `ProgramBuilder::build`; shared as `Arc<Program>`.
  - QuickJS `Runtime` is `!Send`; it lives on its own OS thread behind an mpsc + oneshot channel, so the async `HookRunner` trait object stays `Send + Sync`.
  - SQLite `Connection` is owned by one writer thread; callers hold only an `mpsc::Sender`.
  - Setup singleflight uses one `std::sync::Mutex<HashMap<key, Arc<OnceCell>>>`, locked only to fetch the slot (never across `.await`).

## Part C (lint gates) — not borrow-checker, but the friction an agent hits
- clippy round 1: 4 findings (2 `missing_const_for_fn`, 1 `or_fun_call`, 1 `unwrap_used` in an integration-test helper).
- round 2: 25 findings: 17 `unreachable_pub` (my extra rustc lint), 5 `doc_markdown`, 1 `option_if_let_else`, 1 `too_many_arguments` (5/4, real smell in compiler stub), 1 `many_single_char_names` (test).
- round 3: 13 `redundant_pub_crate` — caused by fixing round 2 with `pub(super)`. rustc `unreachable_pub` and clippy nursery `redundant_pub_crate` contradict each other for items in private modules; resolved by dropping `unreachable_pub` from the config (net zero code change).
- round 4: 3 `unwrap_used` in `cli/tests` helper fn: `allow-unwrap-in-tests` covers `#[test]` fns and `#[cfg(test)]` modules only, not helper fns of integration-test crates. Fixed by returning `io::Result`.
- round 5: green. round 6 (after enabling `clippy::cognitive_complexity`, which is NOT part of pedantic/nursery in clippy 0.1.97): green.
- cargo-mutants on engine: 11 caught / 4 missed / 5 unviable (73%). Added 2 tests (failing ResultStore; concurrency 0/1) and removed a magic `* 2` channel capacity → 13/13 caught (100%).
