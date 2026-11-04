//! `vc list tests|apis|files`: the definitions, for people and scripts (D130).

use std::collections::BTreeSet;

use serde_json::{Value, json};
use vc_model::{Api, Program, SourceFile, SourceFileId, Sym, Test, TestId};
use vc_planner::PlanOptions;

use crate::{Engine, EngineError, ListKind, ListRequest};

/// One listed definition.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Listed {
    /// Its full ID: `users.crud#getsUser`, `users#getUser`, or a file ID.
    pub id: String,
    /// Its `name`, when written.
    pub name: Option<String>,
    /// The file it is in, relative to the project root.
    pub path: String,
    /// A test's tags, its file's first; empty for apis and files.
    pub tags: Vec<String>,
    /// Whether the test, or its file, is marked `skip`.
    pub skipped: bool,
}

/// What `vc list` found, in project order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ListReport {
    /// The definitions.
    pub items: Vec<Listed>,
    /// Problems that did not stop the listing, one line each.
    pub notices: Vec<String>,
}

impl ListReport {
    /// One line per item: the ID, then the name and tags when there are any.
    #[must_use]
    pub fn text(&self) -> String {
        self.items.iter().map(|item| line(item) + "\n").collect()
    }

    /// The items as a JSON array.
    #[must_use]
    pub fn json(&self) -> Value {
        let item = |listed: &Listed| json!({ "id": listed.id, "name": listed.name, "path": listed.path, "tags": listed.tags, "skipped": listed.skipped });
        Value::Array(self.items.iter().map(item).collect())
    }
}

fn line(item: &Listed) -> String {
    let mut line = item.id.clone();
    if let Some(name) = &item.name {
        line.push_str("  ");
        line.push_str(name);
    }
    if !item.tags.is_empty() {
        line.push_str("  [");
        line.push_str(&item.tags.join(", "));
        line.push(']');
    }
    if item.skipped {
        line.push_str("  (skipped)");
    }
    line
}

impl Engine {
    /// Lists the tests `request` selects (skipped ones included), every api, or every
    /// definition file.
    ///
    /// # Errors
    ///
    /// [`EngineError::Invalid`] when the project has errors, and [`EngineError::Selection`]
    /// when a selector cannot be read or matches nothing.
    pub fn list(&self, request: &ListRequest) -> Result<ListReport, EngineError> {
        let (program, notices) = self.program()?;
        let items = match request.kind {
            ListKind::Tests => self.tests(&program, request)?,
            ListKind::Apis => apis(&program),
            ListKind::Files => files(&program),
        };
        Ok(ListReport { items, notices })
    }

    /// The selected tests, in project order, each once however many instances it has.
    fn tests(&self, program: &Program, request: &ListRequest) -> Result<Vec<Listed>, EngineError> {
        let selection = self.selection(&request.select)?;
        let options = PlanOptions { today: Some(self.clock.today().to_string()), ..PlanOptions::default() };
        let plan = vc_planner::plan(program, &selection, &options)
            .map_err(|error| EngineError::Selection { reason: error.to_string(), help: error.help() })?;
        let skipped: BTreeSet<TestId> = plan.skipped.iter().map(|skipped| skipped.test).collect();
        let runs: BTreeSet<TestId> = plan.files.iter().flat_map(|file| &file.instances).map(|i| i.test).collect();
        let picked = program.tests.values().filter(|test| runs.contains(&test.id) || skipped.contains(&test.id));
        let listed = picked.map(|test| listed_test(program, test, skipped.contains(&test.id)));
        Ok(listed.filter(|test| !request.skipped || test.skipped).collect())
    }
}

fn listed_test(program: &Program, test: &Test, skipped: bool) -> Listed {
    let file = program.test_files.get(&test.file);
    let file_tags = file.into_iter().flat_map(|file| &file.tags);
    let mut tags: Vec<String> = Vec::new();
    for tag in file_tags.chain(&test.tags) {
        if !tags.contains(&tag.0) {
            tags.push(tag.0.clone());
        }
    }
    Listed {
        id: text(program, test.id.sym()),
        name: test.name.clone(),
        path: program.files.get(test.span.file).map(|f| f.path.as_str().to_owned()).unwrap_or_default(),
        tags,
        skipped,
    }
}

fn apis(program: &Program) -> Vec<Listed> {
    let api = |api: &Api| Listed {
        id: text(program, api.id.sym()),
        name: api.name.clone(),
        path: program.files.get(api.span.file).map(|f| f.path.as_str().to_owned()).unwrap_or_default(),
        tags: Vec::new(),
        skipped: false,
    };
    program.apis.values().map(api).collect()
}

fn files(program: &Program) -> Vec<Listed> {
    let all = (0..).map(SourceFileId::new).map_while(|id| program.files.get(id));
    let listed = |file: &SourceFile| {
        let id = file.path.file_id()?;
        Some(Listed { id, name: None, path: file.path.as_str().to_owned(), tags: Vec::new(), skipped: false })
    };
    all.filter_map(listed).collect()
}

fn text(program: &Program, sym: Sym) -> String {
    program.str(sym).unwrap_or_default().to_owned()
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
