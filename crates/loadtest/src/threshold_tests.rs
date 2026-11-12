use std::time::Duration as StdDuration;

use indexmap::IndexMap;
use serde_json::json;
use vc_executor::{Judge, MatcherRegistry};
use vc_model::{Duration, FileId, LoadScenario, Matcher, MatcherArg, MixEntry, StringTable, TemplateValue, TestId};

use super::{Breach, aborts, breaches};
use crate::test_support::{Literals, call, number, thresholds, under_ms};
use crate::{Interval, LoadError, RATE_SECONDS, Recorder, Sample, Totals, Unit, WINDOW_SECONDS, Window};

struct Fixture {
    fast: TestId,
    slow: TestId,
    scenario: LoadScenario,
}

fn fixture(scenario: IndexMap<String, Matcher>, slow: IndexMap<String, Matcher>) -> Fixture {
    let mut strings = StringTable::new();
    let fast = TestId::new(strings.intern("a.f#fast").unwrap());
    let slow_id = TestId::new(strings.intern("a.f#slow").unwrap());
    let mix = vec![
        MixEntry { test: fast, weight: 3, thresholds: IndexMap::new() },
        MixEntry { test: slow_id, weight: 1, thresholds: slow },
    ];
    let zero = Duration::new(StdDuration::ZERO);
    let id = FileId::new(strings.intern("a.peak").unwrap());
    let scenario = LoadScenario {
        id,
        name: None,
        mix,
        stages: Vec::new(),
        think_time: (zero, zero),
        full_checks: false,
        thresholds: scenario,
        abort_on: vec!["errorRate".to_owned()],
    };
    Fixture { fast, slow: slow_id, scenario }
}

/// Over 2 seconds: `fast` 30 iterations at 10 ms, `slow` 10 at 400 ms of which 2 failed.
fn totals(fixture: &Fixture) -> Totals {
    let mut recorder = Recorder::new();
    for i in 0..40_u64 {
        let (test, millis, failed) = if i % 4 == 3 { (fixture.slow, 400, i < 8) } else { (fixture.fast, 10, false) };
        recorder.record(Sample { test, second: i % 2, latency: StdDuration::from_millis(millis), failed }).unwrap();
    }
    let mut totals = Totals::new().unwrap();
    for row in recorder.close(2) {
        totals.add(&row).unwrap();
    }
    totals.cover(2);
    totals
}

fn judged(fixture: &Fixture, totals: &Totals) -> Result<Vec<Breach>, LoadError> {
    let registry = MatcherRegistry::standard();
    breaches(&fixture.scenario, totals, &Judge::new(&registry, &Literals))
}

/// A breach as (test, metric, actual rounded to a thousandth).
type Row<'a> = (Option<TestId>, &'a str, i64);

/// Each breach as a [`Row`].
fn summary(found: &[Breach]) -> Vec<Row<'_>> {
    #[expect(clippy::cast_possible_truncation, reason = "test values are small")]
    let round = |actual: f64| (actual * 1000.0).round() as i64;
    found.iter().map(|breach| (breach.test, breach.metric.as_str(), round(breach.actual))).collect()
}

#[test]
fn scenario_thresholds_cover_the_whole_mix_and_entry_thresholds_their_test() {
    let scenario = thresholds(&[
        ("p50", under_ms(50)),
        ("p90", under_ms(50)),
        ("errorRate", number("lt", 0.01)),
        ("rps", number("gte", 20.0)),
        ("max", under_ms(1000)),
    ]);
    let fixture = fixture(scenario, thresholds(&[("p95", under_ms(300)), ("rps", number("gte", 6.0))]));
    let found = judged(&fixture, &totals(&fixture)).unwrap();
    assert_eq!(
        summary(&found),
        [
            (None, "p90", 400_127),
            (None, "errorRate", 50),
            (Some(fixture.slow), "p95", 400_127),
            (Some(fixture.slow), "rps", 5_000)
        ]
    );
    let units: Vec<_> = found.iter().map(|breach| (breach.unit, breach.limit.clone())).collect();
    assert_eq!(
        units,
        [
            (Unit::MS, json!({"lt": 50.0})),
            (Unit::SHARE, json!({"lt": 0.01})),
            (Unit::MS, json!({"lt": 300.0})),
            (Unit::PER_SECOND, json!({"gte": 6.0}))
        ]
    );
    assert!(aborts(&fixture.scenario, &found));
    assert!(!aborts(&fixture.scenario, &found[..1]));
}

