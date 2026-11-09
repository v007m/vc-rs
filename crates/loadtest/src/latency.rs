//! Latency histograms and their stored form, `HdrHistogram`'s uncompressed V2 encoding.

use std::time::Duration;

use hdrhistogram::Histogram;

use crate::LoadError;

/// The smallest latency told apart: 1 µs.
const LOWEST_US: u64 = 1;
/// The largest latency kept; slower ones count as 60 s.
const HIGHEST_US: u64 = 60_000_000;
/// Significant decimal digits of every recorded value.
const DIGITS: u8 = 3;
/// `HdrHistogram`'s uncompressed V2 cookie: `0x1c84_9303` with the word size (`0x10`) set.
const COOKIE: u32 = 0x1c84_9313;
/// The V2 header: cookie, payload length, normalizing offset, digits, lowest, highest, ratio.
const HEADER: usize = 40;

/// Latencies in microseconds, from 1 µs to 60 s at 3 significant digits.
#[derive(Clone, Debug)]
pub struct Latencies {
    histogram: Histogram<u64>,
}

impl Latencies {
    /// No latencies yet.
    ///
    /// # Errors
    ///
    /// [`LoadError::Histogram`] if the histogram cannot be made with the fixed bounds.
    pub fn new() -> Result<Self, LoadError> {
        let histogram = Histogram::new_with_bounds(LOWEST_US, HIGHEST_US, DIGITS)
            .map_err(|error| LoadError::Histogram(format!("{error:?}")))?;
        Ok(Self { histogram })
    }

    /// Records `latency`; one above 60 s counts as 60 s.
    pub fn record(&mut self, latency: Duration) {
        self.histogram.saturating_record(u64::try_from(latency.as_micros()).unwrap_or(u64::MAX));
    }

    /// Adds every latency of `other`.
    ///
    /// # Errors
    ///
    /// [`LoadError::Histogram`] if `other` holds a value beyond these bounds.
    pub fn merge(&mut self, other: &Self) -> Result<(), LoadError> {
        self.histogram.add(&other.histogram).map_err(|error| LoadError::Histogram(format!("{error:?}")))
    }

    /// How many latencies are recorded.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.histogram.len()
    }

    /// The latency at `quantile` (0.0 to 1.0) in milliseconds; 0 without latencies.
    #[must_use]
    pub fn quantile_ms(&self, quantile: f64) -> f64 {
        millis(self.histogram.value_at_quantile(quantile))
    }

    /// The slowest latency in milliseconds; 0 without latencies.
    #[must_use]
    pub fn max_ms(&self) -> f64 {
        if self.histogram.is_empty() { 0.0 } else { millis(self.histogram.max()) }
    }

    /// The count of microsecond value `micros`' bucket.
    #[must_use]
    pub fn count_at(&self, micros: u64) -> u64 {
        self.histogram.count_at(micros)
    }

    /// The uncompressed `HdrHistogram` V2 encoding, as `load_intervals.histogram` stores it.
    ///
    /// Every bucket up to the highest recorded one, as a zig-zag LEB128 count; a run of empty
    /// buckets is one negative count. The ninth byte of a varint holds 8 bits, as in `HdrHistogram`.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let counts = self.counts();
        let mut bytes = Vec::with_capacity(HEADER + counts.len());
        bytes.extend_from_slice(&COOKIE.to_be_bytes());
        bytes.extend_from_slice(&u32::try_from(counts.len()).unwrap_or(u32::MAX).to_be_bytes());
        bytes.extend_from_slice(&0_u32.to_be_bytes());
        bytes.extend_from_slice(&u32::from(self.histogram.sigfig()).to_be_bytes());
        bytes.extend_from_slice(&self.histogram.low().to_be_bytes());
        bytes.extend_from_slice(&self.histogram.high().to_be_bytes());
        bytes.extend_from_slice(&1.0_f64.to_be_bytes());
        bytes.extend_from_slice(&counts);
        bytes
    }

    /// The latencies `bytes` encode, as [`Latencies::encode`] wrote them.
    ///
    /// # Errors
    ///
    /// [`LoadError::BadEncoding`] for bytes of another cookie, bounds or length, or counts that
    /// do not fit the buckets.
    pub fn decode(bytes: &[u8]) -> Result<Self, LoadError> {
        let mut latencies = Self::new()?;
        let payload = latencies.payload(bytes)?;
        let buckets = Self::new()?;
        let mut values = buckets.histogram.iter_all().map(|bucket| bucket.value_iterated_to());
        let mut reader = payload.iter().copied().peekable();
        while reader.peek().is_some() {
            let word = read_varint(&mut reader).ok_or_else(|| bad("a count is cut short"))?;
            latencies.apply(unzigzag(word), &mut values)?;
        }
        Ok(latencies)
    }

    /// Records one decoded word: a count for the next bucket, or a negative run of empty ones.
    fn apply(&mut self, word: i64, values: &mut impl Iterator<Item = u64>) -> Result<(), LoadError> {
        let Ok(count) = u64::try_from(word) else {
            values.nth(skip(word)).ok_or_else(|| bad("an empty run passes the last bucket"))?;
            return Ok(());
        };
        let value = values.next().ok_or_else(|| bad("more counts than buckets"))?;
        self.histogram.record_n(value, count).map_err(|error| bad(&format!("{error:?}")))
    }

    /// The bucket counts as V2 words, run-length encoding empty buckets.
    fn counts(&self) -> Vec<u8> {
        let top = self.histogram.max();
        let mut bytes = Vec::new();
        let mut empty = 0_i64;
        for bucket in self.histogram.iter_all().take_while(|bucket| bucket.value_iterated_to() <= top) {
            let count = bucket.count_at_value();
            if count == 0 {
                empty = empty.saturating_add(1);
                continue;
            }
            write_empty(&mut bytes, empty);
            empty = 0;
            write_varint(&mut bytes, zigzag(i64::try_from(count).unwrap_or(i64::MAX)));
        }
        write_empty(&mut bytes, empty);
        bytes
    }

    /// The counts after `bytes`' header, once the header matches these latencies' own.
    fn payload<'b>(&self, bytes: &'b [u8]) -> Result<&'b [u8], LoadError> {
        let (header, payload) = bytes.split_at_checked(HEADER).ok_or_else(|| bad("shorter than the header"))?;
        if be_u32(header, 0) != Some(COOKIE) {
            return Err(bad("not the uncompressed V2 cookie"));
        }
        if be_u32(header, 4).and_then(|len| usize::try_from(len).ok()) != Some(payload.len()) {
            return Err(bad("the payload length does not match"));
        }
        let ours = (u32::from(self.histogram.sigfig()), self.histogram.low(), self.histogram.high());
        if (be_u32(header, 12), be_u64(header, 16), be_u64(header, 24)) != (Some(ours.0), Some(ours.1), Some(ours.2)) {
            return Err(bad("other bounds than 1 µs to 60 s at 3 digits"));
        }
        Ok(payload)
    }
}

