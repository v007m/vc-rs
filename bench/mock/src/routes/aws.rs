//! `ANY /aws/…`: checks an AWS Signature Version 4 server side, as AWS does, so an end-to-end
//! test proves that what `awsSigV4` sends is what it signed (D457). Written apart from the
//! signer, from AWS's documentation, so the two do not share a mistake.

use std::fmt::Write as _;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::{self, Bytes};
use axum::extract::Request;
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, routing};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

mod canonical;

/// The access key ID the mock knows: AWS's documented example.
pub const ACCESS_KEY_ID: &str = "AKIDEXAMPLE";
/// The secret of [`ACCESS_KEY_ID`]: AWS's documented example.
pub const SECRET_ACCESS_KEY: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";
/// How far `x-amz-date` may be from the server's clock, as AWS allows.
const SKEW_SECS: u64 = 300;
const BODY_LIMIT: usize = 16 * 1024 * 1024;
const ALGORITHM: &str = "AWS4-HMAC-SHA256";
/// The signing names whose paths AWS signs as they arrive, as S3 does.
const S3_SERVICES: [&str; 4] = ["s3", "s3express", "s3-outposts", "s3-object-lambda"];

/// `ANY /aws/{service path}` answers 200 with the service, region and signed header names of a
/// valid signature, or 403 with AWS's error code and, for a mismatch, the canonical request.
pub fn routes() -> Router {
    Router::new().route("/aws/{*rest}", routing::any(aws))
}

async fn aws(request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let Ok(body) = body::to_bytes(body, BODY_LIMIT).await else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs());
    match verify(&parts, &body, now) {
        Ok(checked) => Json(checked).into_response(),
        Err(refusal) => (StatusCode::FORBIDDEN, Json(refusal.0)).into_response(),
    }
}

/// Why a request was refused, as AWS's error body.
#[derive(Debug)]
pub struct Refusal(pub Value);

fn refused(code: &str, message: &str) -> Refusal {
    Refusal(json!({ "error": code, "message": message }))
}

/// The parts of an `Authorization` header.
struct Signed<'a> {
    key_id: &'a str,
    scope: [&'a str; 3],
    names: Vec<&'a str>,
    signature: &'a str,
}

/// Checks the signature on `parts` and `body` at `now`, in seconds since the Unix epoch.
///
/// # Errors
///
/// A [`Refusal`] when the signature is missing, malformed, out of date, by an unknown key or
/// does not match.
pub fn verify(parts: &Parts, body: &Bytes, now: u64) -> Result<Value, Refusal> {
    let header = |name: &str| parts.headers.get(name).and_then(|value| value.to_str().ok());
    let authorization = header("authorization").ok_or_else(|| refused("MissingAuthenticationToken", "no signature"))?;
    let signed = parse(authorization)?;
    if signed.key_id != ACCESS_KEY_ID {
        return Err(refused("InvalidClientTokenId", "the access key ID is not known"));
    }
    let stamp = header("x-amz-date").ok_or_else(|| refused("IncompleteSignature", "no x-amz-date"))?;
    fresh(stamp, signed.scope[0], now)?;
    let s3 = S3_SERVICES.contains(&signed.scope[2]);
    let canonical = canonical::request(&canonical::Arrived { parts, body }, &signed.names, s3)?;
    let scope = format!("{}/aws4_request", signed.scope.join("/"));
    let to_sign = format!("{ALGORITHM}\n{stamp}\n{scope}\n{}", hex(&Sha256::digest(canonical.as_bytes())));
    if signature(&to_sign, signed.scope) != signed.signature {
        let message = "the signature does not match the one the request computes to";
        let body = json!({ "error": "SignatureDoesNotMatch", "message": message,
            "canonicalRequest": canonical, "stringToSign": to_sign });
        return Err(Refusal(body));
    }
    let [_, region, service] = signed.scope;
    Ok(json!({ "service": service, "region": region, "signedHeaders": signed.names }))
}

