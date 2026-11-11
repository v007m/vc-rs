//! `vc list tests|apis|files`: the definitions, for people and scripts (D130).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};
use vc_model::{Api, Examples, FileId, FileKind, Program, SourceFileId, SourcePath, SourceSpan, Sym, Test, TestId};
use vc_planner::{PlanError, PlanOptions};

use crate::list_detail::{self, ApiCall};
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
    /// Where a test or api is written; `None` for a file.
    pub span: Option<SourceSpan>,
    /// The apis a test calls, in order.
    pub calls: Vec<ApiCall>,
    /// A test's step count; 0 for a test with one `call`.
    pub steps: u32,
    /// The vars an api declares, in order.
    pub vars: Vec<String>,
}

impl Listed {
    /// The kind of the file it is in, as its path names it; `None` for a path that names none.
    #[must_use]
    pub fn kind(&self) -> Option<FileKind> {
        SourcePath::new(&self.path).ok().as_ref().and_then(FileKind::from_path)
    }
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
    /// Lists the tests (skipped ones included), the apis or the definition files `request`
    /// selects.
    ///
    /// # Errors
    ///
    /// [`EngineError::Invalid`] when the project has errors, and [`EngineError::Selection`]
    /// when a selector cannot be read or matches nothing, or a tag filters apis.
    pub fn list(&self, request: &ListRequest) -> Result<ListReport, EngineError> {
        let (program, notices) = self.program()?;
        let items = match request.kind {
            ListKind::Tests => self.tests(&program, request)?,
            ListKind::Apis => self.apis(&program, request)?,
            ListKind::Files => self.files(&program, request)?,
        };
        Ok(ListReport { items, notices })
    }

    /// Every api, then every test (skipped ones included), from one read of the project: what
    /// an app that browses the project shows, without compiling it once per kind.
    ///
    /// # Errors
    ///
    /// [`EngineError::Invalid`] when the project has errors.
    pub fn catalog(&self) -> Result<ListReport, EngineError> {
        let (program, notices) = self.program()?;
        let request = ListRequest::default();
        let mut items = self.apis(&program, &request)?;
        items.extend(self.tests(&program, &request)?);
        Ok(ListReport { items, notices })
    }

    /// The selected tests, in project order, each once however many instances it has.
    fn tests(&self, program: &Program, request: &ListRequest) -> Result<Vec<Listed>, EngineError> {
        let selection = self.selection(&request.select)?;
        let (today, example_rows) = (Some(self.clock.today().to_string()), unread_rows(program));
        let options = PlanOptions { today, example_rows, ..PlanOptions::default() };
        let plan = vc_planner::plan(program, &selection, &options).map_err(|error| selection_error(&error))?;
        let skipped: BTreeSet<TestId> = plan.skipped.iter().map(|skipped| skipped.test).collect();
        let runs: BTreeSet<TestId> = plan.files.iter().flat_map(|file| &file.instances).map(|i| i.test).collect();
        let picked = program.tests.values().filter(|test| runs.contains(&test.id) || skipped.contains(&test.id));
        let listed = picked.map(|test| listed_test(program, test, skipped.contains(&test.id)));
        Ok(listed.filter(|test| !request.skipped || test.skipped).collect())
    }

    /// The selected apis. They have no tags, so `-t` is refused rather than listing nothing.
    fn apis(&self, program: &Program, request: &ListRequest) -> Result<Vec<Listed>, EngineError> {
        if !request.select.tags.is_empty() {
            let help = Some("list the tests or files with `-t`, or select apis by ID, file or path".to_owned());
            return Err(EngineError::Selection { reason: "apis have no tags".into(), help });
        }
        let selection = self.selection(&request.select)?;
        let ids = vc_planner::select_apis(program, &selection).map_err(|error| selection_error(&error))?;
        let api = |api: &Api| Listed {
            name: api.name.clone(),
            span: Some(api.span),
            vars: list_detail::vars(program, api),
            ..listed(program, api.span.file, text(program, api.id.sym()))
        };
        Ok(ids.iter().filter_map(|id| program.apis.get(id)).map(api).collect())
    }

    /// The selected definition files.
    fn files(&self, program: &Program, request: &ListRequest) -> Result<Vec<Listed>, EngineError> {
        let selection = self.selection(&request.select)?;
        let ids = vc_planner::select_files(program, &selection).map_err(|error| selection_error(&error))?;
        let file = |id: &SourceFileId| {
            let path = &program.files.get(*id)?.path;
            let file_id = path.file_id()?;
            let name = FileKind::from_path(path).and_then(|kind| file_name(program, kind, &file_id));
            Some(Listed { name, ..listed(program, *id, file_id) })
        };
        Ok(ids.iter().filter_map(file).collect())
    }
}

/// The `name` the `kind` file `id` declares, if it has one.
fn file_name(program: &Program, kind: FileKind, id: &str) -> Option<String> {
    let named = |file: FileId, name: Option<&str>| (program.str(file.sym()) == Some(id)).then_some(name);
    let found = match kind {
        FileKind::Test => program.test_files.values().find_map(|file| named(file.id, file.name.as_deref())),
        FileKind::Api => program.api_groups.values().find_map(|group| named(group.id, group.name.as_deref())),
        FileKind::Load => program.loads.values().find_map(|load| named(load.id, load.name.as_deref())),
        _ => None,
    };
    found.flatten().map(str::to_owned)
}

/// One row for each test whose rows come from a file or a template: a list says which tests run,
/// not how many times, so their rows are not read.
fn unread_rows(program: &Program) -> BTreeMap<TestId, u32> {
    let unread = |test: &&Test| matches!(test.examples, Some(Examples::File(_) | Examples::Template(_)));
    program.tests.values().filter(unread).map(|test| (test.id, 1)).collect()
}

fn selection_error(error: &PlanError) -> EngineError {
    EngineError::Selection { reason: error.to_string(), help: error.help() }
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
        span: Some(test.span),
        calls: list_detail::calls(program, test),
        steps: list_detail::steps(test),
        vars: Vec::new(),
    }
}

/// An unnamed, untagged, unskipped definition `id` in `file`.
fn listed(program: &Program, file: SourceFileId, id: String) -> Listed {
    let path = program.files.get(file).map(|f| f.path.as_str().to_owned()).unwrap_or_default();
    Listed {
        id,
        name: None,
        path,
        tags: Vec::new(),
        skipped: false,
        span: None,
        calls: Vec::new(),
        steps: 0,
        vars: Vec::new(),
    }
}

fn text(program: &Program, sym: Sym) -> String {
    program.str(sym).unwrap_or_default().to_owned()
}

#[cfg(test)]
#[path = "list_tests.rs"]
mod tests;
