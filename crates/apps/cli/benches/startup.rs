//! Wall-clock time of `vc --version`, the startup cost of every command (D120 budget: 20 ms).

use std::hint::black_box;
use std::process::{Command, Output};

use criterion::{Criterion, criterion_group, criterion_main};

fn version() -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_vc")).arg("--version").output()
}

fn startup(c: &mut Criterion) {
    match version() {
        Ok(output) if output.status.success() => {}
        Ok(output) => exit_with(&format!("`vc --version` exited with {}", output.status)),
        Err(error) => exit_with(&format!("`vc --version` did not start: {error}")),
    }
    let mut group = c.benchmark_group("startup");
    group.bench_function("version", |b| b.iter(|| black_box(version())));
    group.finish();
}

/// A bench that cannot run its command has nothing to measure, so it says why and stops.
fn exit_with(reason: &str) {
    eprintln!("bench setup failed: {reason}");
    std::process::exit(1);
}

criterion_group!(benches, startup);
criterion_main!(benches);
