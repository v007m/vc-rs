# Vibranium v2 — Language Decision

Oct 22, 2026 · @Sarath

**Decision (23 October 2026): Rust**, with TypeScript hooks running in an embedded QuickJS engine. The fair rerun below put TypeScript slightly ahead on the original maintainability-heavy weights, but Rust ahead once performance and a single small binary were weighted as Sarath asked (D46). The open risks for Rust were the borrow checker, typed TypeScript hooks and clean-code enforcement, so a focused feasibility spike tested exactly those. It cleared all three; its results are in the next section. The TypeScript analysis further down is kept as the record of how the decision was made.

## Rust feasibility spike

The spike built the riskiest parts of the engine in Rust under the rules the real project would use, on the same 2-vCPU VM as the benchmarks.

| Question | Result |
| --- | --- |
| Can users write hooks in typed TypeScript? | Yes. Hooks are type-checked with `tsc` against a types-only `@vibranium/sdk`; at load, oxc strips the types and the hook runs in QuickJS (rquickjs). Errors point at the original TypeScript lines. |
| What do hooks cost? | QuickJS adds 4.24 MB to the binary and 3.2 ms to start; a `beforeRequest` hook call takes 20 µs and a custom check 36 µs, most of it JSON conversion. V8 (deno\_core) also worked but added 53 MB, 25 ms of startup and was no faster per call. |
| Are hooks sandboxed? | Yes: a 200 ms time budget stopped an infinite loop at 200.3 ms, a 64 MB memory cap held, and file, network and import access are denied by default. One caveat: a single long built-in call (a pathological regex) can overrun the budget before it is stopped. |
| Does the borrow checker get in the way? | Not with the chosen design. A 10-crate, about 1,300-line slice of the engine (IR, compiler, planner, executor with shared setup, SQLite writer) compiled with zero borrow-checker errors on the first `cargo check`, with no explicit lifetimes in any public API, one `Mutex`, no `unsafe` and no global state. The design avoids the friction on purpose: IDs instead of references, an immutable shared program, channels between parts, and the JS engine on its own thread. |
| Does shared setup really run once? | Yes. Against a mock server, 50 tests sharing one file-level setup sent exactly 1 setup request; with memoization switched off, the test catches 50. |
| Can clean code be enforced by machine? | Mostly. One script (`gates.sh`, 17 s) runs clippy pedantic with function length ≤ 40 lines, complexity ≤ 10 and ≤ 3 arguments, plus duplication detection (jscpd), an architecture test of crate dependencies, cargo-deny, cargo-machete and mutation testing. Each gate was proven to fail on deliberately bad code. Naming, shotgun surgery and speculative generality still need a short human review checklist. |
| How fast is the edit loop? | `cargo check` after an edit 0.8 s, the engine's tests 0.8 s, the whole workspace's tests 4.6 s; a cold release build takes about 4 minutes. |

Conditions that come with the decision:

1. Hooks run in QuickJS, not V8.
2. The design rules that kept the borrow checker quiet are machine-checked and written into the LLD.
3. `gates.sh` is a required CI check, and coding agents may not change the lint or architecture-test configuration.
4. Clippy's `cognitive_complexity` lint is switched on explicitly: in the current Clippy it is not part of pedantic, so a complexity limit is otherwise silently ignored.
5. A short review checklist covers the code smells no tool catches.

What the spike did not prove: the slice is small, async hooks were not benchmarked, and the per-call figures come from a slow VM.

## Scores

Each criterion is scored 1–5 from evidence in the spike and weighted per D41; totals are out of 100.

| Criterion (weight) | TypeScript (Bun) | TypeScript (Node 26) | Go | Rust | Evidence |
| --- | --- | --- | --- | --- | --- |
| Code quality and type strictness (30) | 4 | 4 | 3 | 5 | Rust's compiler caught 8 bugs at build time; Go let two type bugs through to run time |
| Library support (25) | 4 | 4 | 5 | 4 | All have maintained YAML, JSON Schema, CEL, protobuf, Connect, MCP, TUI, SQLite and OAuth libraries; Go's are the most uniform |
| Extensibility (20) | 5 | 5 | 3 | 3 | Only TypeScript loads users' typed hooks at run time from a single binary (verified on Bun and on a Node 26 single binary) |
| Compile speed (10) | 4 | 4 | 5 | 5 | 492 ms (Bun), 531 ms (Node 26), 352 ms (Go), 387 ms (Rust) for 1,000 files |
| CEL and hook overhead (10) | 5 | 5 | 3 | 2 | 0.9–1.3 µs (TypeScript) vs 3.37 µs (Go) vs 4.76 µs (Rust) per evaluation |
| Load throughput (5) | 4 | 3 | 4 | 5 | 25.7k, 19.7k, 25.4k and 40.3k requests/s at 5,000 users, 256 connections each |
| **Weighted total** | **86** | **85** | **75** | **81** |  |

