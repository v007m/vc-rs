//! Writing the folder-layout HTML report of a 10,000-test run (D169: under a second).

use std::hint;

use criterion::{Criterion, criterion_group, criterion_main};
use vc_adapter_reporters::{FormatRegistry, FsDestination, HtmlLayout, sample_snapshot};

fn html(c: &mut Criterion) {
    let snapshot = sample_snapshot(10_000);
    let Ok(format) = FormatRegistry::builtin(HtmlLayout::Folder).get("html") else { return };
    let Ok(dir) = tempfile::tempdir() else { return };
    let mut group = c.benchmark_group("report");
    group.sample_size(10);
    group.bench_function("html_10k", |b| {
        b.iter(|| hint::black_box(format.render(&snapshot, &FsDestination::new(dir.path().join("out")))));
    });
    group.finish();
}

criterion_group!(benches, html);
criterion_main!(benches);
