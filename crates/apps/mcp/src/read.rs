//! The tools that only read: `list_definitions`, `search`, `get_definition` and `check`.
//! Every profile allows them (D195).

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{Value, json};
use vc_engine::{CheckRequest, ExplainRequest, JsonRenderer, ListKind, ListRequest, Listed, Render, SelectRequest};

use crate::McpError;
use crate::args::{Args, optional_text, text, texts};
use crate::history::{ExplainFailure, QueryHistory};
use crate::schema::{object, strings};
use crate::tools::{Cx, Tool, Tools};

/// What `list_definitions` lists, by the name its `kind` argument takes.
const KINDS: [(&str, ListKind); 3] = [("tests", ListKind::Tests), ("apis", ListKind::Apis), ("files", ListKind::Files)];

/// What `search` looks through, and the kind each item is reported as.
const SEARCHED: [(&str, ListKind); 2] = [("test", ListKind::Tests), ("api", ListKind::Apis)];

/// Adds the reading tools to `tools`.
pub fn register(tools: &mut Tools) {
    tools.add(Arc::new(ListDefinitions));
    tools.add(Arc::new(Search));
    tools.add(Arc::new(GetDefinition));
    tools.add(Arc::new(Check));
    tools.add(Arc::new(QueryHistory));
    tools.add(Arc::new(ExplainFailure));
}

/// `list_definitions {kind?, ids?, tags?}`: the tests, apis or definition files.
struct ListDefinitions;

#[async_trait]
impl Tool for ListDefinitions {
    fn name(&self) -> &'static str {
        "list_definitions"
    }

    fn description(&self) -> &'static str {
        "Lists the project's tests, apis or definition files, with their IDs, names, paths and tags."
    }

    fn schema(&self) -> Value {
        let kind = json!({"type": "string", "enum": ["tests", "apis", "files"], "description": "What to list; tests when left out."});
        let ids = strings("Test or file IDs to narrow the list to, such as users.crud or users.crud#getsUser.");
        object(json!({"kind": kind, "ids": ids, "tags": strings("Only tests with one of these tags.")}), &[])
    }

    async fn call(&self, cx: &Cx, args: &Args) -> Result<Value, McpError> {
        let name = optional_text(args, "kind")?.unwrap_or_else(|| "tests".to_owned());
        let kind = KINDS.iter().find(|(known, _)| *known == name).map(|(_, kind)| *kind).ok_or_else(|| {
            McpError::Argument { name: "kind", reason: format!("`{name}` is not tests, apis or files") }
        })?;
        let select = SelectRequest { ids: texts(args, "ids")?, tags: texts(args, "tags")?, ..SelectRequest::default() };
        let report = cx.engine.list(&ListRequest { kind, select, skipped: false })?;
        Ok(json!({ "items": report.json(), "notices": report.notices }))
    }
}

/// `search {query}`: the tests and apis whose ID or name holds the words.
struct Search;

#[async_trait]
impl Tool for Search {
    fn name(&self) -> &'static str {
        "search"
    }

    fn description(&self) -> &'static str {
        "Finds the tests and apis whose ID or name contains the query, ignoring case."
    }

    fn schema(&self) -> Value {
        object(json!({"query": {"type": "string", "description": "Text to look for, such as `refund`."}}), &["query"])
    }

    async fn call(&self, cx: &Cx, args: &Args) -> Result<Value, McpError> {
        let query = text(args, "query")?.to_lowercase();
        let mut found = Vec::new();
        for (kind, list) in SEARCHED {
            let report = cx.engine.list(&ListRequest { kind: list, ..ListRequest::default() })?;
            let item =
                |listed: &Listed| json!({"kind": kind, "id": listed.id, "name": listed.name, "path": listed.path});
            found.extend(report.items.iter().filter(|listed| holds(listed, &query)).map(item));
        }
        Ok(json!({ "items": found }))
    }
}

/// Whether `listed`'s ID or name holds `query`, which is in lower case.
fn holds(listed: &Listed, query: &str) -> bool {
    let name = listed.name.as_deref().unwrap_or_default();
    listed.id.to_lowercase().contains(query) || name.to_lowercase().contains(query)
}

/// `get_definition {id}`: a test or api with every layer resolved, as `vc explain` shows it.
struct GetDefinition;

#[async_trait]
impl Tool for GetDefinition {
    fn name(&self) -> &'static str {
        "get_definition"
    }

    fn description(&self) -> &'static str {
        "Shows a test or api with its resolved request, vars, auth, checks and where each comes from."
    }

    fn schema(&self) -> Value {
        let id = json!({"type": "string", "description": "A full ID: users.crud#getsUser for a test, users#get for an api."});
        object(json!({ "id": id }), &["id"])
    }

    async fn call(&self, cx: &Cx, args: &Args) -> Result<Value, McpError> {
        let id = text(args, "id")?;
        let report = cx.engine.explain(&id, &explain_request(cx))?;
        Ok(json!({ "id": id, "text": report.text, "notices": report.notices }))
    }
}

/// The profiles and settings `vc mcp` was started with, for a lookup.
pub fn explain_request(cx: &Cx) -> ExplainRequest {
    ExplainRequest { profiles: cx.profiles.clone(), sets: cx.sets.clone() }
}

/// `check {paths?}`: the project's diagnostics, as `vc check --json` gives them.
struct Check;

#[async_trait]
impl Tool for Check {
    fn name(&self) -> &'static str {
        "check"
    }

    fn description(&self) -> &'static str {
        "Checks the project's definition files and returns every diagnostic with its code, place, help and fix."
    }

    fn schema(&self) -> Value {
        object(json!({"paths": strings("Files or folders to check; the whole project when left out.")}), &[])
    }

    async fn call(&self, cx: &Cx, args: &Args) -> Result<Value, McpError> {
        let report = cx.engine.check(&CheckRequest { paths: texts(args, "paths")? })?;
        let rendered = JsonRenderer.render(&report.diagnostics, &report.files).map_err(internal)?;
        let diagnostics: Value = serde_json::from_str(&rendered).map_err(internal)?;
        let summary = report.summary();
        let counts = json!({"errors": summary.errors, "warnings": summary.warnings, "checked": report.checked});
        Ok(json!({ "summary": counts, "diagnostics": diagnostics, "notices": report.notices }))
    }
}

/// An error that is a bug in the server, not something the agent did.
pub fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::Internal { reason: error.to_string() }
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
