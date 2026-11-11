use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    ActionId, ActionKind, CheckKind, CheckResult, FileId, InstanceId, OwnerKey, Plan, Program, Segment, SourceSpan,
    Sym, TestId, TestResult,
};

/// Text shown for an ID the program does not know.
const UNKNOWN: &str = "?";

/// Where something is in the project's sources.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Location {
    /// The file's project path.
    pub path: String,
    /// The line, from 1.
    pub line: u32,
    /// The column, from 1.
    pub column: u32,
}

/// An instance of the plan: the test it runs, its example row and its repeat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct Planned {
    /// The instance.
    pub instance: InstanceId,
    /// The test it runs.
    pub test: TestId,
    /// Its example row, from 0, when the test has examples.
    pub example_row: Option<u32>,
    /// Its repeat, from 0.
    pub repeat_index: u32,
}

/// Turns the IDs in events into the text reports show, from the program and the plan.
///
/// IDs the program does not know are shown as `?` (or by their index), never an error.
#[derive(Clone, Debug)]
pub struct Names {
    program: Arc<Program>,
    /// Each instance's example row and repeat index.
    instances: HashMap<InstanceId, (Option<u32>, u32)>,
    /// The plan's instances in plan order.
    planned: Vec<Planned>,
    /// The tests the plan runs more than once, by `repeat` or `--repeat`.
    repeated: HashSet<TestId>,
    /// The `name` of each example row that gives one.
    row_names: BTreeMap<(TestId, u32), String>,
}

impl Names {
    /// Names for a run of `plan` over `program`.
    #[must_use]
    pub fn new(program: Arc<Program>, plan: &Plan) -> Self {
        let planned: Vec<Planned> = plan
            .files
            .iter()
            .flat_map(|file| &file.instances)
            .map(|i| Planned { instance: i.id, test: i.test, example_row: i.example_row, repeat_index: i.repeat_index })
            .collect();
        let instances = planned.iter().map(|p| (p.instance, (p.example_row, p.repeat_index))).collect();
        let repeated = planned.iter().filter(|p| p.repeat_index > 0).map(|p| p.test).collect();
        Self { program, instances, planned, repeated, row_names: plan.row_names.clone() }
    }

    /// The plan's instances, in plan order.
    #[must_use]
    pub fn planned(&self) -> &[Planned] {
        &self.planned
    }

    /// The program the names come from.
    #[must_use]
    pub fn program(&self) -> &Program {
        &self.program
    }

    /// The text behind `sym`.
    #[must_use]
    pub fn text(&self, sym: Sym) -> &str {
        self.program.str(sym).unwrap_or(UNKNOWN)
    }

    /// A test's full ID: `users.crud#getsUser`.
    #[must_use]
    pub fn test(&self, test: TestId) -> &str {
        self.text(test.sym())
    }

    /// The file a test is in: from the program, else the full ID before its `#`.
    #[must_use]
    pub fn file_of(&self, test: TestId) -> String {
        if let Some(file) = self.program.tests.get(&test).map(|t| t.file) {
            return self.text(file.sym()).to_owned();
        }
        let id = self.test(test);
        id.split_once('#').map_or(id, |(file, _)| file).to_owned()
    }

    /// A file ID as text.
    #[must_use]
    pub fn file(&self, file: FileId) -> &str {
        self.text(file.sym())
    }

    /// A test's `name`, when it gives one.
    #[must_use]
    pub fn test_name(&self, test: TestId) -> Option<&str> {
        self.program.tests.get(&test).and_then(|found| found.name.as_deref())
    }

    /// The `name` of a test's file, when it gives one.
    #[must_use]
    pub fn file_name_of(&self, test: TestId) -> Option<&str> {
        let file = self.program.tests.get(&test)?.file;
        self.program.test_files.get(&file).and_then(|found| found.name.as_deref())
    }

    /// The `name` of profile `id`, when it gives one.
    #[must_use]
    pub fn profile_name(&self, id: &str) -> Option<&str> {
        let found = self.program.profiles.values().find(|profile| self.text(profile.id.sym()) == id);
        found.and_then(|profile| profile.name.as_deref())
    }

    /// The instance as people know it: `users.crud#rejectsBadEmail [row 2, repeat 1]`, numbered
    /// from 1, or `users.crud#rejectsBadEmail › missing @ [repeat 1]` for a row with a `name`;
    /// the repeat shows on every instance of a test the plan repeats, and on none of one it
    /// does not.
    #[must_use]
    pub fn instance(&self, result: &TestResult) -> String {
        self.label(result.test, result.instance)
    }

    /// The instance's label followed by its test's `name`, as the console and the HTML report
    /// show it: `users.crud#listsUsers — Lists every user`; the bare label when it has none.
    #[must_use]
    pub fn titled(&self, result: &TestResult) -> String {
        let label = self.instance(result);
        match self.test_name(result.test) {
            Some(name) => format!("{label} — {name}"),
            None => label,
        }
    }

