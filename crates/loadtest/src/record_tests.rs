use std::time::Duration;

use vc_model::{StringTable, TestId};

use super::{Recorder, Sample, Tally};

fn ids() -> (TestId, TestId) {
    let mut strings = StringTable::new();
    (TestId::new(strings.intern("a.f#a").unwrap()), TestId::new(strings.intern("b.f#b").unwrap()))
}

fn ms(millis: u64) -> Duration {
    Duration::from_millis(millis)
}

/// Each row as (second, test, count, errors).
fn rows(recorder: &mut Recorder, before: u64) -> Vec<(u64, TestId, u64, u64)> {
    recorder
        .close(before)
        .into_iter()
        .map(|row| (row.second, row.test, row.tally.count(), row.tally.errors()))
        .collect()
}

#[test]
fn a_tally_counts_iterations_errors_and_latencies() {
    let mut tally = Tally::new().unwrap();
    tally.record(ms(10), false);
    tally.record(ms(30), true);
    let mut other = Tally::new().unwrap();
    other.record(ms(20), true);
    tally.merge(&other).unwrap();
    assert_eq!((tally.count(), tally.errors(), tally.latencies().count()), (3, 2, 3));
    assert!((tally.latencies().quantile_ms(0.5) - 20.0).abs() < 0.1);
}

#[test]
fn samples_close_as_one_row_per_test_per_second() {
    let (a, b) = ids();
    let mut recorder = Recorder::new();
    for sample in [
        Sample { test: b, second: 0, latency: ms(5), failed: false },
        Sample { test: a, second: 0, latency: ms(7), failed: true },
        Sample { test: a, second: 0, latency: ms(9), failed: false },
        Sample { test: a, second: 1, latency: ms(4), failed: false },
        Sample { test: a, second: 2, latency: ms(4), failed: false },
    ] {
        recorder.record(sample).unwrap();
    }
    assert_eq!(rows(&mut recorder, 2), [(0, a, 2, 1), (0, b, 1, 0), (1, a, 1, 0)]);
    assert_eq!(rows(&mut recorder, 2), []);
    assert_eq!(rows(&mut recorder, 3), [(2, a, 1, 0)]);
}

#[test]
fn a_late_sample_counts_in_the_oldest_open_second() {
    let (a, _) = ids();
    let mut recorder = Recorder::new();
    recorder.record(Sample { test: a, second: 0, latency: ms(1), failed: false }).unwrap();
    assert_eq!(rows(&mut recorder, 1).len(), 1);
    recorder.record(Sample { test: a, second: 0, latency: ms(1), failed: true }).unwrap();
    assert_eq!(rows(&mut recorder, 2), [(1, a, 1, 1)]);
}
