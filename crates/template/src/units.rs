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
        let bare = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
        let fix = Fix::certain(format!("Write {text}ms"), vec![TextEdit::replace(span, format!("{text}ms"))]);
        Box::new(if bare { diagnostic.fix(fix) } else { diagnostic })
    })?;
    Ok(Duration::new(std::time::Duration::from_millis(millis)))
}

/// Parses a size written with its unit: `512b`, `4kb`, `1mb`, `2gb` (powers of 1024).
///
/// # Errors
///
/// V2301 when the text is not a whole number followed by `b`, `kb`, `mb` or `gb`.
pub fn parse_size(text: &str, span: SourceSpan) -> Result<Size, Box<Diagnostic>> {
    with_unit(text, SIZE_UNITS).map(Size::from_bytes).ok_or_else(|| {
        Box::new(
            Diagnostic::new(codes::UNIT, span, format!("invalid size `{text}`"))
                .help("write a whole number and a unit: `512b`, `4kb`, `1mb` or `2gb`"),
        )
    })
}

/// Parses an expected status: a code from 100 to 599 (`201`) or a class (`2xx`).
///
/// # Errors
///
/// V2303 for anything else.
pub fn parse_status(text: &str, span: SourceSpan) -> Result<StatusPattern, Box<Diagnostic>> {
    let digits = |t: &&str| t.bytes().all(|b| b.is_ascii_digit());
    let class = text.strip_suffix("xx").filter(|d| d.len() == 1 && digits(d)).and_then(|d| d.parse::<u8>().ok());
    let code = Some(text).filter(|t| t.len() == 3 && digits(t)).and_then(|t| t.parse::<u16>().ok());
    let class = class.filter(|c| (1..=5).contains(c));
    let code = code.filter(|c| (100..=599).contains(c));
    class.map(StatusPattern::Class).or_else(|| code.map(StatusPattern::Code)).ok_or_else(|| {
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
