use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;

use vc_model::{
    ActionId, ActionKind, CheckKind, CheckResult, FileId, InstanceId, OwnerKey, Plan, Program, Segment, SourceSpan,
    Sym, TestId, TestResult,
};

/// Text shown for an ID the program does not know.
const UNKNOWN: &str = "?";

/// Where a span points, as people read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    /// The project-relative file path.
    pub path: String,
    /// The line, from 1.
    pub line: u32,
    /// The column, from 1.
    pub column: u32,
}

/// Turns the IDs in events into the text reports show, from the program and the plan.
///
/// IDs the program does not know are shown as `?` (or by their index), never an error.
#[derive(Clone, Debug)]
pub struct Names {
    program: Arc<Program>,
    /// Each instance's example row and repeat index.
    instances: HashMap<InstanceId, (Option<u32>, u32)>,
}

impl Names {
    /// Names for a run of `plan` over `program`.
    pub fn new(program: Arc<Program>, plan: &Plan) -> Self {
        let instances = plan.files.iter().flat_map(|file| &file.instances);
        let instances = instances.map(|i| (i.id, (i.example_row, i.repeat_index))).collect();
        Self { program, instances }
    }

    /// The program the names come from.
    pub fn program(&self) -> &Program {
        &self.program
    }

    /// The text behind `sym`.
    pub fn text(&self, sym: Sym) -> &str {
        self.program.str(sym).unwrap_or(UNKNOWN)
    }

    /// A test's full ID: `users.crud#getsUser`.
    pub fn test(&self, test: TestId) -> &str {
        self.text(test.sym())
    }

    /// The file a test is in: from the program, else the full ID before its `#`.
    pub fn file_of(&self, test: TestId) -> String {
        if let Some(file) = self.program.tests.get(&test).map(|t| t.file) {
            return self.text(file.sym()).to_owned();
        }
        let id = self.test(test);
        id.split_once('#').map_or(id, |(file, _)| file).to_owned()
    }

    /// A file ID as text.
    pub fn file(&self, file: FileId) -> &str {
        self.text(file.sym())
    }

    /// The instance as people know it: `users.crud#rejectsBadEmail [row 2, repeat 1]`, numbered
    /// from 1; the repeat shows only when the test repeats.
    pub fn instance(&self, result: &TestResult) -> String {
        let mut label = self.test(result.test).to_owned();
        let slot = result.instance.and_then(|id| self.instances.get(&id));
        let Some(&(row, repeat)) = slot else { return label };
        let repeats = repeat > 0 || self.program.tests.get(&result.test).is_some_and(|t| t.repeat > 1);
        let row = row.map(|r| format!("row {}", u64::from(r) + 1));
        let repeat = repeats.then(|| format!("repeat {}", u64::from(repeat) + 1));
        let marks: Vec<String> = row.into_iter().chain(repeat).collect();
        if !marks.is_empty() {
            let _ = write!(label, " [{}]", marks.join(", "));
        }
        label
    }

    /// An action as people know it: its `id`, else what it runs (`call users#create`).
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
    pub fn owner(&self, owner: OwnerKey) -> String {
        match owner {
            OwnerKey::Run => "run".to_owned(),
            OwnerKey::File(file) => format!("file {}", self.file(file)),
            OwnerKey::Instance(instance) => format!("instance {}", instance.index()),
        }
    }

    /// Where `span` points, when its file is in the program.
    pub fn location(&self, span: SourceSpan) -> Option<Location> {
        let file = self.program.files.get(span.file)?;
        Some(Location { path: file.path.as_str().to_owned(), line: span.start.line, column: span.start.column })
    }

    /// Where a test is written, when the program has it.
    pub fn test_location(&self, test: TestId) -> Option<Location> {
        self.program.tests.get(&test).and_then(|t| self.location(t.span))
    }

    /// The test's tags, the file's first.
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
pub fn check_label(check: &CheckResult) -> String {
    let target = check.target.as_ref().map(|path| path_text(&path.segments)).unwrap_or_default();
    let (key, with_target) = match check.kind {
        CheckKind::Status => ("status", false),
        CheckKind::Header => ("headers", true),
        CheckKind::Schema => ("body.schema", false),
        CheckKind::Match => ("body.match", true),
        CheckKind::Equals => ("body.equals", false),
        CheckKind::Size => ("size", false),
        CheckKind::Timing => ("timing", true),
        CheckKind::Cel => ("cel", true),
        CheckKind::Custom => ("custom", false),
    };
    if with_target && !target.is_empty() { format!("{key}.{target}") } else { key.to_owned() }
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

#[cfg(test)]
#[path = "names_tests.rs"]
mod tests;
