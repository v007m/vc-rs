//! What each built-in pipe does at run time (syntax reference "Converting types with pipes").
//! A pipe gives the reason it failed; the renderer adds the pipe and the value.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC};
use serde_json::{Number, Value};

/// Applies a pipe to a value.
pub type PipeFn = fn(&Value, &PipeArgs<'_>) -> Result<Value, String>;

/// A source of random choices; seeded by `--seed` so a run can be repeated.
pub trait Chance {
    /// A number from `0` up to, but not including, `bound`; `0` when `bound` is `0`.
    fn below(&self, bound: usize) -> usize;
}

/// What a pipe gets besides the value: its literal arguments and the random source.
#[derive(Clone, Copy)]
pub struct PipeArgs<'a> {
    /// The literal arguments, in order; the compiler checked their count and kinds.
    pub args: &'a [Value],
    /// For `random` and `shuffle`.
    pub chance: &'a dyn Chance,
}

/// RFC 3986 unreserved characters stay as they are.
const UNRESERVED: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

fn text(value: &Value) -> Result<&str, String> {
    value.as_str().ok_or_else(|| "it takes text".to_owned())
}

fn list(value: &Value) -> Result<&Vec<Value>, String> {
    value.as_array().ok_or_else(|| "it takes a list".to_owned())
}

fn parsed(value: &Value) -> Result<Number, String> {
    match value {
        Value::Number(number) => Ok(number.clone()),
        Value::String(text) => text.trim().parse().map_err(|_| "the text is not a number".to_owned()),
        _ => Err("it takes text or a number".to_owned()),
    }
}

/// `number`: `"42.5"` → `42.5`.
pub fn number(value: &Value, _: &PipeArgs<'_>) -> Result<Value, String> {
    parsed(value).map(Value::Number)
}

/// `int`: `"42"` or `42.9` → `42`, cutting off any fraction.
pub fn int(value: &Value, _: &PipeArgs<'_>) -> Result<Value, String> {
    let number = parsed(value)?;
    if number.is_i64() || number.is_u64() {
        return Ok(Value::Number(number));
    }
    let whole = number.as_f64().map(f64::trunc).unwrap_or_default();
    let whole: i64 = format!("{whole}").parse().map_err(|_| "the number is too large for a whole number".to_owned())?;
    Ok(Value::from(whole))
}

/// `bool`: `"true"` or `"false"` → a boolean.
pub fn bool(value: &Value, _: &PipeArgs<'_>) -> Result<Value, String> {
    match value {
        Value::Bool(flag) => Ok(Value::Bool(*flag)),
        Value::String(text) => {
            text.trim().parse().map(Value::Bool).map_err(|_| "the text is not `true` or `false`".to_owned())
        }
        _ => Err("it takes `true` or `false`".to_owned()),
    }
}

/// `string`: text stays as it is; anything else becomes compact JSON.
#[expect(clippy::unnecessary_wraps, reason = "every pipe has the `PipeFn` signature")]
pub fn string(value: &Value, _: &PipeArgs<'_>) -> Result<Value, String> {
    Ok(Value::String(crate::render::to_text(value)))
}

/// `json`: text holding JSON → the value.
pub fn json(value: &Value, _: &PipeArgs<'_>) -> Result<Value, String> {
    serde_json::from_str(text(value)?).map_err(|error| format!("the text is not JSON: {error}"))
}

/// `jsonString`: any value → its JSON text.
#[expect(clippy::unnecessary_wraps, reason = "every pipe has the `PipeFn` signature")]
pub fn json_string(value: &Value, _: &PipeArgs<'_>) -> Result<Value, String> {
    Ok(Value::String(value.to_string()))
}

/// `base64`: encodes the text's UTF-8 bytes.
pub fn base64(value: &Value, _: &PipeArgs<'_>) -> Result<Value, String> {
    Ok(Value::String(STANDARD.encode(text(value)?)))
}

/// `fromBase64`: decodes to UTF-8 text.
pub fn from_base64(value: &Value, _: &PipeArgs<'_>) -> Result<Value, String> {
    let bytes = STANDARD.decode(text(value)?).map_err(|error| format!("the text is not base64: {error}"))?;
    String::from_utf8(bytes).map(Value::String).map_err(|_| "the decoded bytes are not UTF-8 text".to_owned())
}

/// `lower`.
pub fn lower(value: &Value, _: &PipeArgs<'_>) -> Result<Value, String> {
    text(value).map(|text| Value::String(text.to_lowercase()))
}

/// `upper`.
pub fn upper(value: &Value, _: &PipeArgs<'_>) -> Result<Value, String> {
    text(value).map(|text| Value::String(text.to_uppercase()))
}

/// `trim`: drops leading and trailing white space.
pub fn trim(value: &Value, _: &PipeArgs<'_>) -> Result<Value, String> {
    text(value).map(|text| Value::String(text.trim().to_owned()))
}

/// `urlEncode`: percent-encodes all but the RFC 3986 unreserved characters.
pub fn url_encode(value: &Value, _: &PipeArgs<'_>) -> Result<Value, String> {
    text(value).map(|text| Value::String(percent_encoding::utf8_percent_encode(text, UNRESERVED).to_string()))
}

/// `default(x)`: a missing or `null` value → `x`.
#[expect(clippy::unnecessary_wraps, reason = "every pipe has the `PipeFn` signature")]
pub fn default(value: &Value, args: &PipeArgs<'_>) -> Result<Value, String> {
    let fallback = args.args.first().cloned().unwrap_or_default();
    Ok(if value.is_null() { fallback } else { value.clone() })
}

/// `random`: one element of a list.
pub fn random(value: &Value, args: &PipeArgs<'_>) -> Result<Value, String> {
    let items = list(value)?;
    items.get(args.chance.below(items.len())).cloned().ok_or_else(|| "the list is empty".to_owned())
}

/// `shuffle`: the list in random order.
pub fn shuffle(value: &Value, args: &PipeArgs<'_>) -> Result<Value, String> {
    let mut items = list(value)?.clone();
    for last in (1..items.len()).rev() {
        items.swap(last, args.chance.below(last + 1).min(last));
    }
    Ok(Value::Array(items))
}

/// `take(n)`: the first `n` elements.
pub fn take(value: &Value, args: &PipeArgs<'_>) -> Result<Value, String> {
    let count = args.args.first().and_then(Value::as_u64).and_then(|count| usize::try_from(count).ok());
    let count = count.ok_or_else(|| "it needs a whole number of elements".to_owned())?;
    Ok(Value::Array(list(value)?.iter().take(count).cloned().collect()))
}