## Measurements

All three implementations passed the correctness proof and sent the identical 8,181 requests for the full 5,668-test suite; they differ in speed, size and build loop.

| Measure | TypeScript (Bun) | Go | Rust |
| --- | --- | --- | --- |
| Compile 1,000 files: wall time, peak memory | 492 ms, 92 MB | 352 ms, 68 MB | 387 ms, 68 MB |
| CEL, 1 million evaluations | 1.29 µs/op | 3.37 µs/op | 4.76 µs/op |
| Load, 1,000 users: req/s, p99 | 22.2k, 86 ms | 17.2k, 112 ms | 29.8k, 152 ms |
| Load, 5,000 users: req/s, p99, peak memory | 20.6k, 356 ms, 124 MB | 16.6k, 541 ms, 534 MB | 26.9k, 819 ms, 296 MB |
| CPU per 1,000 requests at 5,000 users | 48 ms | 59 ms | 37 ms |
| Full suite (5,668 tests, 8,181 requests) | 748 ms | 921 ms | 502 ms |
| Startup `--version`, binary size | 9.3 ms, 81 MB | 9.5 ms,&#32; 23 MB | 2.1 ms, 16 MB |
| Lines of code (same features) | 876 | 1,561 | 1,344 |
| Incremental build after one change | 0.5 s (binary) | 0.17 s | 75 s release; 1.4 s debug |

Correctness proof, identical in all three: 50 tests sharing a file-scoped given caused exactly 1 create request; a run-scoped given shared by 20 tests caused 1; `repeat: 5` with a test-scoped given caused 5 creates and 5 requests. Load runs had zero errors.

Diagnostics: all three printed file, line and column with "did you mean" suggestions on the broken project. TypeScript added one redundant error for a misspelled `expect` key.

## Fair rerun

The first load comparison was unfair: Go and Node opened one connection per virtual user (5,000), while Bun was capped at 256 and Rust was unlimited. The rerun capped every client at 256 connections and added Node 26.12, the latest release. Its numbers replace the load figures above.

| At 5,000 users, 256 connections | Requests/s | p50 | p99 | CPU per 1,000 requests | Peak memory |
| --- | --- | --- | --- | --- | --- |
| Rust | 40.3k | 122 ms | 164 ms | 24 ms | 89 MB |
| TypeScript on Bun 1.4 | 25.7k | 193 ms | 274 ms | 38 ms | 121 MB |
| Go 1.24 | 25.4k | 178 ms | 307 ms | 39 ms | 346 MB |
| TypeScript on Node 26.12 | 19.7k | 244 ms | 381 ms | 50 ms | 260 MB |
| TypeScript on Node 22.22 | 17.8k | 266 ms | 520 ms | 56 ms | 248 MB |

At 1,000 users the order is the same: Rust 43.9k, Go 29.3k, Bun 28.8k, Node 26 21.9k, Node 22 21.0k requests/s. All runs had zero errors; figures are medians of 3 runs of 30 s.

What changed: Go now matches Bun on throughput, so its earlier load score was a configuration artefact, not a language limit. Node 26 is about 10% faster than Node 22 under load and reaches about 77% of Bun's throughput.

*The next two sections record the case for TypeScript as it stood before the Rust spike. They remain useful as the fallback plan if Rust stalls.*

## Why TypeScript wins, and where it loses

TypeScript wins on the things a solo maintainer and plugin authors live with every day; it loses on raw efficiency and strictness.

Where it wins:

- **Extensibility.** Users write code hooks in typed TypeScript and the Bun single binary loads them at run time; Go and Rust would need an embedded JavaScript engine or a separate process for the same thing.
- **CEL speed.** The fastest of the three at 1.29 µs per evaluation, because the TypeScript CEL library handles untyped JSON well; the Go and Rust CEL libraries are slower on untyped bodies.
- **Less code.** 876 lines for the same features, about 35% less than Rust and 44% less than Go.
- **Fast loop.** 0.5 s to rebuild the binary and 0.75 s to type-check, against 75 s for a Rust release build.
- **Maintainer fit.** Fluency in TypeScript makes reviewing AI-written code fast and reliable; with little Rust, borrow-checker and async disputes would stall work.

Where it loses:

- **Type strictness.** Rust caught 8 bugs at build time; TypeScript is strong but has escape hatches that lint rules must close.
- **Raw throughput.** With equal connection limits, Rust sends about 57% more requests per second than Bun at 5,000 users, using about a third less CPU per request and three quarters of the memory.
- **Binary size.** 81 MB against 16–23 MB; startup is still fast at 9.3 ms.
- **YAML with source positions is slow.** The position-keeping parser took about 2 s for 1,000 files on its own; the 492 ms result relies on parsing data fast and building positions only when there is an error.

## Runtime: Bun first, Node as standby