    /// The label of [`Names::instance`] for `test` run as `instance`.
    #[must_use]
    pub fn label(&self, test: TestId, instance: Option<InstanceId>) -> String {
        let mut label = self.test(test).to_owned();
        let slot = instance.and_then(|id| self.instances.get(&id));
        let Some(&(row, repeat)) = slot else { return label };
        let named = row.and_then(|row| self.row_names.get(&(test, row)));
        if let Some(name) = named {
            let _ = write!(label, " › {name}");
        }
        let row = row.filter(|_| named.is_none()).map(|r| format!("row {}", u64::from(r) + 1));
        let repeat = self.repeated.contains(&test).then(|| format!("repeat {}", u64::from(repeat) + 1));
        let marks: Vec<String> = row.into_iter().chain(repeat).collect();
        if !marks.is_empty() {
            let _ = write!(label, " [{}]", marks.join(", "));
        }
        label
    }

    /// The example row and repeat index of a result's instance; `(None, 0)` without one.
    #[must_use]
    pub fn slot(&self, result: &TestResult) -> (Option<u32>, u32) {
        result.instance.and_then(|id| self.instances.get(&id)).copied().unwrap_or((None, 0))
    }

    /// The api a `call` action calls.
    #[must_use]
    pub fn api_of(&self, action: ActionId) -> Option<String> {
        let found = usize::try_from(action.index()).ok().and_then(|at| self.program.actions.get(at))?;
        if let ActionKind::Call(api) = &found.kind { Some(self.text(api.sym()).to_owned()) } else { None }
    }

    /// An action as people know it: its `id`, else what it runs (`call users#create`).
    #[must_use]
    pub fn action(&self, action: ActionId) -> String {
        let found = usize::try_from(action.index()).ok().and_then(|at| self.program.actions.get(at));
        let Some(found) = found else { return format!("action {}", action.index()) };
        if let Some(id) = found.id {
            return self.text(id).to_owned();
        }
        match &found.kind {
            ActionKind::Call(api) => format!("call {}", self.text(api.sym())),
            ActionKind::Test(test) => format!("test {}", self.test(*test)),
            ActionKind::Function(hook) => format!("function {}#{}", hook.file, hook.export),
            ActionKind::Command(_) => "command".to_owned(),
            ActionKind::Inline(_) => "request".to_owned(),
        }
    }

    /// The frame an action ran in: `run`, `file users.crud`, `instance 3`.
    #[must_use]
    pub fn owner(&self, owner: OwnerKey) -> String {
        match owner {
            OwnerKey::Run => "run".to_owned(),
            OwnerKey::File(file) => format!("file {}", self.file(file)),
            OwnerKey::Instance(instance) => format!("instance {}", instance.index()),
        }
    }

    /// Where `span` points, when its file is in the program.
    #[must_use]
    pub fn location(&self, span: SourceSpan) -> Option<Location> {
        let file = self.program.files.get(span.file)?;
        Some(Location { path: file.path.as_str().to_owned(), line: span.start.line, column: span.start.column })
    }

    /// Where a test is written, when the program has it.
    #[must_use]
    pub fn test_location(&self, test: TestId) -> Option<Location> {
        self.program.tests.get(&test).and_then(|t| self.location(t.span))
    }

    /// The test's tags, the file's first.
    #[must_use]
    pub fn tags(&self, test: TestId) -> Vec<String> {
        let Some(found) = self.program.tests.get(&test) else { return Vec::new() };
        let file = self.program.test_files.get(&found.file).map(|f| f.tags.as_slice()).unwrap_or_default();
        let mut tags: Vec<String> = Vec::new();
        for tag in file.iter().chain(&found.tags) {
            if !tags.contains(&tag.0) {
                tags.push(tag.0.clone());
            }
        }
        tags
    }
}

/// The `expect` key a check was written under: `status`, `body.match.items[*].price`.
#[must_use]
pub fn check_label(check: &CheckResult) -> String {
    match check_parts(check) {
        (key, Some(target)) => format!("{key}.{target}"),
        (key, None) => key.to_owned(),
    }
}

/// The `expect` key of a check and the path it targets, when the key takes one:
/// `("body.match", Some("errors[0].field"))`.
#[must_use]
pub fn check_parts(check: &CheckResult) -> (&'static str, Option<String>) {
    let target = check.target.as_ref().map(|path| path_text(&path.segments)).unwrap_or_default();
    let with_target = matches!(check.kind, CheckKind::Header | CheckKind::Match | CheckKind::Timing | CheckKind::Cel);
    (check.kind.key(), (with_target && !target.is_empty()).then_some(target))
}

/// A path as written: `items[*].price`.
fn path_text(segments: &[Segment]) -> String {
    let mut out = String::new();
    for segment in segments {
        match segment {
            Segment::Key(key) if out.is_empty() => out.push_str(key),
            Segment::Key(key) => {
                out.push('.');
                out.push_str(key);
            }
            Segment::Index(index) => {
                let _ = write!(out, "[{index}]");
            }
            Segment::All => out.push_str("[*]"),
        }
    }
    out
}
