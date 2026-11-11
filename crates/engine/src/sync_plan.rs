//! What `vc sync` does to one api file: each operation's three versions (the snapshot, the spec
//! now, the api file) compared by the rules in `sync_rules.rs`, as steps for `sync_apply.rs`
//! and a line each for the user.

use indexmap::IndexMap;
use serde_json::Value;
use vc_interfaces::DraftApi;

use crate::sync_rules::{Field, Operation, field, operation};

/// The fields sync owns of one api, as text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Owned {
    /// `method`, upper case; GET when it is not written.
    pub method: String,
    /// `path`, as template text.
    pub path: String,
    /// `query`, by name.
    pub query: IndexMap<String, String>,
    /// The `content-type` header.
    pub content_type: Option<String>,
}

/// One api of the api file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mine {
    pub id: String,
    pub operation_id: Option<String>,
    pub owned: Owned,
    /// The names of its `vars`.
    pub vars: Vec<String>,
    pub deprecated: bool,
}

/// One change to the api file; an api is named by its ID in the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Sets the api's `key` (`method`, `path`), adding it when it is not written.
    Set { api: String, key: &'static str, value: String },
    /// Sets `key` in the api's map `map` (`query`, `vars`, `headers`), adding either as needed.
    Put { api: String, map: &'static str, key: String, value: Value },
    /// Removes `key` from the api's map `map`.
    Remove { api: String, map: &'static str, key: String },
    /// Marks the api `deprecated: true`.
    Deprecate { api: String },
    /// Appends a new api.
    Append(Box<DraftApi>),
}

/// How a conflict on one api is settled: `--take spec`, or `--take mine` and `--resolved`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[expect(clippy::exhaustive_enums, reason = "the two sides of a three-way merge (D160)")]
pub enum SyncSide {
    /// `--take spec`: the spec's values replace the file's.
    Spec,
    /// `--take mine` or `--resolved`: the file stays as it is, and the snapshot takes the spec.
    Mine,
}

/// The three versions of one api file, and the conflicts settled by api ID.
pub struct Versions<'a> {
    pub file_id: &'a str,
    pub base: &'a [DraftApi],
    pub theirs: &'a [DraftApi],
    pub mine: &'a [Mine],
    pub settled: &'a IndexMap<String, SyncSide>,
}

/// Everything sync does to one api file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub steps: Vec<Step>,
    pub lines: Vec<String>,
    /// The operation IDs left in conflict, whose snapshot entries stay as they were.
    pub conflicted: Vec<String>,
}

/// A field sync owns, the same for every api.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Key {
    Method,
    Path,
    ContentType,
    Query(String),
}

/// What the rules make of one field: the spec's value to take, a conflict, or nothing.
type Change = Field<Option<String>>;

/// The plan for one api file: operations in spec order, then the file's own.
pub fn plan(versions: &Versions<'_>) -> Plan {
    let mut plan = Plan::default();
    for op in operations(versions) {
        let base = by_operation(versions.base, &op);
        let theirs = by_operation(versions.theirs, &op);
        let mine = versions.mine.iter().find(|api| api.operation_id.as_deref() == Some(op.as_str()));
        match (operation(base.is_some(), theirs.is_some(), mine.is_some()), theirs, mine) {
            (Operation::Add, Some(theirs), _) => add(&mut plan, versions, theirs),
            (Operation::Deprecate, _, Some(mine)) => deprecate(&mut plan, versions.file_id, mine),
            (Operation::Merge, Some(theirs), Some(mine)) => {
                Merge { versions, mine, theirs, base: base.map(owned) }.settle(&mut plan, &op);
                optional_query(&mut plan, versions.file_id, (base, theirs, mine));
            }
            _ => {}
        }
    }
    plan
}

fn operations(versions: &Versions<'_>) -> Vec<String> {
    let spec = versions.theirs.iter().filter_map(|api| api.operation_id.clone());
    let file = versions.mine.iter().filter_map(|api| api.operation_id.clone());
    let mut all: Vec<String> = Vec::new();
    for op in spec.chain(file) {
        if !all.contains(&op) {
            all.push(op);
        }
    }
    all
}

fn by_operation<'d>(apis: &'d [DraftApi], op: &str) -> Option<&'d DraftApi> {
    apis.iter().find(|api| api.operation_id.as_deref() == Some(op))
}

/// The fields of a drafted api.
pub fn owned(api: &DraftApi) -> Owned {
    Owned {
        method: api.method.name().to_owned(),
        path: api.path.clone(),
        query: api.query.clone(),
        content_type: api.content_type.clone(),
    }
}

fn add(plan: &mut Plan, versions: &Versions<'_>, theirs: &DraftApi) {
    let full = format!("{}#{}", versions.file_id, theirs.id);
    let what = format!("{} {}", theirs.method.name(), theirs.path);
    if versions.mine.iter().any(|api| api.id == theirs.id) {
        plan.lines.push(format!("! {full} new operation {what}: the ID is taken, so add it by hand"));
        plan.conflicted.extend(theirs.operation_id.clone());
        return;
    }
    plan.lines.push(format!("+ {full} new operation {what}"));
    plan.steps.push(Step::Append(Box::new(theirs.clone())));
}

fn deprecate(plan: &mut Plan, file_id: &str, mine: &Mine) {
    if !mine.deprecated {
        plan.lines.push(format!("- {file_id}#{} removed from the spec: kept, marked deprecated", mine.id));
        plan.steps.push(Step::Deprecate { api: mine.id.clone() });
    }
}

