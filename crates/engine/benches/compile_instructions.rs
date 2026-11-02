//! Instruction counts of `vc check` on a 1,000-file project (D120): stable enough to gate every
//! PR on, unlike wall-clock time. gungraun runs the bench under valgrind, so it is Linux only;
//! elsewhere the bench says so and exits cleanly.

#[cfg(target_os = "linux")]
mod fixture;

#[cfg(target_os = "linux")]
use fixture::{Bench, Scenario};
#[cfg(target_os = "linux")]
use gungraun::prelude::*;

/// Each argument is built before the measured part starts, so only the check is counted.
#[cfg(target_os = "linux")]
#[library_benchmark]
#[bench::cold_1000(Bench::prepare().engine(Scenario::Cold))]
#[bench::warm_1000(Bench::prepare().engine(Scenario::Warm))]
#[bench::changed_1000(Bench::prepare().engine(Scenario::Changed))]
fn check(engine: vc_engine::Engine) -> (vc_engine::Engine, usize) {
    let count = fixture::check(&engine);
    (engine, std::hint::black_box(count))
}

#[cfg(target_os = "linux")]
library_benchmark_group!(name = compile, benchmarks = check);

#[cfg(target_os = "linux")]
main!(library_benchmark_groups = compile);

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("skipped: the instruction-count benches need valgrind, which runs on Linux only");
}