fn millis(micros: u64) -> f64 {
    Duration::from_micros(micros).as_secs_f64() * 1000.0
}

fn bad(why: &str) -> LoadError {
    LoadError::BadEncoding(why.to_owned())
}

fn be_u32(bytes: &[u8], at: usize) -> Option<u32> {
    bytes.get(at..at.checked_add(4)?)?.try_into().ok().map(u32::from_be_bytes)
}

fn be_u64(bytes: &[u8], at: usize) -> Option<u64> {
    bytes.get(at..at.checked_add(8)?)?.try_into().ok().map(u64::from_be_bytes)
}

/// A run of `empty` buckets: nothing, one zero count, or the run's length negated.
fn write_empty(bytes: &mut Vec<u8>, empty: i64) {
    match empty {
        0 => {}
        1 => write_varint(bytes, zigzag(0)),
        run => write_varint(bytes, zigzag(-run)),
    }
}

/// The index offset `nth` takes to pass a run of `-count` empty buckets.
fn skip(count: i64) -> usize {
    usize::try_from(count.unsigned_abs().saturating_sub(1)).unwrap_or(usize::MAX)
}

const fn zigzag(value: i64) -> u64 {
    u64::from_ne_bytes(((value << 1) ^ (value >> 63)).to_ne_bytes())
}

const fn unzigzag(word: u64) -> i64 {
    i64::from_ne_bytes((word >> 1).to_ne_bytes()) ^ -i64::from_ne_bytes((word & 1).to_ne_bytes())
}

/// LEB128 for the first 8 bytes; a ninth holds the top 8 bits whole.
fn write_varint(bytes: &mut Vec<u8>, mut word: u64) {
    for _ in 0..8 {
        let low = word.to_le_bytes()[0];
        if word < 0x80 {
            bytes.push(low);
            return;
        }
        bytes.push((low & 0x7F) | 0x80);
        word >>= 7;
    }
    bytes.push(word.to_le_bytes()[0]);
}

fn read_varint(bytes: &mut impl Iterator<Item = u8>) -> Option<u64> {
    let mut word = 0_u64;
    for shift in (0..56).step_by(7) {
        let byte = bytes.next()?;
        word |= u64::from(byte & 0x7F) << shift;
        if byte & 0x80 == 0 {
            return Some(word);
        }
    }
    bytes.next().map(|byte| word | (u64::from(byte) << 56))
}

#[cfg(test)]
#[path = "latency_tests.rs"]
mod tests;
