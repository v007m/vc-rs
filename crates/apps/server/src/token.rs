//! The secrets of one `vc ui` process, each 256 random bits in base64url. The launch token is
//! known only to the page opened with it and to clients given it as a bearer token; the session
//! secret is what the sign-in cookie holds. The two are never interchangeable, so a cookie value
//! that leaks is no bearer token and the printed token is no cookie. Both are compared in
//! constant time and never shown by `Debug`.

use std::fmt;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};

use crate::ServerError;

/// 256 bits, 43 base64url characters.
const BYTES: usize = 32;

/// The launch token and the session secret. A candidate is checked by verifying an HMAC of it
/// against the secret's own, under a per-process key, which `ring` compares in constant time.
pub struct Token {
    key: hmac::Key,
    launch: Secret,
    session: Secret,
}

/// One secret: its text and its HMAC under the process's key.
struct Secret {
    text: String,
    tag: hmac::Tag,
}

impl Token {
    /// A new token and session secret from the system's random source.
    pub fn generate() -> Result<Self, ServerError> {
        let random = SystemRandom::new();
        let key = hmac::Key::generate(hmac::HMAC_SHA256, &random).map_err(|_| ServerError::Random)?;
        let launch = Secret::generate(&random, &key)?;
        let session = Secret::generate(&random, &key)?;
        Ok(Self { key, launch, session })
    }

    /// Whether `candidate` is the launch token, in time that does not depend on where they differ.
    pub fn matches(&self, candidate: &str) -> bool {
        self.launch.verify(&self.key, candidate)
    }

    /// Whether `candidate` is the session secret, in the same way.
    pub fn session_matches(&self, candidate: &str) -> bool {
        self.session.verify(&self.key, candidate)
    }

    /// The launch token's text, for the launch URL only.
    pub fn expose(&self) -> &str {
        &self.launch.text
    }

    /// The session secret's text, for the sign-in cookie only.
    pub fn session(&self) -> &str {
        &self.session.text
    }
}

impl Secret {
    fn generate(random: &SystemRandom, key: &hmac::Key) -> Result<Self, ServerError> {
        let mut bytes = [0_u8; BYTES];
        random.fill(&mut bytes).map_err(|_| ServerError::Random)?;
        let text = URL_SAFE_NO_PAD.encode(bytes);
        let tag = hmac::sign(key, text.as_bytes());
        Ok(Self { text, tag })
    }

    fn verify(&self, key: &hmac::Key, candidate: &str) -> bool {
        hmac::verify(key, candidate.as_bytes(), self.tag.as_ref()).is_ok()
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(<redacted>)")
    }
}
