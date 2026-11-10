//! Thresholds: the load metrics, judged with the run's matchers.

use indexmap::IndexMap;
use serde_json::Value;
use vc_executor::Judge;
use vc_model::{LoadScenario, Matcher, TestId};

use crate::{LoadError, Tally, Totals};

/// One threshold that did not hold.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Breach {
    /// The mix test whose own threshold it is; `None` for a scenario threshold.
    pub test: Option<TestId>,
    /// The metric, as written (`p95`).
    pub metric: String,
    /// What the metric measured: milliseconds for latencies, a share for `errorRate`, per
    /// second for `rps`.
    pub actual: f64,
}

/// Reads one metric from iterations over a span of seconds.
type MetricFn = fn(&Tally, u64) -> f64;

/// The metrics a threshold names, by name (syntax reference "Load files").
const METRICS: &[(&str, MetricFn)] = &[
    ("p50", |tally, _| tally.latencies().quantile_ms(0.50)),
    ("p90", |tally, _| tally.latencies().quantile_ms(0.90)),
    ("p95", |tally, _| tally.latencies().quantile_ms(0.95)),
    ("p99", |tally, _| tally.latencies().quantile_ms(0.99)),
    ("max", |tally, _| tally.latencies().max_ms()),
    ("errorRate", |tally, _| ratio(tally.errors(), tally.count())),
    ("rps", |tally, seconds| ratio(tally.count(), seconds)),
];

/// The thresholds of `scenario` that `totals` breach: the scenario's on every mix test
/// together, then each mix entry's on that test alone, in written order.
///
/// `judge` renders the matchers' arguments; a latency argument such as `200ms` compares in
/// milliseconds.
///
/// # Errors
///
/// [`LoadError::Threshold`] for a metric the engine does not measure or a matcher that cannot
/// be applied; [`LoadError::Histogram`] if an empty tally cannot be made.
pub fn breaches(scenario: &LoadScenario, totals: &Totals, judge: &Judge<'_>) -> Result<Vec<Breach>, LoadError> {
    let mut found = Vec::new();
    let mut check = |test: Option<TestId>, thresholds: &IndexMap<String, Matcher>, tally: &Tally| {
        for (metric, matcher) in thresholds {
            if let Some(actual) = breach(judge, (metric, matcher), (tally, totals.seconds()))? {
                found.push(Breach { test, metric: metric.clone(), actual });
            }
        }
        Ok::<(), LoadError>(())
    };
    check(None, &scenario.thresholds, totals.all())?;
    let idle = Tally::new()?;
    for entry in scenario.mix.iter().filter(|entry| !entry.thresholds.is_empty()) {
        check(Some(entry.test), &entry.thresholds, totals.test(entry.test).unwrap_or(&idle))?;
    }
    Ok(found)
}

/// Whether any of `breaches` is of a metric `scenario` aborts on.
#[must_use]
pub fn aborts(scenario: &LoadScenario, breaches: &[Breach]) -> bool {
    breaches.iter().any(|breach| scenario.abort_on.contains(&breach.metric))
}

/// What `metric` measured over `span` (iterations, seconds), if `matcher` rejects it.
fn breach(
    judge: &Judge<'_>,
    (metric, matcher): (&str, &Matcher),
    span: (&Tally, u64),
) -> Result<Option<f64>, LoadError> {
    let refuse = |message: String| LoadError::Threshold { metric: metric.to_owned(), message };
    let (_, read) =
        METRICS.iter().find(|(name, _)| *name == metric).ok_or_else(|| refuse("not a load metric".to_owned()))?;
    let actual = read(span.0, span.1);
    let holds = judge.matches(matcher, Some(&Value::from(actual))).map_err(refuse)?;
    Ok((!holds).then_some(actual))
}

#[expect(clippy::cast_precision_loss, reason = "a rate needs no more than f64's 52 bits of precision")]
fn ratio(part: u64, whole: u64) -> f64 {
    if whole == 0 { 0.0 } else { part as f64 / whole as f64 }
}

#[cfg(test)]
#[path = "threshold_tests.rs"]
mod tests;
