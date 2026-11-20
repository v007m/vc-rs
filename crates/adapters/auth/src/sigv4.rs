//! `awsSigV4`: AWS Signature Version 4 in the `authorization` header (D431–D443).
//!
//! [`SigV4`] is pure: given a request and the time, it sets `x-amz-date`,
//! `x-amz-content-sha256`, `x-amz-security-token` and `authorization`. The settings are checked
//! once, when the signer is made. The canonical request and the string to sign never leave this
//! module, and the derived keys are wiped after each signature (D439).

mod canonical;
mod crypto;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod vectors;

use std::fmt;

use http::HeaderValue;
use http::header::AUTHORIZATION;
use vc_interfaces::{AuthError, AuthRequest, Secret};
use vc_model::RenderedRequest;

use self::canonical::Canonical;
use self::crypto::Moment;
use crate::token::required;

const ALGORITHM: &str = "AWS4-HMAC-SHA256";
const UNSIGNED: &str = "UNSIGNED-PAYLOAD";
const DATE: &str = "x-amz-date";
const CONTENT: &str = "x-amz-content-sha256";
const TOKEN: &str = "x-amz-security-token";
const KEY_ID_RULE: &str = "accessKeyId: printable ASCII with no / or spaces";
const TOKEN_RULE: &str = "sessionToken: a header value, with no line breaks";

/// What the canonical request holds for the body (`payload`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Payload {
    /// The body's SHA-256, in hex.
    Signed,
    /// The literal `UNSIGNED-PAYLOAD`.
    Unsigned,
}

/// One `awsSigV4` definition's rendered settings, checked. `Debug` shows the region and service
/// only; the secret and token are wiped when it is dropped.
pub struct SigV4 {
    auth: String,
    region: String,
    service: String,
    key_id: String,
    secret: Secret,
    token: Option<Secret>,
    payload: Payload,
    s3: bool,
}

impl SigV4 {
    /// The signer for `request`'s settings: `region`, `service`, `accessKeyId`,
    /// `secretAccessKey`, `sessionToken` (none when it renders empty) and `payload`.
    ///
    /// # Errors
    ///
    /// [`AuthError::Missing`], naming the key and never a value: an empty `accessKeyId` or
    /// `secretAccessKey`, an `accessKeyId` holding `/` or whitespace, a `region` or `service`
    /// that is not lowercase letters, digits and `-`, a `sessionToken` that cannot be a header
    /// value, or a `payload` other than `signed` or `unsigned`.
    pub fn from_request(request: &AuthRequest) -> Result<Self, AuthError> {
        let missing = |key: &str| AuthError::Missing { auth: request.auth.clone(), key: key.to_owned() };
        let key_id = filled(request, "accessKeyId")?;
        if !key_id.bytes().all(|byte| byte.is_ascii_graphic() && byte != b'/') {
            return Err(missing(KEY_ID_RULE));
        }
        let token = request.text("sessionToken").filter(|token| !token.is_empty());
        if token.is_some_and(|token| HeaderValue::from_str(token).is_err()) {
            return Err(missing(TOKEN_RULE));
        }
        let payload = match request.text("payload") {
            None | Some("signed") => Payload::Signed,
            Some("unsigned") => Payload::Unsigned,
            Some(_) => return Err(missing("payload: signed or unsigned")),
        };
        let service = scope_name(request, "service")?;
        Ok(Self {
            auth: request.auth.clone(),
            region: scope_name(request, "region")?,
            s3: service == "s3",
            service,
            key_id: key_id.to_owned(),
            secret: Secret::new(filled(request, "secretAccessKey")?),
            token: token.map(Secret::new),
            payload,
        })
    }

    /// `request` signed as at `now_ms` (UTC, milliseconds since the Unix epoch). For `s3`, the
    /// path is rewritten into AWS's encoding first (D436).
    ///
    /// # Errors
    ///
    /// [`AuthError::Missing`] if a header the signer sets cannot hold its value; the checks in
    /// [`SigV4::from_request`] rule that out.
    pub fn sign(&self, request: RenderedRequest, now_ms: u64) -> Result<RenderedRequest, AuthError> {
        let moment = Moment::at(now_ms);
        let payload = self.payload_hash(&request.body);
        let mut request = self.stamp(request, &moment, &payload)?;
        let canonical = canonical::request(&request, &payload, self.s3);
        let authorization = self.authorization(&canonical, &moment)?;
        request.headers.insert(AUTHORIZATION, authorization);
        Ok(request)
    }

