//! What a row source may read. `examples: ${…}` is rendered before the run starts, when only
//! `var`, `env` and `dataset` have a value, and so is every var it reads (D125). The compiler
//! checks the definitions every profile shares, and the engine checks the active profiles' ones
//! before it renders the rows, both in these words (V2105).

use vc_model::Namespace;

use crate::parse::root_name;

/// The roots a row source, and every var it reads, may read.
pub const ROW_ROOTS: &[Namespace] = &[Namespace::Var, Namespace::Env, Namespace::Dataset];

/// Why a row source reads only [`ROW_ROOTS`].
pub const ROW_HELP: &str = "`examples` is read before the run starts, so it reads only `var`, `env` and `dataset`";

/// Why the vars a row source reads read only [`ROW_ROOTS`].
const VAR_HELP: &str =
    "`examples` is read before the run starts, so the vars it reads may read only `var`, `env` and `dataset`";

/// A root outside [`ROW_ROOTS`] as a help line names it: `` `secret` ``, or an action's response.
#[must_use]
pub fn row_root_label(ns: Namespace) -> String {
    root_name(ns).map_or_else(|| "an action's response".to_owned(), |root| format!("`{root}`"))
}

/// The message and help of V2105 at `examples` when it reads `var.<read>`, and the var `var`
/// (`read` itself, or a var it reads in turn) needs `needs`, which has no value before the run.
#[must_use]
pub fn row_var_unread(read: &str, var: &str, needs: &str) -> (String, String) {
    let message = format!("`examples` reads `var.{read}`, which has no value before the run");
    (message, format!("`var.{var}` needs {needs}; {VAR_HELP}"))
}

#[cfg(test)]
mod tests {
    use vc_model::{Namespace, StringTable};

    use super::{row_root_label, row_var_unread};

    #[test]
    fn a_root_is_named_in_backticks_and_a_step_as_a_response() {
        assert_eq!(row_root_label(Namespace::Secret), "`secret`");
        let step = StringTable::new().intern("seeded").unwrap();
        assert_eq!(row_root_label(Namespace::Step(step)), "an action's response");
    }

    #[test]
    fn the_message_names_the_var_read_and_the_help_the_var_that_needs_the_root() {
        let (message, help) = row_var_unread("picked", "token", "`secret`");
        assert_eq!(message, "`examples` reads `var.picked`, which has no value before the run");
        assert!(help.starts_with("`var.token` needs `secret`; `examples` is read before the run starts"), "{help}");
    }
}
