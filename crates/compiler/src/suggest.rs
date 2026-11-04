//! "Did you mean" for unknown keys, words and matchers.

use vc_diagnostics::{Diagnostic, Fix, TextEdit};
use vc_model::SourceSpan;

/// The candidates close to `word`: equal ignoring case, or within a third of its length in edits.
pub fn close<'a>(word: &str, candidates: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    let lower = word.to_lowercase();
    let limit = (word.chars().count() / 3).max(1);
    let mut found: Vec<(usize, &str)> = candidates
        .into_iter()
        .map(|candidate| (strsim::damerau_levenshtein(&lower, &candidate.to_lowercase()), candidate))
        .filter(|(distance, _)| *distance <= limit)
        .collect();
    found.sort_unstable();
    found.into_iter().map(|(_, candidate)| candidate).collect()
}

/// What was written, where, and what it could have been.
pub struct Misspelling<'a> {
    /// The text as written.
    pub word: &'a str,
    /// Where it is, exactly; a fix replaces this span.
    pub span: SourceSpan,
    /// The known words.
    pub known: &'a [&'a str],
}

/// Adds a hint to `diagnostic`: a certain fix when exactly one known word is close, the close
/// words when several are, or every known word when none is.
pub fn hint(diagnostic: Diagnostic, misspelling: &Misspelling<'_>) -> Diagnostic {
    let close = close(misspelling.word, misspelling.known.iter().copied());
    match close.as_slice() {
        [one] if misspelling.span.byte_range().len() == misspelling.word.len() => {
            let edit = TextEdit::replace(misspelling.span, *one);
            diagnostic
                .help(format!("did you mean `{one}`?"))
                .fix(Fix::certain(format!("Replace with {one}"), vec![edit]))
        }
        [] => diagnostic.help(format!("expected one of {}", quoted(misspelling.known))),
        several => diagnostic.help(format!("did you mean one of {}?", quoted(several))),
    }
}

/// `a`, `b`, `c`.
pub fn quoted(words: &[&str]) -> String {
    words.iter().map(|word| format!("`{word}`")).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use vc_diagnostics::{Code, Diagnostic};
    use vc_model::{Pos, SourceFileId, SourceSpan};

    use super::{Misspelling, close, hint};

    fn span(len: usize) -> SourceSpan {
        SourceSpan::new(SourceFileId::new(0), Pos::START, Pos::new(1, 1 + u32::try_from(len).unwrap(), len))
    }

    #[test]
    fn finds_close_words_nearest_first() {
        assert_eq!(close("baseURL", ["baseUrl", "path"]), ["baseUrl"]);
        assert_eq!(close("stauts", ["status", "states", "body"]), ["status", "states"]);
        assert_eq!(close("zzz", ["status"]), [] as [&str; 0]);
        assert_eq!(close("ab", ["ac", "xy"]), ["ac"]);
    }

    #[test]
    fn hints_with_a_certain_fix_only_for_one_close_word() {
        let diagnostic = || Diagnostic::new(Code::V1004, span(6), "unknown key");
        let one = hint(diagnostic(), &Misspelling { word: "methd", span: span(5), known: &["method", "path"] });
        assert_eq!(one.help, ["did you mean `method`?"]);
        assert_eq!(one.fixes.len(), 1);
        let none = hint(diagnostic(), &Misspelling { word: "zzz", span: span(3), known: &["a", "b"] });
        assert_eq!(none.help, ["expected one of `a`, `b`"]);
        assert_eq!(none.fixes, []);
        let quoted = hint(diagnostic(), &Misspelling { word: "methd", span: span(7), known: &["method"] });
        assert_eq!(quoted.fixes, []);
        let several = hint(diagnostic(), &Misspelling { word: "stauts", span: span(6), known: &["status", "states"] });
        assert_eq!(several.help, ["did you mean one of `status`, `states`?"]);
    }
}
