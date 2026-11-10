//! What each built-in `${fake.…}` generator does, and the arguments it takes (D174). Names and
//! addresses come from the `fake` crate's English data; every draw comes from the run's seeded
//! [`Chance`], so `--seed` repeats them.

use std::convert::Infallible;

use fake::Dummy;
use fake::faker::address::en::{BuildingNumber, CityName, CountryCode, CountryName, StreetName, ZipCode};
use fake::faker::internet::en::{SafeEmail, Username};
use fake::faker::job::en::Title;
use fake::faker::lorem::en::{Paragraphs, Sentence, Word, Words};
use fake::faker::name::en::{FirstName, Name};
use fake::faker::phone_number::en::PhoneNumber;
use rand_core::TryRng;
use serde_json::{Number, Value};

use crate::codes;
use crate::fakes::{FakeCx, FakeError};
use crate::pipe_fns::Chance;
use crate::regex_gen::Pattern;
use crate::time::{Duration, Timestamp};

/// The most words, paragraphs or characters one generator makes.
const MAX_COUNT: u64 = 10_000;
/// The largest whole number a JavaScript client reads exactly: Faker's default `number.int` top.
pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
const ALPHANUMERIC: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
const MS_PER_DAY: u64 = 86_400_000;
const NANOS_PER_MS: i128 = 1_000_000;

/// The run's draws as the random generator the `fake` crate takes.
struct Draws<'a>(&'a dyn Chance);

impl TryRng for Draws<'_> {
    type Error = Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Infallible> {
        Ok(u32::try_from(self.0.next_u64() >> 32).unwrap_or_default())
    }

    fn try_next_u64(&mut self) -> Result<u64, Infallible> {
        Ok(self.0.next_u64())
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), Infallible> {
        for chunk in dst.chunks_mut(8) {
            chunk.iter_mut().zip(self.0.next_u64().to_le_bytes()).for_each(|(slot, byte)| *slot = byte);
        }
        Ok(())
    }
}

fn faked<F, T: Dummy<F>>(faker: &F, cx: &FakeCx<'_>) -> T {
    T::dummy_with_rng(faker, &mut Draws(cx.chance))
}

/// Generators that take no arguments and give the `fake` crate's text.
macro_rules! texts {
    ($($doc:literal $name:ident => $faker:expr;)*) => {$(
        #[doc = $doc]
        pub fn $name(_: &[Value], cx: &FakeCx<'_>) -> Result<Value, String> {
            Ok(Value::String(faked::<_, String>(&$faker, cx)))
        }
    )*};
}

texts! {
    "`person.fullName`" full_name => Name();
    "`person.firstName`" first_name => FirstName();
    "`person.jobTitle`" job_title => Title();
    "`internet.email`, at a reserved example domain" email => SafeEmail();
    "`internet.userName`" user_name => Username();
    "`phone.number`" phone => PhoneNumber();
    "`location.city`" city => CityName();
    "`location.zipCode`" zip_code => ZipCode();
    "`location.country`" country => CountryName();
    "`location.countryCode`" country_code => CountryCode();
    "`lorem.word`" word => Word();
    "`lorem.sentence`" sentence => Sentence(4..10);
}

/// `location.streetAddress`: `482 Kristin Mews`.
#[expect(clippy::unnecessary_wraps, reason = "every generator has the `FakeFn` signature")]
pub fn street_address(_: &[Value], cx: &FakeCx<'_>) -> Result<Value, String> {
    let (number, street): (String, String) = (faked(&BuildingNumber(), cx), faked(&StreetName(), cx));
    Ok(Value::String(format!("{number} {street}")))
}

/// The count argument, or `default`.
fn count(args: &[Value], default: usize) -> usize {
    args.first().and_then(Value::as_u64).and_then(|count| usize::try_from(count).ok()).unwrap_or(default)
}

/// `lorem.words(n)`: `n` words (3 by default), spaced.
#[expect(clippy::unnecessary_wraps, reason = "every generator has the `FakeFn` signature")]
pub fn words(args: &[Value], cx: &FakeCx<'_>) -> Result<Value, String> {
    let count = count(args, 3);
    let words: Vec<String> = faked(&Words(count..count + 1), cx);
    Ok(Value::String(words.join(" ")))
}

