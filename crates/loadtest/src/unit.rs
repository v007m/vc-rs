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
        format!("{}{}", number(value * self.scale), self.suffix)
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

/// `value` with at most three decimals and no trailing zeros: `30`, `45.471`.
fn number(value: f64) -> String {
    let text = format!("{value:.3}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

#[cfg(test)]
#[path = "unit_tests.rs"]
mod tests;
