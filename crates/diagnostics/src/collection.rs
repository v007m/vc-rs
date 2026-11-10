use serde::Serialize;

use crate::{Diagnostic, Severity};

/// How many diagnostics there are of each severity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built or matched in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Summary {
    /// Diagnostics with severity `Error`.
    pub errors: usize,
    /// Diagnostics with severity `Warning`.
    pub warnings: usize,
    /// Diagnostics with severity `Info`.
    pub infos: usize,
}

/// The diagnostics found by one check or run, in the order they were pushed until sorted.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Diagnostics(Vec<Diagnostic>);

impl Diagnostics {
    /// An empty collection.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds one diagnostic.
    pub fn push(&mut self, diagnostic: Diagnostic) {
        self.0.push(diagnostic);
    }

    /// Whether any diagnostic is an error.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.0.iter().any(Diagnostic::is_error)
    }

    /// How many diagnostics there are.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are none.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Puts the diagnostics in display order: by file-table order (the compiler adds files in
    /// path order), then position, then code.
    /// Diagnostics at the same place with the same code keep their order.
    pub fn sort(&mut self) {
        self.0.sort_by_key(|diagnostic| (diagnostic.primary.span, diagnostic.code));
    }

    /// The count of each severity.
    #[must_use]
    pub fn summary(&self) -> Summary {
        let mut counts = [0; Severity::COUNT];
        for diagnostic in &self.0 {
            counts[diagnostic.severity.index()] += 1;
        }
        let [infos, warnings, errors] = counts;
        Summary { errors, warnings, infos }
    }

    /// The diagnostics in their current order.
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.0.iter()
    }
}

impl Extend<Diagnostic> for Diagnostics {
    fn extend<I: IntoIterator<Item = Diagnostic>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}

impl FromIterator<Diagnostic> for Diagnostics {
    fn from_iter<I: IntoIterator<Item = Diagnostic>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl IntoIterator for Diagnostics {
    type Item = Diagnostic;
    type IntoIter = std::vec::IntoIter<Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use vc_model::{Pos, SourceFileId, SourceSpan};

    use super::{Diagnostics, Summary};
    use crate::{Code, Diagnostic, Severity};

    fn at(file: u32, line: u32, code: Code) -> Diagnostic {
        let start = Pos::new(line, 1, 0);
        Diagnostic::new(code, SourceSpan::new(SourceFileId::new(file), start, start), "")
    }

    #[test]
    fn has_errors_ignores_warnings() {
        let mut diagnostics = Diagnostics::new();
        assert!(!diagnostics.has_errors() && diagnostics.is_empty());
        diagnostics.push(at(0, 1, Code::V3104));
        assert!(!diagnostics.has_errors());
        diagnostics.extend([at(0, 2, Code::V2003)]);
        assert!(diagnostics.has_errors());
        assert_eq!(diagnostics.len(), 2);
    }

    #[test]
    fn summary_counts_each_severity() {
        let mut info = at(0, 3, Code::V3104);
        info.severity = Severity::Info;
        let diagnostics: Diagnostics = [at(0, 1, Code::V2003), at(0, 2, Code::V3104), info].into_iter().collect();
        assert_eq!(diagnostics.summary(), Summary { errors: 1, warnings: 1, infos: 1 });
    }

    #[test]
    fn sort_orders_by_file_position_then_code() {
        let mut diagnostics = Diagnostics::new();
        diagnostics.extend([
            at(1, 1, Code::V1004),
            at(0, 9, Code::V2003),
            at(0, 2, Code::V2101),
            at(0, 2, Code::V2001),
        ]);
        diagnostics.sort();
        let order: Vec<_> = diagnostics.iter().map(|d| (d.primary.span.file.index(), d.code)).collect();
        let expected = [(0, Code::V2001), (0, Code::V2101), (0, Code::V2003), (1, Code::V1004)];
        assert_eq!(order, expected);
        assert_eq!(diagnostics.into_iter().count(), 4);
    }
}