/// `lorem.paragraphs(n)`: `n` paragraphs (3 by default), one per line.
#[expect(clippy::unnecessary_wraps, reason = "every generator has the `FakeFn` signature")]
pub fn paragraphs(args: &[Value], cx: &FakeCx<'_>) -> Result<Value, String> {
    let count = count(args, 3);
    let paragraphs: Vec<String> = faked(&Paragraphs(count..count + 1), cx);
    let lines: Vec<String> = paragraphs.iter().map(|paragraph| paragraph.replace('\n', " ")).collect();
    Ok(Value::String(lines.join("\n")))
}

/// `string.alphanumeric(n)`: `n` letters and digits (1 by default).
#[expect(clippy::unnecessary_wraps, reason = "every generator has the `FakeFn` signature")]
pub fn alphanumeric(args: &[Value], cx: &FakeCx<'_>) -> Result<Value, String> {
    let pick = |_| ALPHANUMERIC.get(cx.chance.below(ALPHANUMERIC.len())).map_or('A', |byte| char::from(*byte));
    Ok(Value::String((0..count(args, 1)).map(pick).collect()))
}

/// `string.uuid`: a version 4 UUID from two draws.
#[expect(clippy::unnecessary_wraps, reason = "every generator has the `FakeFn` signature")]
pub fn uuid(_: &[Value], cx: &FakeCx<'_>) -> Result<Value, String> {
    let mut bytes = [0_u8; 16];
    bytes[..8].copy_from_slice(&cx.chance.next_u64().to_be_bytes());
    bytes[8..].copy_from_slice(&cx.chance.next_u64().to_be_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let nibbles = bytes.iter().flat_map(|byte| [byte >> 4, byte & 0x0f]);
    let hex: String = nibbles.filter_map(|nibble| char::from_digit(u32::from(nibble), 16)).collect();
    let part = |range: std::ops::Range<usize>| hex.get(range).unwrap_or_default().to_owned();
    Ok(Value::String([part(0..8), part(8..12), part(12..16), part(16..20), part(20..32)].join("-")))
}

/// `string.fromRegExp('…')`: text the pattern matches.
pub fn from_reg_exp(args: &[Value], cx: &FakeCx<'_>) -> Result<Value, String> {
    let pattern = Pattern::parse(args.first().and_then(Value::as_str).unwrap_or_default())?;
    Ok(Value::String(pattern.generate(cx.chance)))
}

/// `number.int(min, max)`: a whole number from `min` to `max`, both included.
pub fn int(args: &[Value], cx: &FakeCx<'_>) -> Result<Value, String> {
    let bound = |index: usize, default: i64| args.get(index).and_then(Value::as_i64).unwrap_or(default);
    let (min, max) = (i128::from(bound(0, 0)), i128::from(bound(1, MAX_SAFE_INTEGER)));
    let span = u128::try_from(max - min + 1).map_err(|_| "`min` is above `max`".to_owned())?;
    let offset = i128::try_from(u128::from(cx.chance.next_u64()) % span).unwrap_or_default();
    i64::try_from(min + offset).map(Value::from).map_err(|_| "the number is out of range".to_owned())
}

/// A number from `min` up to `max`: the arguments, or `default`.
fn between(args: &[Value], default: (f64, f64), cx: &FakeCx<'_>) -> f64 {
    let bound = |index: usize, default: f64| args.get(index).and_then(Value::as_f64).unwrap_or(default);
    let (min, max) = (bound(0, default.0), bound(1, default.1));
    let unit = f64::from(u32::try_from(cx.chance.next_u64() >> 32).unwrap_or_default()) / 4_294_967_296.0;
    (max - min).mul_add(unit, min)
}

fn number(value: f64) -> Result<Value, String> {
    Number::from_f64(value).map(Value::Number).ok_or_else(|| "the number is not finite".to_owned())
}

/// `number.float(min, max)`: a number from `min` up to `max` (0 to 1 by default).
pub fn float(args: &[Value], cx: &FakeCx<'_>) -> Result<Value, String> {
    number(between(args, (0.0, 1.0), cx))
}

/// `commerce.price(min, max)`: a price with two decimals (1 to 1000 by default).
pub fn price(args: &[Value], cx: &FakeCx<'_>) -> Result<Value, String> {
    number((between(args, (1.0, 1000.0), cx) * 100.0).round() / 100.0)
}

/// An instant up to `days` before (or after, when `ahead`) the run's start, as RFC 3339.
fn date(cx: &FakeCx<'_>, days: u64, ahead: bool) -> Result<Value, String> {
    let offset = 1 + cx.chance.next_u64() % (days * MS_PER_DAY);
    let offset = i128::from(offset) * NANOS_PER_MS;
    let span = Duration::from_nanos(if ahead { offset } else { -offset });
    let start = Timestamp::from_nanos(i128::from(cx.started_ms) * NANOS_PER_MS);
    let time = start.zip(span).and_then(|(start, span)| start.plus(span));
    time.map(|time| Value::String(time.to_string())).ok_or_else(|| "the date is out of range".to_owned())
}

/// `date.past`: within the year before the run started.
pub fn past(_: &[Value], cx: &FakeCx<'_>) -> Result<Value, String> {
    date(cx, 365, false)
}

/// `date.future`: within the year after the run started.
pub fn future(_: &[Value], cx: &FakeCx<'_>) -> Result<Value, String> {
    date(cx, 365, true)
}

/// `date.recent`: within the day before the run started.
pub fn recent(_: &[Value], cx: &FakeCx<'_>) -> Result<Value, String> {
    date(cx, 1, false)
}

/// `helpers.arrayElement(…)`: one of the arguments.
pub fn array_element(args: &[Value], cx: &FakeCx<'_>) -> Result<Value, String> {
    args.get(cx.chance.below(args.len())).cloned().ok_or_else(|| "there is nothing to choose from".to_owned())
}

fn wrong(message: &str) -> FakeError {
    FakeError::new(codes::FAKE_ARGS, message)
}

/// No arguments.
pub fn no_args(args: &[Value]) -> Result<(), FakeError> {
    args.is_empty().then_some(()).ok_or_else(|| wrong("takes no arguments"))
}

/// Nothing, or one count up to [`MAX_COUNT`].
pub fn count_arg(args: &[Value]) -> Result<(), FakeError> {
    match args {
        [] => Ok(()),
        [count] if count.as_u64().is_some_and(|count| count <= MAX_COUNT) => Ok(()),
        _ => Err(wrong(&format!("takes nothing or one whole number from 0 to {MAX_COUNT}"))),
    }
}

/// One pattern the generator supports.
pub fn pattern_arg(args: &[Value]) -> Result<(), FakeError> {
    let [Value::String(pattern)] = args else { return Err(wrong("takes one pattern, such as '[A-Z]{3}'")) };
    let refuse = |reason| FakeError::new(codes::FAKE_PATTERN, &format!("cannot make text for this pattern: {reason}"));
    Pattern::parse(pattern).map(drop).map_err(refuse)
}

/// Nothing, or two whole numbers, the first no larger.
pub fn int_range(args: &[Value]) -> Result<(), FakeError> {
    match args {
        [] => Ok(()),
        [min, max] if min.as_i64().zip(max.as_i64()).is_some_and(|(min, max)| min <= max) => Ok(()),
        _ => Err(wrong("takes nothing or two whole numbers, min then max")),
    }
}

/// Nothing, or two numbers, the first no larger.
pub fn number_range(args: &[Value]) -> Result<(), FakeError> {
    match args {
        [] => Ok(()),
        [min, max] if min.as_f64().zip(max.as_f64()).is_some_and(|(min, max)| min <= max) => Ok(()),
        _ => Err(wrong("takes nothing or two numbers, min then max")),
    }
}

/// One or more values to choose from.
pub fn choices(args: &[Value]) -> Result<(), FakeError> {
    (!args.is_empty()).then_some(()).ok_or_else(|| wrong("takes one or more values to choose from"))
}
