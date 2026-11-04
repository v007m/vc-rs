#![cfg(test)]

use vc_model::{CelSource, Namespace, Pos, ScalarStyle, SourceFileId, SourceSpan, StringTable};
use vc_template::{CelCompiler, Roots};

/// A string prefix letter (`b`, `r`) not followed by a quote is a plain word, so the text after
/// it is still read for the root `var`.
#[test]
fn a_prefix_letter_without_a_quote_does_not_start_a_string() {
    for text in ["[1].all(b, b > var.a)", "[1].all(r, r > var.a)"] {
        let width = u32::try_from(text.len()).unwrap();
        let span = SourceSpan::new(SourceFileId::new(0), Pos::START, Pos::new(1, 1 + width, text.len()));
        let cel = CelSource { text: text.into(), span, style: ScalarStyle::Plain };
        let program = CelCompiler::new().check(&cel, &Roots::new(), &mut StringTable::new()).unwrap();
        let roots: Vec<_> = program.refs.iter().map(|r| r.ns).collect();
        assert_eq!(roots, [Namespace::Var], "{text}");
    }
}
