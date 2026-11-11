use serde_json::Number;
use vc_diagnostics::{Diagnostic, Fix, TextEdit};
use vc_model::{Duration, Size, SourceSpan, StatusPattern};

use crate::codes;

/// Duration units and their length in milliseconds.
const DURATION_UNITS: &[(&str, u64)] = &[("ms", 1), ("s", 1_000), ("m", 60_000), ("h", 3_600_000)];
/// Size units and their bytes; `kb` is 1024 bytes.
const SIZE_UNITS: &[(&str, u64)] = &[("b", 1), ("kb", 1 << 10), ("mb", 1 << 20), ("gb", 1 << 30)];

/// Parses a duration written with its unit: `250ms`, `2s`, `1m`, `12h`.
///
/// # Errors
///
/// V2301 when the text is not a whole number followed by `ms`, `s`, `m` or `h`. A bare
/// number gets a certain fix that adds `ms`.
pub fn parse_duration(text: &str, span: SourceSpan) -> Result<Duration, Box<Diagnostic>> {
    let millis = with_unit(text, DURATION_UNITS).ok_or_else(|| {
        let diagnostic = Diagnostic::new(codes::UNIT, span, format!("invalid duration `{text}`"))
            .help("write a whole number and a unit: `250ms`, `2s`, `1m` or `12h`");
        Box::new(match bare(text, "ms", span) {
            Some(fix) => diagnostic.fix(fix),
            None => diagnostic,
        })
    })?;
    Ok(Duration::new(std::time::Duration::from_millis(millis)))
}

/// Parses a size written with its unit: `512b`, `4kb`, `1mb`, `2gb` (powers of 1024).
///
/// # Errors
///
/// V2301 when the text is not a whole number followed by `b`, `kb`, `mb` or `gb`. A bare
/// number gets a certain fix that adds `b`.
pub fn parse_size(text: &str, span: SourceSpan) -> Result<Size, Box<Diagnostic>> {
    with_unit(text, SIZE_UNITS).map(Size::from_bytes).ok_or_else(|| {
        let diagnostic = Diagnostic::new(codes::UNIT, span, format!("invalid size `{text}`"))
            .help("write a whole number and a unit: `512b`, `4kb`, `1mb` or `2gb`");
        Box::new(match bare(text, "b", span) {
            Some(fix) => diagnostic.fix(fix),
            None => diagnostic,
        })
    })
}

/// The certain fix that adds `unit` to a bare whole number.
fn bare(text: &str, unit: &str, span: SourceSpan) -> Option<Fix> {
    let bare = text.bytes().all(|b| b.is_ascii_digit()) && text.parse::<u64>().is_ok();
    bare.then(|| Fix::certain(format!("Write {text}{unit}"), vec![TextEdit::replace(span, format!("{text}{unit}"))]))
}

/// Parses a rate as a share from 0 to 1: a percentage from `0%` to `100%` (`1%`, `0.5%`) or
/// the share itself (`0.01`).
///
/// # Errors
///
/// V2301 when the text is neither, or is out of range.
pub fn parse_rate(text: &str, span: SourceSpan) -> Result<Number, Box<Diagnostic>> {
    let (digits, scale) = text.strip_suffix('%').map_or((text, 1.0), |digits| (digits, 100.0));
    let share = decimal(digits).map(|value| value / scale).filter(|share| (0.0..=1.0).contains(share));
    share.and_then(Number::from_f64).ok_or_else(|| {
        Box::new(
            Diagnostic::new(codes::UNIT, span, format!("invalid rate `{text}`"))
                .help("write a percentage from 0% to 100% (`1%`) or a share from 0 to 1 (`0.01`)"),
        )
    })
}

/// An unsigned decimal number: digits and at most one point.
fn decimal(text: &str) -> Option<f64> {
    let plain = text.bytes().all(|byte| byte.is_ascii_digit() || byte == b'.');
    if plain { text.parse().ok() } else { None }
}

/// Parses an expected status: a code from 100 to 599 (`201`) or a class (`2xx`).
///
/// # Errors
///
/// V2303 for anything else.
pub fn parse_status(text: &str, span: SourceSpan) -> Result<StatusPattern, Box<Diagnostic>> {
    StatusPattern::parse(text).ok_or_else(|| {
        Box::new(
            Diagnostic::new(codes::STATUS, span, format!("invalid status `{text}`"))
                .help("write a code from 100 to 599 or a class from 1xx to 5xx"),
        )
    })
}

fn with_unit(text: &str, units: &[(&str, u64)]) -> Option<u64> {
    let split = text.find(|c: char| !c.is_ascii_digit())?;
    let (number, unit) = text.split_at(split);
    let scale = units.iter().find(|(name, _)| *name == unit)?.1;
    number.parse::<u64>().ok()?.checked_mul(scale)
}
