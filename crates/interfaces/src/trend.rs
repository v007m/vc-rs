//! How a test or an api did across stored runs (G15): the History tab's flaky and failing tests,
//! and Insights' slowest apis.

use std::collections::BTreeMap;

use crate::ExchangeRow;

/// One test over the newest runs of a window, from [`crate::HistoryReader::trends`].
#[derive(Clone, Debug, Default, PartialEq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "history row built and read across crates; every user is in this repo (D160)"
)]
pub struct TestTrend {
    /// Its full ID: `users.crud#getsUser`.
    pub test_id: String,
    /// Its results that ran in the window, each `examples` row and repeat counted; skipped
    /// results are left out.
    pub results: u32,
    /// How many of them failed or were blocked.
    pub failed: u32,
    /// How many of them were flaky.
    pub flaky: u32,
    /// Their mean wall time, in milliseconds.
    pub mean_ms: f64,
}

/// One api over the newest runs of a window, from [`crate::HistoryReader::api_trends`]: the
/// Insights screen's slowest apis, and the statuses its coverage counts as exercised.
#[derive(Clone, Debug, Default, PartialEq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "history row built and read across crates; every user is in this repo (D160)"
)]
pub struct ApiTrend {
    /// Its full ID: `users#get`.
    pub api_id: String,
    /// The method of its first call given, the newest one from a store.
    pub method: String,
    /// Its calls in the window, those that got no response included.
    pub calls: u32,
    /// The median total time of the calls that got a response, in milliseconds, by nearest
    /// rank; none when none did.
    pub p50_ms: Option<f64>,
    /// Their 95th percentile, likewise.
    pub p95_ms: Option<f64>,
    /// The statuses its responses had, each once, lowest first.
    pub statuses: Vec<u16>,
}

/// An api's trend so far, with the total times of its calls that got a response.
type Tally = (ApiTrend, Vec<f64>);

/// Each api's trend over `calls`, in api ID order; a call of no api is left out. A call that got
/// no response counts in `calls` but not in the percentiles.
#[must_use]
pub fn api_trends(calls: &[ExchangeRow]) -> Vec<ApiTrend> {
    let mut by_api: BTreeMap<&str, Tally> = BTreeMap::new();
    for call in calls {
        let Some(api_id) = call.api_id.as_deref() else { continue };
        let (trend, times) = by_api.entry(api_id).or_insert_with(|| {
            (ApiTrend { api_id: api_id.to_owned(), method: call.method.clone(), ..ApiTrend::default() }, Vec::new())
        });
        trend.calls = trend.calls.saturating_add(1);
        if let Some(status) = call.status {
            times.push(call.timing.total_ms);
            trend.statuses.push(status);
        }
    }
    by_api.into_values().map(|(trend, times)| summed(trend, times)).collect()
}

/// `trend` with its percentiles taken over `times` and its statuses sorted, each once.
fn summed(trend: ApiTrend, mut times: Vec<f64>) -> ApiTrend {
    times.sort_by(f64::total_cmp);
    let mut statuses = trend.statuses;
    statuses.sort_unstable();
    statuses.dedup();
    ApiTrend { p50_ms: nearest_rank(&times, 50), p95_ms: nearest_rank(&times, 95), statuses, ..trend }
}

/// The `percent`th percentile of `sorted` by nearest rank: the smallest value at or above that
/// share of them; none when it is empty.
fn nearest_rank(sorted: &[f64], percent: usize) -> Option<f64> {
    let rank = sorted.len().saturating_mul(percent).div_ceil(100);
    sorted.get(rank.checked_sub(1)?).copied()
}

#[cfg(test)]
mod tests {
    use super::{ApiTrend, api_trends, nearest_rank};
    use crate::{ExchangeRow, TimingRow};

    fn call(api_id: Option<&str>, status: Option<u16>, total_ms: f64) -> ExchangeRow {
        let timing = TimingRow { total_ms, ..TimingRow::default() };
        ExchangeRow {
            api_id: api_id.map(str::to_owned),
            method: "GET".into(),
            status,
            timing,
            ..ExchangeRow::default()
        }
    }

    #[test]
    fn the_nearest_rank_is_the_smallest_value_at_or_above_the_share() {
        let twenty: Vec<f64> = (1..=20).map(f64::from).collect();
        assert_eq!((nearest_rank(&twenty, 50), nearest_rank(&twenty, 95)), (Some(10.0), Some(19.0)));
        assert_eq!((nearest_rank(&[7.0], 50), nearest_rank(&[7.0], 95)), (Some(7.0), Some(7.0)));
        assert_eq!(nearest_rank(&[], 50), None);
    }

    #[test]
    fn each_api_counts_every_call_but_times_only_the_answered_ones() {
        let calls = [
            call(Some("users#list"), Some(500), 40.0),
            call(Some("users#get"), Some(200), 30.0),
            call(Some("users#list"), None, 9_000.0),
            call(None, Some(200), 1.0),
            call(Some("users#list"), Some(200), 10.0),
            call(Some("users#list"), Some(200), 20.0),
        ];
        let trend = |api_id: &str, calls, p50_ms, p95_ms, statuses: &[u16]| ApiTrend {
            api_id: api_id.into(),
            method: "GET".into(),
            calls,
            p50_ms,
            p95_ms,
            statuses: statuses.to_vec(),
        };
        assert_eq!(
            api_trends(&calls),
            [
                trend("users#get", 1, Some(30.0), Some(30.0), &[200]),
                trend("users#list", 4, Some(20.0), Some(40.0), &[200, 500]),
            ]
        );
        assert_eq!(api_trends(&[call(Some("users#get"), None, 5.0)]), [trend("users#get", 1, None, None, &[])]);
    }
}
