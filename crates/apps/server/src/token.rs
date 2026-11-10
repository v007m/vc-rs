//! The secrets of one `vc ui` process, each 256 random bits in base64url (D239, D240). The
//! launch token signs one browser in, once: it is traded for the session secret, which only the
//! sign-in cookie holds and which nobody has seen before that. The agent token is what MCP
//! clients send as a bearer token on `/mcp`. No secret is ever taken for another, so a leaked
//! cookie is no agent token and the agent token opens no page. All are compared in constant time
//! and never shown by `Debug`.

use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};

use crate::ServerError;

/// 256 bits, 43 base64url characters.
const BYTES: usize = 32;

/// The launch token, the session secret and the agent token. A candidate is checked by
/// verifying an HMAC of it against the secret's own, under a per-process key, which `ring`
/// compares in constant time.
pub struct Token {
    key: hmac::Key,
    launch: Secret,
    session: Secret,
    agent: Secret,
    /// Turns true when the launch token is traded for the cookie, which it is only once.
    launched: AtomicBool,
}

/// What a launch token given at `GET /?token=…` gets.
#[derive(Debug, PartialEq, Eq)]
pub enum Exchange {
    /// The cookie: this is the token's first use.
    SignedIn,
    /// Nothing: the token was traded already.
    Used,
    /// Nothing: it is not this process's launch token.
    Wrong,
}

/// One secret: its text and its HMAC under the process's key.
struct Secret {
    text: String,
    tag: hmac::Tag,
}

impl Token {
    /// New secrets from the system's random source.
    pub fn generate() -> Result<Self, ServerError> {
        let random = SystemRandom::new();
        let key = hmac::Key::generate(hmac::HMAC_SHA256, &random).map_err(|_| ServerError::Random)?;
        let launch = Secret::generate(&random, &key)?;
        let session = Secret::generate(&random, &key)?;
        let agent = Secret::generate(&random, &key)?;
        Ok(Self { key, launch, session, agent, launched: AtomicBool::new(false) })
    }

    /// Trades `candidate` for the cookie if it is the launch token and has not been traded yet.
    /// It is compared in time that does not depend on where they differ, and a wrong guess does
    /// not use the token up.
    pub fn exchange(&self, candidate: &str) -> Exchange {
        if !self.launch.verify(&self.key, candidate) {
            return Exchange::Wrong;
        }
        let first = self.launched.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire);
        if first.is_ok() { Exchange::SignedIn } else { Exchange::Used }
    }

    /// Whether `candidate` is the session secret, in the same way.
    pub fn session_matches(&self, candidate: &str) -> bool {
        self.session.verify(&self.key, candidate)
    }

    /// Whether `candidate` is the agent token, in the same way.
    pub fn agent_matches(&self, candidate: &str) -> bool {
        self.agent.verify(&self.key, candidate)
    }

    /// The launch token's text, for the launch URL only.
    pub fn expose(&self) -> &str {
        &self.launch.text
    }

    /// The session secret's text, for the sign-in cookie only.
    pub fn session(&self) -> &str {
        &self.session.text
    }

    /// The agent token's text, for the `vc ui: mcp` line and the Connect panel only.
    pub fn agent(&self) -> &str {
        &self.agent.text
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
