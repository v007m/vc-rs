//! SHA-256, HMAC-SHA256, hex, the signing key and the signature's time (D442).

use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use vc_interfaces::Secret;
use vc_model::Date;
use zeroize::Zeroizing;

const SECONDS_PER_DAY: i64 = 86_400;
const HEX: &[u8; 16] = b"0123456789abcdef";

/// A key or MAC, wiped when it is dropped.
pub type Key = Zeroizing<[u8; 32]>;

/// The time one signature is made at, in UTC.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moment {
    /// `YYYYMMDD`, for the scope and the signing key.
    pub date: String,
    /// `YYYYMMDD'T'HHMMSS'Z'`, for `x-amz-date` and the string to sign.
    pub stamp: String,
}

impl Moment {
    /// The time `now_ms` milliseconds after the Unix epoch; past year 9999, the epoch, as a
    /// clock that reads the system time does for a time out of range.
    pub fn at(now_ms: u64) -> Self {
        let seconds = i64::try_from(now_ms / 1000).unwrap_or(i64::MAX);
        let (day, second) = Date::from_unix_days(seconds.div_euclid(SECONDS_PER_DAY))
            .map_or((Date::UNIX_EPOCH, 0), |day| (day, seconds.rem_euclid(SECONDS_PER_DAY)));
        let date = format!("{:04}{:02}{:02}", day.year(), day.month(), day.day());
        let stamp = format!("{date}T{:02}{:02}{:02}Z", second / 3600, second / 60 % 60, second % 60);
        Self { date, stamp }
    }
}

/// The lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// HMAC-SHA256 of `data` under `key`. HMAC takes a key of any length, so the key is always
/// accepted; the hash states are wiped when they are dropped.
pub fn hmac(key: &[u8], data: &[u8]) -> Key {
    let mut out = Zeroizing::new([0; 32]);
    if let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(key) {
        mac.update(data);
        out.copy_from_slice(&mac.finalize().into_bytes());
    }
    out
}

/// The signing key: `"AWS4" + secret`, then HMAC with the date, region, service and
/// `aws4_request` in turn. Each step's key is wiped as the next replaces it.
pub fn signing_key(secret: &Secret, [date, region, service]: [&str; 3]) -> Key {
    let secret = secret.expose().as_bytes();
    let mut seed = Zeroizing::new(Vec::with_capacity(secret.len() + 4));
    seed.extend_from_slice(b"AWS4");
    seed.extend_from_slice(secret);
    let key = hmac(&seed, date.as_bytes());
    [region, service, "aws4_request"].into_iter().fold(key, |key, step| hmac(&*key, step.as_bytes()))
}

/// `bytes` as lowercase hex.
pub fn hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        text.push(char::from(HEX[usize::from(byte >> 4)]));
        text.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    text
}
