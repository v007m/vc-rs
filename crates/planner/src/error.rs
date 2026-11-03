//! Why a plan could not be made.

use vc_diagnostics::Diagnostic;
use vc_model::ActionId;

/// Why [`plan`](crate::plan) refused a selection or a program.
///
/// Selector problems carry the user's own text and no span: they come from the command line,
/// not from a definition file. The CLI shows them as `error: …` with [`PlanError::help`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    /// Problems found in the definitions themselves, with their codes and spans.
    #[error("the definitions have {} problem(s)", .0.len())]
    Diagnostics(Vec<Diagnostic>),
    /// A positional selector matched no test and names no test file.
    #[error("no test or test file matches `{selector}`")]
    UnknownSelector {
        /// The selector as given.
        selector: String,
        /// Test and file IDs close to it, closest first.
        suggestions: Vec<String>,
    },
    /// A `:n` selector asked for an instance the test does not have.
    #[error("`{selector}` asks for an instance the test does not have; it has {count}")]
    NoSuchInstance {
        /// The selector as given.
        selector: String,
        /// How many instances the test has.
        count: u32,
    },
    /// A `--tag` expression could not be read.
    #[error("cannot read the tag expression `{expr}`: {reason}")]
    BadTagExpr {
        /// The expression as given.
        expr: String,
        /// What is wrong with it.
        reason: String,
    },
    /// A glob selector could not be compiled.
    #[error("cannot read the glob `{pattern}`: {reason}")]
    BadGlob {
        /// The glob as given.
        pattern: String,
        /// What is wrong with it.
        reason: String,
    },
    /// A selected test reads its examples from a file or a template the engine did not preload.
    #[error("the example rows of `{test}` were not loaded")]
    RowsNotLoaded {
        /// The test's full ID.
        test: String,
    },
    /// A planned action has a kind this milestone cannot run yet.
    #[error("`{kind}:` actions are not supported yet")]
    NotSupportedYet {
        /// The action kind's YAML key: `function` or `command`.
        kind: &'static str,
        /// The action.
        action: ActionId,
    },
}

impl PlanError {
    /// The `help:` line to show under the error, when there is one.
    #[must_use]
    pub fn help(&self) -> Option<String> {
        match self {
            Self::UnknownSelector { suggestions, .. } => {
                suggestions.first().map(|best| format!("did you mean `{best}`?"))
            }
            Self::NoSuchInstance { count, .. } => Some(format!("pick an instance from 1 to {count}")),
            Self::BadTagExpr { .. } => {
                Some("`,` means or, `&` means and, `!` excludes: `smoke,checkout & !slow`".into())
            }
            Self::NotSupportedYet { .. } => Some("`function:` and `command:` actions arrive in M4".into()),
            Self::Diagnostics(_) | Self::BadGlob { .. } | Self::RowsNotLoaded { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use vc_model::ActionId;

    use super::PlanError;

    #[test]
    fn help_suggests_the_closest_id_and_names_the_milestone() {
        let unknown =
            PlanError::UnknownSelector { selector: "users.crd".into(), suggestions: vec!["users.crud".into()] };
        assert_eq!(unknown.help().as_deref(), Some("did you mean `users.crud`?"));
        let none = PlanError::UnknownSelector { selector: "zzz".into(), suggestions: Vec::new() };
        assert_eq!(none.help(), None);
        let m4 = PlanError::NotSupportedYet { kind: "command", action: ActionId::new(0) };
        assert_eq!(m4.to_string(), "`command:` actions are not supported yet");
        assert!(m4.help().is_some_and(|help| help.contains("M4")));
        let row = PlanError::NoSuchInstance { selector: "a.b#c:4".into(), count: 3 };
        assert_eq!(row.help().as_deref(), Some("pick an instance from 1 to 3"));
        assert!(PlanError::BadTagExpr { expr: "!".into(), reason: "empty".into() }.help().is_some());
        assert_eq!(PlanError::RowsNotLoaded { test: "a.b#c".into() }.help(), None);
    }
}
