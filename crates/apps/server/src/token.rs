//! The access token of one `vc ui` process: 256 random bits, base64url, known only to the page
//! opened with it. It is compared in constant time and never shown by `Debug`.

use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};

use crate::ServerError;

/// 256 bits, 43 base64url characters.
const BYTES: usize = 32;

/// The token. A candidate is checked by verifying an HMAC of it against the token's own, under a
/// per-process key, which `ring` compares in constant time.
pub struct Token {
    text: String,
    key: hmac::Key,
    tag: hmac::Tag,
}

impl Token {
    /// A new token from the system's random source.
    pub fn generate() -> Result<Self, ServerError> {
        let random = SystemRandom::new();
        let mut bytes = [0_u8; BYTES];
        random.fill(&mut bytes).map_err(|_| ServerError::Random)?;
        let key = hmac::Key::generate(hmac::HMAC_SHA256, &random).map_err(|_| ServerError::Random)?;
        let text = URL_SAFE_NO_PAD.encode(bytes);
        let tag = hmac::sign(&key, text.as_bytes());
        Ok(Self { text, key, tag })
    }

    /// Whether `candidate` is the token, in time that does not depend on where they differ.
    pub fn matches(&self, candidate: &str) -> bool {
        hmac::verify(&self.key, candidate.as_bytes(), self.tag.as_ref()).is_ok()
    }

    /// The token's text, for the launch URL and the sign-in cookie only.
    pub fn expose(&self) -> &str {
        &self.text
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}
