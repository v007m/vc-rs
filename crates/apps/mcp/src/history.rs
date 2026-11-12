//! The tools that read stored runs: `query_history` and `explain_failure`.

use async_trait::async_trait;
use serde_json::{Value, json};
use vc_engine::RunFilter;

use crate::McpError;
use crate::args::{Args, count, optional_text, text};
use crate::read::explain_request;
use crate::schema::object;
use crate::tools::{Cx, Tool};

/// How many runs `query_history` lists when no `limit` is given.
pub const LIMIT: usize = 20;

/// `query_history {limit?, run?}`: the latest runs, or one run with every test.
pub struct QueryHistory;

#[async_trait]
impl Tool for QueryHistory {
    fn name(&self) -> &'static str {
        "query_history"
    }

    fn description(&self) -> &'static str {
        "Lists the latest stored runs, or with `run` one run and the status of every test in it."
    }

    fn schema(&self) -> Value {
        let limit = json!({"type": "integer", "minimum": 1, "description": "How many runs to list; 20 when left out."});
        object(json!({ "limit": limit, "run": run_property() }), &[])
    }

    async fn call(&self, cx: &Cx, args: &Args) -> Result<Value, McpError> {
        if let Some(run) = optional_text(args, "run")? {
            return Ok(cx.engine.stored(&run)?.json());
        }
        let limit = count(args, "limit")?.unwrap_or(LIMIT);
        Ok(json!({ "runs": cx.engine.history(limit, &RunFilter::default())?.json() }))
    }
}

/// `explain_failure {test, run?}`: how a test ended in a stored run, beside its definition.
pub struct ExplainFailure;

#[async_trait]
impl Tool for ExplainFailure {
    fn name(&self) -> &'static str {
        "explain_failure"
    }

    fn description(&self) -> &'static str {
        "Shows how a test ended in a stored run (the last one when `run` is left out): its status, \
         failure message and what blocked it, with the test's resolved definition beside it."
    }

    fn schema(&self) -> Value {
        let test = json!({"type": "string", "description": "The test's full ID, such as users.crud#getsUser."});
        object(json!({ "test": test, "run": run_property() }), &["test"])
    }

    async fn call(&self, cx: &Cx, args: &Args) -> Result<Value, McpError> {
        let test = text(args, "test")?;
        let which = optional_text(args, "run")?.unwrap_or_else(|| "last".to_owned());
        let stored = cx.engine.stored(&which)?.json();
        let results = stored["tests"].as_array().into_iter().flatten().filter(|row| is(row, &test)).cloned();
        let definition = cx.engine.explain(&test, &explain_request(cx))?.text;
        Ok(json!({ "run": stored["run"], "results": results.collect::<Vec<_>>(), "definition": definition }))
    }
}

fn run_property() -> Value {
    json!({"type": "string", "description": "A run number from query_history, or `last`."})
}

/// Whether a stored test row is an instance of `test`: the test itself, an example row or a
/// repeat (`users.crud#rejectsBadEmail [row 2]`).
fn is(row: &Value, test: &str) -> bool {
    row["id"].as_str().is_some_and(|id| id == test || id.strip_prefix(test).is_some_and(|rest| rest.starts_with(' ')))
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;