/// `AWS4-HMAC-SHA256 Credential=<key>/<date>/<region>/<service>/aws4_request,
/// SignedHeaders=<names>, Signature=<hex>`.
fn parse(authorization: &str) -> Result<Signed<'_>, Refusal> {
    let incomplete = || refused("IncompleteSignature", "the Authorization header is not a SigV4 signature");
    let fields = authorization.strip_prefix(ALGORITHM).ok_or_else(incomplete)?;
    let field =
        |name: &str| fields.split(',').map(str::trim).find_map(|field| field.strip_prefix(name)?.strip_prefix('='));
    let credential: Vec<&str> = field("Credential").ok_or_else(incomplete)?.split('/').collect();
    let [key_id, date, region, service, "aws4_request"] = credential[..] else { return Err(incomplete()) };
    let names = field("SignedHeaders").ok_or_else(incomplete)?.split(';').collect();
    let signature = field("Signature").ok_or_else(incomplete)?;
    Ok(Signed { key_id, scope: [date, region, service], names, signature })
}

/// Checks `stamp` (`YYYYMMDD'T'HHMMSS'Z'`) is on the scope's `date` and within the skew of `now`.
fn fresh(stamp: &str, date: &str, now: u64) -> Result<(), Refusal> {
    let at = seconds(stamp).ok_or_else(|| refused("IncompleteSignature", "x-amz-date is not a SigV4 date"))?;
    if !stamp.starts_with(date) {
        return Err(refused("SignatureDoesNotMatch", "the credential scope's date is not x-amz-date's"));
    }
    if at.abs_diff(now) > SKEW_SECS {
        return Err(refused("RequestTimeTooSkewed", "x-amz-date is more than 5 minutes from the server's time"));
    }
    Ok(())
}

/// `YYYYMMDD'T'HHMMSS'Z'` in seconds since the Unix epoch; `None` for any other text, or a
/// month, day, hour, minute or second out of its range.
fn seconds(stamp: &str) -> Option<u64> {
    let (date, time) = stamp.strip_suffix('Z')?.split_once('T')?;
    if (date.len(), time.len()) != (8, 6) {
        return None;
    }
    let number =
        |text: &str, from: usize, to: usize, top: u64| text.get(from..to)?.parse::<u64>().ok().filter(|n| *n <= top);
    let (month, day) = (number(date, 4, 6, 12).filter(|m| *m > 0)?, number(date, 6, 8, 31).filter(|d| *d > 0)?);
    let days = days_from_civil(number(date, 0, 4, 9999)?, month, day)?;
    Some(days * 86_400 + number(time, 0, 2, 23)? * 3600 + number(time, 2, 4, 59)? * 60 + number(time, 4, 6, 60)?)
}

/// Days from 1970-01-01 to a date on or after it (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: u64, month: u64, day: u64) -> Option<u64> {
    let year = if month <= 2 { year.checked_sub(1)? } else { year };
    let era = year / 400;
    let of_era = year - era * 400;
    let of_year = (153 * ((month + 9) % 12) + 2) / 5 + day.checked_sub(1)?;
    let of_era_days = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    (era * 146_097 + of_era_days).checked_sub(719_468)
}

/// The hex HMAC of `to_sign` under the key derived for `[date, region, service]`.
fn signature(to_sign: &str, scope: [&str; 3]) -> String {
    let seed = format!("AWS4{SECRET_ACCESS_KEY}");
    let key = scope.into_iter().chain(["aws4_request"]).fold(seed.into_bytes(), |key, step| hmac(&key, step));
    hex(&hmac(&key, to_sign))
}

fn hmac(key: &[u8], data: &str) -> Vec<u8> {
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(key) else { return Vec::new() };
    mac.update(data.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

#[cfg(test)]
#[path = "aws_tests.rs"]
mod tests;