    /// The canonical request's last line: the body's hash, or `UNSIGNED-PAYLOAD`.
    fn payload_hash(&self, body: &[u8]) -> String {
        match self.payload {
            Payload::Signed => crypto::sha256_hex(body),
            Payload::Unsigned => UNSIGNED.to_owned(),
        }
    }

    /// `request` with the S3 path, `x-amz-date`, `x-amz-content-sha256` and the session token,
    /// each replacing a header of that name.
    fn stamp(
        &self,
        mut request: RenderedRequest,
        moment: &Moment,
        payload: &str,
    ) -> Result<RenderedRequest, AuthError> {
        if self.s3 {
            request.uri = canonical::s3_uri(request.uri);
        }
        request.headers.insert(DATE, self.header(DATE, &moment.stamp, false)?);
        request.headers.insert(CONTENT, self.header(CONTENT, payload, false)?);
        if let Some(token) = &self.token {
            request.headers.insert(TOKEN, self.header(TOKEN_RULE, token.expose(), true)?);
        }
        Ok(request)
    }

    /// The `authorization` value for `canonical`, marked sensitive.
    fn authorization(&self, canonical: &Canonical, moment: &Moment) -> Result<HeaderValue, AuthError> {
        let scope = self.scope(&moment.date);
        let text = string_to_sign(&moment.stamp, &scope, canonical.text());
        let signature = self.signature(&moment.date, &text);
        let value = format!(
            "{ALGORITHM} Credential={}/{scope}, SignedHeaders={}, Signature={signature}",
            self.key_id,
            canonical.signed()
        );
        self.header(KEY_ID_RULE, &value, true)
    }

    /// `date/region/service/aws4_request`.
    fn scope(&self, date: &str) -> String {
        format!("{date}/{}/{}/aws4_request", self.region, self.service)
    }

    /// The hex HMAC of `text` under the key derived for `date`, which is wiped on return.
    fn signature(&self, date: &str, text: &str) -> String {
        let key = crypto::signing_key(&self.secret, [date, &self.region, &self.service]);
        crypto::hex(&*crypto::hmac(&*key, text.as_bytes()))
    }

    /// `text` as a header value, marked `sensitive` or not; the setting `key` it came from is
    /// the error.
    fn header(&self, key: &str, text: &str, sensitive: bool) -> Result<HeaderValue, AuthError> {
        let mut value = HeaderValue::from_str(text)
            .map_err(|_| AuthError::Missing { auth: self.auth.clone(), key: key.to_owned() })?;
        value.set_sensitive(sensitive);
        Ok(value)
    }
}

impl fmt::Debug for SigV4 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SigV4").field("region", &self.region).field("service", &self.service).finish_non_exhaustive()
    }
}

/// The string to sign: the algorithm, the time, the scope and the canonical request's hash.
fn string_to_sign(stamp: &str, scope: &str, canonical: &[u8]) -> String {
    format!("{ALGORITHM}\n{stamp}\n{scope}\n{}", crypto::sha256_hex(canonical))
}

/// The setting `key`, which must be non-empty text.
fn filled<'r>(request: &'r AuthRequest, key: &str) -> Result<&'r str, AuthError> {
    let value = required(request, key)?;
    if value.is_empty() {
        return Err(AuthError::Missing { auth: request.auth.clone(), key: key.to_owned() });
    }
    Ok(value)
}

/// The setting `key` as a scope part: lowercase letters, digits and `-`.
fn scope_name(request: &AuthRequest, key: &str) -> Result<String, AuthError> {
    let value = filled(request, key)?;
    if value.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-') {
        return Ok(value.to_owned());
    }
    Err(AuthError::Missing { auth: request.auth.clone(), key: format!("{key}: lowercase letters, digits and -") })
}
