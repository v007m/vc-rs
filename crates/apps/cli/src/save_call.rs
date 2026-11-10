//! `vc call --save-as-test`: the call just made, written as a test that expects the status it
//! got and the body fields named with `--expect-field` (D176).

use clap::Args;
use vc_engine::{CallAsTest, CallReport, Engine};

use crate::{Outcome, changes};

#[derive(Args)]
pub struct SaveArgs {
    /// Writes the call as a test with this ID (`users.crud#listsUsers`) that expects the
    /// status it got; no schema is inferred.
    #[arg(long, value_name = "TEST_ID")]
    save_as_test: Option<String>,
    /// With `--save-as-test`, also expects this body field at the value it came back with
    /// (`body.id`, `body.items[0].id`); repeat it for more.
    #[arg(long, value_name = "body.PATH", requires = "save_as_test")]
    expect_field: Vec<String>,
}

/// The call `vc call` made: its api, its `--var name=value`s and what came back.
pub struct Made<'a> {
    pub api: &'a str,
    pub vars: &'a [String],
    pub report: &'a CallReport,
}

/// Writes `made` as a test when `--save-as-test` asks for it and says where; `None` when it
/// does not ask.
pub fn save(engine: &Engine, args: &SaveArgs, made: &Made<'_>) -> Option<Outcome> {
    let call = CallAsTest {
        id: args.save_as_test.clone()?,
        api: made.api.to_owned(),
        vars: made.vars.to_vec(),
        response: made.report.response.clone(),
        fields: args.expect_field.clone(),
    };
    Some(match engine.save_call_as_test(&call) {
        Ok(added) => {
            let done = format!("✓ wrote {} to {}:{}", added.id, added.path, added.line);
            changes::write(engine, Ok(added.changes), &done)
        }
        Err(error) => Outcome::Engine(error),
    })
}
