//! The units load metrics are measured in, and how a report shows them.

use serde_json::Value;

/// The unit of a load metric: what a report calls it, rounds it to and shows it as.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Unit {
    name: &'static str,
    /// What a shown value is multiplied by: 100 for a share shown as a percentage.
    scale: f64,
    /// What follows a shown value.
    suffix: &'static str,
    /// The decimal places a JSON value keeps.
    places: i32,
}

impl Unit {
    /// Milliseconds, for the latencies.
    pub const MS: Self = Self { name: "ms", scale: 1.0, suffix: " ms", places: 3 };
    /// A share of the iterations, from 0 to 1, shown as a percentage: `errorRate`.
    pub const SHARE: Self = Self { name: "share", scale: 100.0, suffix: "%", places: 4 };
    /// Iterations per second: `rps`.
    pub const PER_SECOND: Self = Self { name: "perSecond", scale: 1.0, suffix: "/s", places: 3 };

    /// The unit's name in JSON: `ms`, `share` or `perSecond`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.name
    }

    /// `value` rounded to the unit's places, for JSON: `47.711`, not `47.711000000000006`.
    #[must_use]
    pub fn round(self, value: f64) -> f64 {
        let factor = 10_f64.powi(self.places);
        (value * factor).round() / factor
    }

    /// `value` as a report shows it: `45.471 ms`, `100%`, `12.5/s`.
    #[must_use]
    pub fn show(self, value: f64) -> String {
        format!("{}{}", number(value * self.scale, PLACES), self.suffix)
    }

    /// A breached `actual` next to its threshold's `limit`: `31.25 ms (lt 30 ms)`. The actual
    /// gets more decimals, up to six, where three would show it as a number in the
    /// limit, or as 0 when it is not, so a breach never reads as a pass: `30.0004 ms (lte 30 ms)`.
    #[must_use]
    pub fn breach(self, actual: f64, limit: &Value) -> String {
        let (value, limits) = (actual * self.scale, numbers(limit));
        let reads_wrong = |places| {
            let text = number(value, places);
            (text == "0" && value.abs() > 0.0) || limits.iter().any(|limit| number(limit * self.scale, places) == text)
        };
        let places = (PLACES..MOST_PLACES).find(|&places| !reads_wrong(places)).unwrap_or(MOST_PLACES);
        format!("{}{} ({})", number(value, places), self.suffix, self.limit(limit))
    }

    /// A threshold's limit as a report shows it, each number in the unit: `lt 30 ms`,
    /// `lt 1%`, `gte 20/s, lt 50/s`.
    #[must_use]
    pub fn limit(self, limit: &Value) -> String {
        match limit {
            Value::Number(number) => number.as_f64().map_or_else(|| number.to_string(), |value| self.show(value)),
            Value::Array(items) => items.iter().map(|item| self.limit(item)).collect::<Vec<_>>().join(", "),
            Value::Object(fields) => {
                fields.iter().map(|(name, arg)| format!("{name} {}", self.limit(arg))).collect::<Vec<_>>().join(", ")
            }
            other => other.to_string(),
        }
    }
}

/// The decimals a report shows a value with.
const PLACES: usize = 3;

/// The most decimals a breached value gets so as not to read as its limit.
const MOST_PLACES: usize = 6;

/// `value` with at most `places` decimals and no trailing zeros: `30`, `45.471`.
fn number(value: f64, places: usize) -> String {
    let text = format!("{value:.places$}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Every number in a threshold's limit.
fn numbers(limit: &Value) -> Vec<f64> {
    match limit {
        Value::Number(number) => number.as_f64().into_iter().collect(),
        Value::Array(items) => items.iter().flat_map(numbers).collect(),
        Value::Object(fields) => fields.values().flat_map(numbers).collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
#[path = "unit_tests.rs"]
mod tests;