Bun is faster, smaller and quicker to start, but Node 26 is a workable fallback: the same code runs, ships as one file and loads TypeScript hooks. Because Anthropic acquired Bun in December 2025 (Bun stays MIT-licensed), the plan keeps the code runtime-neutral so either can be shipped.

| Measure | Bun 1.4 | Node 26.12 |
| --- | --- | --- |
| Load, 5,000 users: req/s, p99 (256 connections) | 25.7k, 274 ms | 19.7k, 381 ms |
| Peak memory under load | 121 MB | 260 MB |
| Compile 1,000 files | 492 ms | 531 ms |
| CEL, per evaluation | 1.29 µs | 0.90 µs |
| Single binary size | 81 MB | 153 MB |
| Startup `--version` | 7 ms | 30 ms (with code cache) |
| Loads a user's `.ts` hook from disk | Yes, with `import()` | Yes, with `createRequire` (type stripping) |
| Single-binary feature status | Stable | `node --build-sea`, added in 25.5, marked "active development" |
| Governance | Owned by Anthropic, MIT licence | OpenJS Foundation |

TypeScript 7 does not change these numbers: it speeds up type-checking (0.75 s for the spike) but runtime speed comes from Bun or Node. Startup times were measured as medians of 20 runs; the Node figures under load are medians of 3 runs.

## Sensitivity

TypeScript's lead over Rust is 4–5 points; Go stays last even after the fair rerun. These sensitivity results come from the first scoring (86 vs 81) and still apply, since the totals barely moved.

- Of 30 tested shifts of 10 weight points between criteria, Rust overtakes TypeScript in 7. Every one moves weight away from extensibility or CEL overhead and towards type strictness, compile speed, load or libraries.
- One score change alone also flips it: lowering TypeScript's code-quality score from 4 to 3 gives Rust the lead.
- Go does not win under any shift tested.

So the choice rests on two judgements: that runtime-loaded TypeScript hooks matter to users (extensibility, weight 20), and that lint rules can make TypeScript strict enough to keep its quality score at 4. If either proves wrong in practice, Rust is the fallback, and the IR and executor port make a later Rust executor possible without a rewrite of everything else.

## Conditions for the LLD

Superseded: the language decision moved to Rust (see the decision log, D36–D41, and lld.md). Kept for history.

The decision holds only if the low-level design adopts these, each of which came out of the spike:

1. **Bun as the shipping runtime, Node as standby** for binaries and load testing; code against Node-compatible APIs and web standards so the npm package still runs on Node; CI builds and tests a Node 26 single binary on every release, so switching runtime is a build change.
2. **Strict TypeScript:** `strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`, no `any`, branded IDs, and lint rules that ban unchecked casts. This keeps the quality score that the result depends on.
3. **Two-speed YAML parsing:** a fast parser for data, and the position-keeping parser (`yaml`) only for files with errors, plus the hash cache so unchanged files are never re-parsed.
4. **Libraries used or checked in the spike:** Zod 4 for definitions, Ajv for response schemas, `@marcbachmann/cel-js` for CEL, undici on Node and Bun's built-in client on Bun behind the `Transport` port, `@bufbuild/protobuf` and `@connectrpc/connect` for contracts, `@modelcontextprotocol/sdk` for MCP, and Bun's built-in SQLite (with `better-sqlite3` as the Node adapter).
5. **Connection limits set explicitly** per host in the transport, rather than relying on each runtime's default.
6. **Executor behind a port**, so a Rust executor could later run the same IR if load needs outgrow Bun.

## Method and caveats

Each language implemented the same slice: discover and parse 1,000 generated YAML files with source positions, validate, link with cycle and reference checks, plan, execute with memoized givens and singleflight, evaluate CEL and JSON Schema, and run a load mode, all against one shared mock server that counts requests.

- **Machine:** one cloud VM with 2 vCPUs (Xeon, 2.1 GHz) and 7.8 GiB RAM, Ubuntu 24.04. The mock server was pinned to one core and the client to the other.
- **Versions:** Bun 1.4.2, Node 22.22, TypeScript 7.0.2, Go 1.24.7, Rust 1.97.
- **Runs:** medians of 7 runs for compile, 5 for CEL and 3 for load; run-to-run spread was 5–10%.
- **Go setup:** the Go module proxy was blocked, so dependencies came straight from GitHub and a newer Go toolchain could not be installed.
- **Library effects:** results mix language and library quality, especially for CEL and YAML parsing.
- **CEL figure:** one representative expression on a 5-item body.
- **HTTP clients:** default connection limits differed (Bun 256 sockets, Go one per user, Rust uncapped), which affects the load comparison.
- **Scale:** on a 2-core machine absolute numbers are low; the ratios matter more than the totals.

The spike's code, generator, mock server, benchmark scripts, raw results (`results/raw.json`) and diagnostics samples are in `spike/` in this repository (the shared mock server and fixture generator under `spike/language-spike/`).
