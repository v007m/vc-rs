# Spikes (reference only; not part of the build)

| Folder | What it is |
| --- | --- |
| `rust-feasibility/partb` | The 10-crate Rust slice of the engine: IR, compiler, planner, executor with shared setup, SQLite writer, and the hook bridge (oxc + QuickJS). It also holds the quality-gate set-up: `gates.sh`, `clippy.toml`, `deny.toml`, `rustfmt.toml`, workspace lints and `arch-tests`. **M0 starts from this workspace.** |
| `rust-feasibility/parta` | The experiment comparing QuickJS and V8 (deno_core) as the hook runtime. |
| `rust-feasibility/logs/borrow_checker_log.md` | A record of every borrow-checker problem met in the spike, and the design rule that avoided each one. |
| `rust-feasibility/raw`, `results/` | Raw measurements and the assembled results (`rust_feasibility.md`) behind `docs/design/language-decision.md`. |
| `language-spike/` | The earlier TypeScript, Go and Rust comparison: the Go mock server that counts requests (`mock-server/main.go`), the 1,000-file fixture generator (`generator/generate.py`), the Rust implementation, and the benchmark and scoring scripts. |

Build outputs and generated fixtures were not copied. Run `python3 language-spike/generator/generate.py` to recreate the fixtures. Some scripts still use absolute paths from the original cloud workspace (`/home/claude/vibranium-spike/...`).

The spike workspace says `license = "MIT"`. The project is licensed Apache-2.0, so change it in M0.
