//! The positional selector kinds, as a registry: the first kind that claims a selector's text
//! reads it, and text that no kind claims is a file ID.

use globset::{Glob, GlobMatcher};

use super::spec::{Candidate, Spec};
use crate::PlanError;

/// A positional selector, read: which tests it matches and, for `:n`, which instance.
pub struct Positional {
    pub text: String,
    pub spec: Box<dyn Spec>,
    /// The 1-based instance a `:n` selector picks.
    pub ordinal: Option<u32>,
}

/// One kind of positional selector: reads the text when it claims it, else `None`.
type Kind = fn(&str) -> Option<Result<Positional, PlanError>>;

/// Tried in order.
const KINDS: &[Kind] = &[path, glob, instance, test_id];

/// Reads one positional selector with the first kind that claims it.
pub fn read(text: &str) -> Result<Positional, PlanError> {
    KINDS.iter().find_map(|kind| kind(text)).unwrap_or_else(|| Ok(whole(text, FileIdIs(text.to_owned()))))
}

/// Reads a `paths` entry, which is always a path, even without a `/`.
pub fn read_path(text: &str) -> Positional {
    let trimmed = text.trim_start_matches("./").trim_end_matches('/');
    whole(text, PathUnder(trimmed.to_owned()))
}

fn whole(text: &str, spec: impl Spec + 'static) -> Positional {
    Positional { text: text.to_owned(), spec: Box::new(spec), ordinal: None }
}

/// A folder or a file: has a `/` or a YAML extension.
fn path(text: &str) -> Option<Result<Positional, PlanError>> {
    let is_path = text.contains('/') || [".yaml", ".yml"].iter().any(|suffix| text.ends_with(suffix));
    is_path.then(|| Ok(read_path(text)))
}

/// A glob over IDs: has `*`, `?`, `[` or `{`.
fn glob(text: &str) -> Option<Result<Positional, PlanError>> {
    if !text.contains(['*', '?', '[', '{']) {
        return None;
    }
    let bad = |error: globset::Error| PlanError::BadGlob { pattern: text.to_owned(), reason: error.kind().to_string() };
    Some(Glob::new(text).map(|glob| whole(text, GlobOver(glob.compile_matcher()))).map_err(bad))
}

/// One instance of a test: `users.crud#rejectsBadEmail:2`.
fn instance(text: &str) -> Option<Result<Positional, PlanError>> {
    let (id_part, ordinal) = text.rsplit_once(':')?;
    let ordinal = ordinal.parse().ok().filter(|_| ordinal.bytes().all(|byte| byte.is_ascii_digit()))?;
    let mut positional = test_id(id_part)?.ok()?;
    text.clone_into(&mut positional.text);
    positional.ordinal = Some(ordinal);
    Some(Ok(positional))
}

/// A full test ID: has a `#`.
fn test_id(text: &str) -> Option<Result<Positional, PlanError>> {
    text.contains('#').then(|| Ok(whole(text, TestIdIs(text.to_owned()))))
}

/// The test's source file is this path or under this folder; an empty path is the project.
struct PathUnder(String);

impl Spec for PathUnder {
    fn matches(&self, candidate: &Candidate<'_>) -> bool {
        let path = candidate.path;
        let under = path.strip_prefix(self.0.as_str()).is_some_and(|rest| rest.starts_with('/'));
        self.0.is_empty() || self.0 == "." || path == self.0 || under
    }
}

/// The glob matches the test's full ID or its file ID.
struct GlobOver(GlobMatcher);

impl Spec for GlobOver {
    fn matches(&self, candidate: &Candidate<'_>) -> bool {
        self.0.is_match(candidate.full_id) || self.0.is_match(candidate.file_id)
    }
}

struct TestIdIs(String);

impl Spec for TestIdIs {
    fn matches(&self, candidate: &Candidate<'_>) -> bool {
        candidate.full_id == self.0
    }
}

struct FileIdIs(String);

impl Spec for FileIdIs {
    fn matches(&self, candidate: &Candidate<'_>) -> bool {
        candidate.file_id == self.0
    }
}