/// A line for each optional query param the spec added or dropped since the snapshot. Sync sends
/// no optional param, so these are listed, not taken: an added one the file does not send, and
/// a dropped one it still sends.
fn optional_query(plan: &mut Plan, file_id: &str, (base, theirs, mine): (Option<&DraftApi>, &DraftApi, &Mine)) {
    let Some(base) = base else { return };
    let full = format!("{file_id}#{}", mine.id);
    let sent = |name: &String| mine.owned.query.contains_key(name);
    let added = theirs.optional_query.iter().filter(|name| !base.optional_query.contains(*name) && !sent(*name));
    for name in added {
        plan.lines
            .push(format!("~ {full} optional query {name} added to the spec: not sent, add `query.{name}` to send it"));
    }
    let gone = |name: &&String| !theirs.optional_query.contains(*name) && !theirs.query.contains_key(*name);
    for name in base.optional_query.iter().filter(gone).filter(|name| sent(*name)) {
        plan.lines.push(format!("~ {full} optional query {name} dropped from the spec: the file still sends it"));
    }
}

/// One api in all three versions, or new in both the spec and the file.
struct Merge<'v, 'a> {
    versions: &'v Versions<'a>,
    mine: &'v Mine,
    theirs: &'v DraftApi,
    base: Option<Owned>,
}

impl Merge<'_, '_> {
    fn settle(self, plan: &mut Plan, op: &str) {
        let full = format!("{}#{}", self.versions.file_id, self.mine.id);
        let fields = self.fields();
        let settled = self.versions.settled.get(&self.mine.id).copied();
        let open: Vec<String> = fields.iter().filter(|(_, f)| *f == Field::Conflict).map(|(k, _)| label(k)).collect();
        if !open.is_empty() && settled.is_none() {
            plan.lines.push(format!("! {full} conflict: the spec and the file both changed {}", open.join(", ")));
            plan.lines.push(format!(
                "  resolve: vc sync {} --take spec {full} | --take mine {full} | edit, then --resolved {full}",
                self.versions.file_id
            ));
            plan.conflicted.push(op.to_owned());
            return;
        }
        let mut vars = self.mine.vars.clone();
        for (key, change) in fields {
            match (change, settled) {
                (Field::Take(value), _) => self.take(plan, (&key, value), &mut vars),
                (Field::Conflict, Some(SyncSide::Spec)) => self.take(plan, (&key, self.value(&key)), &mut vars),
                (Field::Conflict, _) => plan.lines.push(format!("= {full} kept your {}", label(&key))),
                (Field::Keep, _) => {}
            }
        }
    }

    /// Each owned field with what the rules make of it.
    fn fields(&self) -> Vec<(Key, Change)> {
        let theirs = owned(self.theirs);
        let mut keys = vec![Key::Method, Key::Path, Key::ContentType];
        let names = theirs.query.keys().chain(self.mine.owned.query.keys());
        let names = names.chain(self.base.iter().flat_map(|base| base.query.keys()));
        for name in names {
            let key = Key::Query(name.clone());
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        keys.into_iter()
            .map(|key| {
                let base = self.base.as_ref().map(|base| read(base, &key));
                let change = field(base.as_ref(), &read(&theirs, &key), &read(&self.mine.owned, &key));
                (key, change)
            })
            .collect()
    }

    fn value(&self, key: &Key) -> Option<String> {
        read(&owned(self.theirs), key)
    }

    /// The steps that give the api the spec's `value` for `key`, with the vars it reads.
    fn take(&self, plan: &mut Plan, (key, value): (&Key, Option<String>), vars: &mut Vec<String>) {
        let api = self.mine.id.clone();
        let was = read(&self.mine.owned, key).unwrap_or_else(|| "none".to_owned());
        let full = format!("{}#{api}", self.versions.file_id);
        plan.lines.push(format!("↑ {full} {} {was} → {}", label(key), value.as_deref().unwrap_or("none")));
        plan.steps.push(step(api.clone(), key, value.clone()));
        for name in read_vars(value.as_deref().unwrap_or_default()) {
            if let Some(default) = self.theirs.vars.get(&name).filter(|_| !vars.contains(&name)) {
                plan.steps.push(Step::Put { api: api.clone(), map: "vars", key: name.clone(), value: default.clone() });
                vars.push(name);
            }
        }
    }
}

fn step(api: String, key: &Key, value: Option<String>) -> Step {
    let (map, name) = match key {
        Key::Method => return Step::Set { api, key: "method", value: value.unwrap_or_default() },
        Key::Path => return Step::Set { api, key: "path", value: value.unwrap_or_default() },
        Key::ContentType => ("headers", "content-type".to_owned()),
        Key::Query(name) => ("query", name.clone()),
    };
    match value {
        Some(value) => Step::Put { api, map, key: name, value: Value::String(value) },
        None => Step::Remove { api, map, key: name },
    }
}

fn read(owned: &Owned, key: &Key) -> Option<String> {
    match key {
        Key::Method => Some(owned.method.clone()),
        Key::Path => Some(owned.path.clone()),
        Key::ContentType => owned.content_type.clone(),
        Key::Query(name) => owned.query.get(name).cloned(),
    }
}

fn label(key: &Key) -> String {
    match key {
        Key::Method => "method".to_owned(),
        Key::Path => "path".to_owned(),
        Key::ContentType => "content type".to_owned(),
        Key::Query(name) => format!("query {name}"),
    }
}

/// The names `${var.<name>}` reads in `text`.
fn read_vars(text: &str) -> Vec<String> {
    text.split("${var.")
        .skip(1)
        .filter_map(|rest| rest.split_once('}'))
        .map(|(name, _)| name.split(['.', '[', ' ']).next().unwrap_or_default().to_owned())
        .collect()
}

#[cfg(test)]
#[path = "sync_plan_tests.rs"]
mod tests;