#[test]
fn a_test_that_did_not_run_is_judged_on_no_iterations() {
    let fixture = fixture(IndexMap::new(), thresholds(&[("rps", number("gte", 1.0)), ("p99", under_ms(1))]));
    let found = judged(&fixture, &Totals::new().unwrap()).unwrap();
    assert_eq!(summary(&found), [(Some(fixture.slow), "rps", 0)]);
}

#[test]
fn an_unknown_metric_or_unusable_matcher_is_an_error() {
    let unknown = fixture(thresholds(&[("p42", under_ms(1))]), IndexMap::new());
    let refused = judged(&unknown, &Totals::new().unwrap());
    assert!(matches!(refused, Err(LoadError::Threshold { metric, .. }) if metric == "p42"));
    let unusable = fixture(thresholds(&[("rps", call("lt", MatcherArg::Value(TemplateValue::Null)))]), IndexMap::new());
    assert!(matches!(judged(&unusable, &Totals::new().unwrap()), Err(LoadError::Threshold { .. })));
}

#[test]
fn totals_list_each_test_with_its_own_iterations() {
    let fixture = fixture(IndexMap::new(), IndexMap::new());
    let totals = totals(&fixture);
    let tests: Vec<(TestId, u64, u64)> =
        totals.tests().map(|(test, tally)| (test, tally.count(), tally.errors())).collect();
    assert_eq!(tests, [(fixture.fast, 30, 0), (fixture.slow, 10, 2)]);
}

#[test]
fn the_window_keeps_the_last_ten_seconds() {
    let fixture = fixture(IndexMap::new(), IndexMap::new());
    let mut recorder = Recorder::new();
    let mut window = Window::new();
    for second in 0..15 {
        recorder
            .record(Sample { test: fixture.fast, second, latency: StdDuration::from_millis(second + 1), failed: false })
            .unwrap();
        let rows: Vec<Interval> = recorder.close(second + 1);
        window.advance(second + 1, rows);
        let totals = window.totals().unwrap();
        assert_eq!(totals.seconds(), (second + 1).min(WINDOW_SECONDS));
        assert_eq!(totals.all().count(), (second + 1).min(WINDOW_SECONDS));
    }
    let totals = window.totals().unwrap();
    assert!((totals.all().latencies().quantile_ms(0.0) - 6.0).abs() < 0.01, "seconds 5 to 14 remain");
    assert_eq!(totals.test(fixture.fast).map(crate::Tally::count), Some(10));
    assert!(totals.test(fixture.slow).is_none());
}

#[test]
fn the_live_rate_follows_the_last_three_seconds() {
    let fixture = fixture(IndexMap::new(), IndexMap::new());
    let mut recorder = Recorder::new();
    let mut window = Window::new();
    let mut rates = Vec::new();
    for second in 0..6 {
        for _ in 0..=second {
            let latency = StdDuration::from_millis(1);
            recorder.record(Sample { test: fixture.fast, second, latency, failed: false }).unwrap();
        }
        window.advance(second + 1, recorder.close(second + 1));
        rates.push(window.rps());
    }
    assert_eq!(rates, [1, 2, 2, 3, 4, 5], "1, 3 and 6 over 1, 2 and 3 seconds; then 9, 12, 15 over 3");
    assert_eq!(RATE_SECONDS, 3);
}
