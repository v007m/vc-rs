//! Wall-clock time of `vc check` on a 1,000-file project: cold, warm and with one file changed.
//! `scripts/bench-budgets.sh` holds the medians to the D120 budgets.

use std::hint::black_box;

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};

mod fixture;

use fixture::{Bench, Scenario};

fn compile(c: &mut Criterion) {
    let bench = Bench::prepare();
    let mut group = c.benchmark_group("compile");
    group.sample_size(20);
    for (id, scenario) in
        [("cold_1000", Scenario::Cold), ("warm_1000", Scenario::Warm), ("changed_1000", Scenario::Changed)]
    {
        group.bench_function(id, |b| {
            b.iter_batched(
                || bench.engine(scenario),
                |engine| {
                    let count = fixture::check(&engine);
                    (engine, black_box(count))
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, compile);
criterion_main!(benches);
