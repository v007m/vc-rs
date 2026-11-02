use syn::visit::{self, Visit};
use syn::{Lit, Pat};

use super::Finder;

const MAX_STRING_ARMS: usize = 2;

/// Kinds are registry entries, not a `match` on three or more string literals.
pub struct StringMatch;

syntax_rule!(StringMatch, "ARCH-STRING-MATCH");

impl Visit<'_> for Finder<StringMatch> {
    fn visit_expr_match(&mut self, expr: &syn::ExprMatch) {
        let count: usize = expr.arms.iter().map(|arm| string_patterns(&arm.pat)).sum();
        if count > MAX_STRING_ARMS {
            let message = format!("`match` on {count} string literals; register the kinds in a registry instead");
            self.push(expr.match_token.span, message);
        }
        visit::visit_expr_match(self, expr);
    }
}

fn string_patterns(pat: &Pat) -> usize {
    match pat {
        Pat::Lit(lit) => usize::from(matches!(lit.lit, Lit::Str(_))),
        Pat::Or(or) => or.cases.iter().map(string_patterns).sum(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::StringMatch;
    use crate::fixtures::check_lib;

    #[test]
    fn accepts_two_string_arms_and_other_literals() {
        let src = r#"fn f(s: &str, n: u8) { match s { "a" | "b" => {}, _ => {} } match n { 1 => {}, 2 => {}, 3 => {}, _ => {} } }"#;
        assert_eq!(check_lib(&StringMatch, src), []);
    }

    #[test]
    fn rejects_three_string_arms() {
        let src = r#"fn f(s: &str) -> u8 { match s { "a" => 1, "b" | "c" => 2, _ => 0 } }"#;
        assert_eq!(check_lib(&StringMatch, src).len(), 1);
    }
}
