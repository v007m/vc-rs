//! The tools that act: `run_tests`, `call_api` and `write_definition`. The engine gates each
//! one by the profiles' `agents` rules, asks a person when a rule says so, and logs it (D195).

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use vc_engine::{CallRequest, PlanRequest, RunRequest, SelectRequest, WriteRequest, interrupts};

use crate::McpError;
use crate::args::{Args, flag, optional_text, pairs, text, texts};
use crate::schema::{object, strings};
use crate::tools::{Cx, Tool, Tools};

/// Adds the acting tools to `tools`.
pub fn register(tools: &mut Tools) {
    tools.add(Arc::new(RunTests));
    tools.add(Arc::new(CallApi));
    tools.add(Arc::new(WriteDefinition));
}

/// `run_tests {ids?, tags?, grep?}`: runs the selected tests, with one approval for the run
/// when the profiles ask.
struct RunTests;

#[async_trait]
impl Tool for RunTests {
    fn name(&self) -> &'static str {
        "run_tests"
    }

    fn description(&self) -> &'static str {
        "Runs the selected tests (every test when nothing is selected) and returns each test's status. \
         The project's agent rules may refuse the run or wait for a person to approve it."
    }

    fn reads(&self) -> bool {
        false
    }

    fn schema(&self) -> Value {
        let ids = strings("Test or file IDs to run, such as users.crud or users.crud#getsUser.");
        let grep = json!({"type": "string", "description": "Only tests whose name contains this."});
        object(json!({ "ids": ids, "tags": strings("Only tests with one of these tags."), "grep": grep }), &[])
    }

    async fn call(&self, cx: &Cx, args: &Args) -> Result<Value, McpError> {
        let select = SelectRequest {
            ids: texts(args, "ids")?,
            tags: texts(args, "tags")?,
            grep: optional_text(args, "grep")?,
            ..SelectRequest::default()
        };
        let plan =
            PlanRequest { select, profiles: cx.profiles.clone(), sets: cx.sets.clone(), ..PlanRequest::default() };
        let request = RunRequest { plan, agent: Some(cx.agent.clone()), ..RunRequest::default() };
        let report = cx.engine.run(&request, interrupts().1).await?;
        let result = report.run.map(|run| cx.engine.stored(&run.to_string())).transpose()?;
        let tests = result.map(|stored| stored.json()["tests"].clone());
        Ok(
            json!({ "exit": report.exit, "run": report.run, "seed": report.seed, "tests": tests, "notices": report.notices }),
        )
    }
}

/// `call_api {api, vars?, keep?}`: sends one api, as `vc call` does.
struct CallApi;

#[async_trait]
impl Tool for CallApi {
    fn name(&self) -> &'static str {
        "call_api"
    }

    fn description(&self) -> &'static str {
        "Sends one api with its auth and checks, then runs its cleanup unless `keep` is true. \
         The project's agent rules may refuse the call or wait for a person to approve it."
    }

    fn reads(&self) -> bool {
        false
    }

    fn schema(&self) -> Value {
        let api = json!({"type": "string", "description": "The api's full ID, such as users#create."});
        let vars =
            json!({"type": "object", "description": "Vars to send it with, over the api's own: {\"userId\": 7}."});
        let keep = json!({"type": "boolean", "description": "Leaves what the call made: its cleanup does not run."});
        object(json!({ "api": api, "vars": vars, "keep": keep }), &["api"])
    }

    async fn call(&self, cx: &Cx, args: &Args) -> Result<Value, McpError> {
        let request = CallRequest {
            api: text(args, "api")?,
            vars: pairs(args, "vars")?,
            keep: flag(args, "keep")?,
            profiles: cx.profiles.clone(),
            sets: cx.sets.clone(),
            agent: Some(cx.agent.clone()),
        };
        let report = cx.engine.call(&request).await?;
        Ok(json!({ "exit": report.exit, "response": report.response, "text": report.text, "notices": report.notices }))
    }
}

/// `write_definition {path, text}`: writes one api, test or load file whole, once it compiles.
struct WriteDefinition;

#[async_trait]
impl Tool for WriteDefinition {
    fn name(&self) -> &'static str {
        "write_definition"
    }

    fn description(&self) -> &'static str {
        "Writes one api, test or load file whole. The text must keep the project free of errors, \
         or nothing is written. The project's agent rules may refuse it or wait for a person to approve it."
    }

    fn reads(&self) -> bool {
        false
    }

    fn schema(&self) -> Value {
        let path = json!({"type": "string", "description": "The file's path from the project root, such as services/users/crud.test.yaml."});
        let body = json!({"type": "string", "description": "The file's whole new YAML text."});
        object(json!({ "path": path, "text": body }), &["path", "text"])
    }

    async fn call(&self, cx: &Cx, args: &Args) -> Result<Value, McpError> {
        let request = WriteRequest {
            path: text(args, "path")?,
            text: text(args, "text")?,
            agent: cx.agent.clone(),
            profiles: cx.profiles.clone(),
        };
        let changes = cx.engine.write_definition(&request).await?;
        let files: Vec<&str> = changes.files.iter().map(|file| file.path.as_str()).collect();
        Ok(json!({ "written": files, "titles": changes.titles }))
    }
}

#[cfg(test)]
#[path = "act_tests.rs"]
mod tests;
