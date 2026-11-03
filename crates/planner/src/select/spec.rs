//! Specifications: composable predicates over a test, combined with and, or and not.

use std::collections::BTreeSet;

use vc_model::{Program, Test};

use crate::visit::{Visit, Walker};

/// One test as the selectors see it, with its IDs and path resolved once.
pub struct Candidate<'p> {
    pub program: &'p Program,
    pub test: &'p Test,
    /// `users.crud#getsUser`.
    pub full_id: &'p str,
    /// `users.crud`.
    pub file_id: &'p str,
    /// `services/users/crud.test.yaml`, or empty when the source file is unknown.
    pub path: &'p str,
    /// The file's tags followed by the test's.
    pub tags: Vec<&'p str>,
}

impl<'p> Candidate<'p> {
    pub fn new(program: &'p Program, test: &'p Test) -> Self {
        let resolve = |sym| program.str(sym).unwrap_or_default();
        let file = program.test_files.get(&test.file);
        let file_tags = file.into_iter().flat_map(|file| file.tags.iter());
        let tags = file_tags.chain(&test.tags).map(|tag| tag.0.as_str()).collect();
        let path = program.files.get(test.span.file).map_or("", |source| source.path.as_str());
        Self { program, test, full_id: resolve(test.id.sym()), file_id: resolve(test.file.sym()), path, tags }
    }
}

/// A predicate over tests.
pub trait Spec {
    fn matches(&self, candidate: &Candidate<'_>) -> bool;
}

pub type BoxSpec = Box<dyn Spec>;

/// Holds when any part holds.
pub struct AnyOf(pub Vec<BoxSpec>);

impl Spec for AnyOf {
    fn matches(&self, candidate: &Candidate<'_>) -> bool {
        self.0.iter().any(|spec| spec.matches(candidate))
    }
}

/// Holds when every part holds; an empty one always holds.
pub struct AllOf(pub Vec<BoxSpec>);

impl Spec for AllOf {
    fn matches(&self, candidate: &Candidate<'_>) -> bool {
        self.0.iter().all(|spec| spec.matches(candidate))
    }
}

/// Holds when its part does not.
pub struct Not(pub BoxSpec);

impl Spec for Not {
    fn matches(&self, candidate: &Candidate<'_>) -> bool {
        !self.0.matches(candidate)
    }
}

/// The test or its file has the tag.
pub struct HasTag(pub String);

impl Spec for HasTag {
    fn matches(&self, candidate: &Candidate<'_>) -> bool {
        candidate.tags.contains(&self.0.as_str())
    }
}

/// The test's full ID or name contains the text, ignoring case.
pub struct Grep(String);

impl Grep {
    pub fn new(text: &str) -> Self {
        Self(text.to_lowercase())
    }
}

impl Spec for Grep {
    fn matches(&self, candidate: &Candidate<'_>) -> bool {
        let name = candidate.test.name.as_deref().unwrap_or_default();
        [candidate.full_id, name].iter().any(|text| text.to_lowercase().contains(&self.0))
    }
}

/// The test's file changed, or it calls an api whose file changed (`--changed`).
pub struct Changed(pub BTreeSet<String>);

impl Spec for Changed {
    fn matches(&self, candidate: &Candidate<'_>) -> bool {
        if self.0.contains(candidate.file_id) {
            return true;
        }
        let mut walker = Walker::new(candidate.program, Groups::default());
        walker.instance(candidate.test);
        let groups = walker.into_visitor().0;
        groups.iter().filter_map(|group| candidate.program.str(group.sym())).any(|id| self.0.contains(id))
    }
}

/// The api files an instance calls.
#[derive(Default)]
struct Groups(BTreeSet<vc_model::FileId>);

impl Visit for Groups {
    fn group(&mut self, group: vc_model::FileId) {
        self.0.insert(group);
    }
}
