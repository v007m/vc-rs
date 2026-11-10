use serde::{Deserialize, Serialize};

use crate::Size;

/// The body size `history.maxBodySize` keeps when it is not written.
pub const DEFAULT_MAX_BODY: Size = Size::from_bytes(64 * 1024);

/// The number of runs `history.keep` keeps when it is not written (D170).
pub const DEFAULT_KEEP: u32 = 50;

/// `history` in `vibranium.config.yaml` (D170).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct HistoryConfig {
    /// `bodies`: which exchanges keep their bodies.
    pub bodies: Bodies,
    /// `maxBodySize`: a longer body is cut to this size before it leaves the executor.
    pub max_body_size: Size,
    /// `keep`: how many runs stay after a run ends.
    pub keep: Keep,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self { bodies: Bodies::Failures, max_body_size: DEFAULT_MAX_BODY, keep: Keep::Runs(DEFAULT_KEEP) }
    }
}

impl HistoryConfig {
    /// `maxBodySize` in bytes, as [`cut_body`] takes it.
    #[must_use]
    pub fn max_body(&self) -> usize {
        usize::try_from(self.max_body_size.bytes()).unwrap_or(usize::MAX)
    }
}

/// `body` as text, at most `max` bytes, ending on a character boundary; and whether it was cut.
/// The history, the reports and the web UI's run feed all keep a body this way, after masking.
#[must_use]
pub fn cut_body(body: &[u8], max: usize) -> (Option<String>, bool) {
    if body.is_empty() {
        return (None, false);
    }
    let text = String::from_utf8_lossy(body);
    if text.len() <= max {
        return (Some(text.into_owned()), false);
    }
    let end = text.floor_char_boundary(max);
    (Some(text.get(..end).unwrap_or_default().to_owned()), true)
}

/// `history.bodies`: which exchanges keep their request and response bodies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum Bodies {
    /// No bodies are stored.
    None,
    /// Only the exchanges of failed or blocked tests and lifecycle actions.
    #[default]
    Failures,
    /// Every exchange.
    All,
}

impl Bodies {
    /// The value written as `word`, if it is one.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        [("none", Self::None), ("failures", Self::Failures), ("all", Self::All)]
            .into_iter()
            .find_map(|(name, bodies)| (name == word).then_some(bodies))
    }
}

/// `history.keep`: how many runs stay in the history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_enums,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub enum Keep {
    /// The newest this many runs.
    Runs(u32),
    /// Every run.
    All,
}

#[cfg(test)]
mod tests {
    use super::{Bodies, HistoryConfig, Keep, cut_body};

    #[test]
    fn defaults_follow_d170() {
        let config = HistoryConfig::default();
        assert_eq!(
            (config.bodies, config.keep, config.max_body_size.bytes()),
            (Bodies::Failures, Keep::Runs(50), 65_536)
        );
    }

    #[test]
    fn bodies_parse_their_words() {
        assert_eq!(Bodies::parse("all"), Some(Bodies::All));
        assert_eq!(Bodies::parse("none"), Some(Bodies::None));
        assert_eq!(Bodies::parse("failures"), Some(Bodies::Failures));
        assert_eq!(Bodies::parse("some"), None);
    }

    #[test]
    fn a_body_is_cut_on_a_character_boundary_and_says_so() {
        assert_eq!(cut_body(b"", 4), (None, false));
        assert_eq!(cut_body(b"abcd", 4), (Some("abcd".to_owned()), false));
        assert_eq!(cut_body("ab\u{e9}d".as_bytes(), 3), (Some("ab".to_owned()), true));
        assert_eq!(HistoryConfig::default().max_body(), 65_536);
    }
}
