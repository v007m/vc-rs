//! Tag expressions: `,` means or, `&` means and, `!` excludes, binding in that order from
//! loosest to tightest, so `smoke,checkout & !slow` is `smoke` or (`checkout` and not `slow`).

use super::spec::{AllOf, AnyOf, BoxSpec, HasTag, Not};
use crate::PlanError;

/// Reads one `-t` expression into a specification.
pub fn parse(expr: &str) -> Result<BoxSpec, PlanError> {
    let any = expr.split(',').map(|alternative| all(expr, alternative)).collect::<Result<_, _>>()?;
    Ok(Box::new(AnyOf(any)))
}

fn all(expr: &str, alternative: &str) -> Result<BoxSpec, PlanError> {
    let terms = alternative.split('&').map(|term| term_spec(expr, term)).collect::<Result<_, _>>()?;
    Ok(Box::new(AllOf(terms)))
}

fn term_spec(expr: &str, term: &str) -> Result<BoxSpec, PlanError> {
    let term = term.trim();
    if let Some(negated) = term.strip_prefix('!') {
        return Ok(Box::new(Not(term_spec(expr, negated)?)));
    }
    let bad = |reason: String| PlanError::BadTagExpr { expr: expr.to_owned(), reason };
    if term.is_empty() {
        return Err(bad("a tag is missing".to_owned()));
    }
    if term.contains(char::is_whitespace) {
        return Err(bad(format!("`{term}` is not one tag; join tags with `,` or `&`")));
    }
    Ok(Box::new(HasTag(term.to_owned())))
}

#[cfg(test)]
mod tests {
    use super::parse;
    use crate::PlanError;
    use crate::select::spec::Candidate;
    use crate::testing::Fixture;

    fn holds(expr: &str, tags: &[&str]) -> bool {
        let mut fixture = Fixture::new();
        let test = fixture.test("a", tags);
        let program = fixture.program;
        parse(expr).unwrap().matches(&Candidate::new(&program, &program.tests[&test]))
    }

    #[test]
    fn reads_or_and_and_not_with_and_binding_tighter() {
        let table = [
            ("smoke", &["smoke"][..], true),
            ("smoke", &["slow"][..], false),
            ("smoke,checkout", &["checkout"], true),
            ("smoke & slow", &["smoke"], false),
            ("smoke & slow", &["smoke", "slow"], true),
            ("!slow", &["smoke"], true),
            ("!slow", &["slow"], false),
            ("!!slow", &["slow"], true),
            ("smoke,checkout & !slow", &["smoke", "slow"], true),
            ("smoke,checkout & !slow", &["checkout", "slow"], false),
            ("smoke,checkout & !slow", &["checkout"], true),
            (" smoke ", &["smoke"], true),
        ];
        for (expr, tags, expected) in table {
            assert_eq!(holds(expr, tags), expected, "{expr} over {tags:?}");
        }
    }

    #[test]
    fn rejects_missing_and_spaced_tags() {
        for (expr, reason) in [("smoke,", "a tag is missing"), ("!", "a tag is missing"), ("a & ", "a tag is missing")]
        {
            assert_eq!(parse(expr).err(), Some(PlanError::BadTagExpr { expr: expr.into(), reason: reason.into() }));
        }
        let spaced = parse("smoke slow").err().map(|error| error.to_string());
        assert_eq!(
            spaced.as_deref(),
            Some("cannot read the tag expression `smoke slow`: `smoke slow` is not one tag; join tags with `,` or `&`")
        );
    }
}
