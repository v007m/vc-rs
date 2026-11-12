//! The `vc-model` values the records carry, in their wire form. Counts past `u32::MAX` saturate;
//! durations become milliseconds.

use std::time::Duration;

use crate::results::v1 as wire;

impl From<vc_model::TestStatus> for wire::TestStatus {
    fn from(status: vc_model::TestStatus) -> Self {
        match status {
            vc_model::TestStatus::Passed => Self::Passed,
            vc_model::TestStatus::Failed => Self::Failed,
            vc_model::TestStatus::Blocked => Self::Blocked,
            vc_model::TestStatus::Flaky => Self::Flaky,
            vc_model::TestStatus::Skipped => Self::Skipped,
            vc_model::TestStatus::AllowedFailure => Self::AllowedFailure,
        }
    }
}

impl From<vc_model::RunStatus> for wire::RunStatus {
    fn from(status: vc_model::RunStatus) -> Self {
        match status {
            vc_model::RunStatus::Passed => Self::Passed,
            vc_model::RunStatus::Failed => Self::Failed,
            vc_model::RunStatus::Cancelled => Self::Cancelled,
        }
    }
}

impl From<vc_model::CheckKind> for wire::CheckKind {
    fn from(kind: vc_model::CheckKind) -> Self {
        match kind {
            vc_model::CheckKind::Status => Self::Status,
            vc_model::CheckKind::Header => Self::Header,
            vc_model::CheckKind::Schema => Self::Schema,
            vc_model::CheckKind::Match => Self::Match,
            vc_model::CheckKind::Equals => Self::Equals,
            vc_model::CheckKind::Size => Self::Size,
            vc_model::CheckKind::Timing => Self::Timing,
            vc_model::CheckKind::Cel => Self::Cel,
            vc_model::CheckKind::Custom => Self::Custom,
        }
    }
}

impl From<vc_model::Protocol> for wire::Protocol {
    fn from(protocol: vc_model::Protocol) -> Self {
        match protocol {
            vc_model::Protocol::Http1 => Self::Http1,
            vc_model::Protocol::Http2 => Self::Http2,
        }
    }
}

impl From<vc_model::Totals> for wire::Totals {
    fn from(totals: vc_model::Totals) -> Self {
        Self {
            passed: count(totals.passed),
            failed: count(totals.failed),
            blocked: count(totals.blocked),
            flaky: count(totals.flaky),
            skipped: count(totals.skipped),
            allowed_failures: count(totals.allowed_failures),
        }
    }
}

impl From<vc_model::Timing> for wire::Timing {
    fn from(timing: vc_model::Timing) -> Self {
        Self {
            dns_ms: timing.dns.map(millis),
            connect_ms: timing.connect.map(millis),
            tls_ms: timing.tls.map(millis),
            first_byte_ms: millis(timing.first_byte),
            download_ms: millis(timing.download),
            total_ms: millis(timing.total),
        }
    }
}

/// `n` as a wire count, saturating at `u32::MAX`.
#[must_use]
pub fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// `duration` in wire milliseconds.
#[must_use]
pub fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

#[cfg(test)]
#[path = "convert_tests.rs"]
mod tests;
