use indexmap::IndexMap;
use serde_json::json;
use vc_interfaces::DraftApi;
use vc_model::Method;

use super::{Mine, Plan, Step, SyncSide, Versions, owned, plan};

fn draft(op: &str, method: Method, path: &str) -> DraftApi {
    let id = op.strip_suffix("User").unwrap_or(op).to_owned();
    DraftApi { id, method, path: path.to_owned(), operation_id: Some(op.to_owned()), ..DraftApi::default() }
}

fn mine(api: &DraftApi) -> Mine {
    Mine {
        id: api.id.clone(),
        operation_id: api.operation_id.clone(),
        owned: owned(api),
        vars: api.vars.keys().cloned().collect(),
        deprecated: false,
    }
}

/// The three versions of `users`, with the conflicts settled.
#[derive(Default)]
struct Case {
    base: Vec<DraftApi>,
    theirs: Vec<DraftApi>,
    mine: Vec<Mine>,
    settled: IndexMap<String, SyncSide>,
}

impl Case {
    fn new(base: DraftApi, theirs: DraftApi, file: &DraftApi) -> Self {
        Self { base: vec![base], theirs: vec![theirs], mine: vec![mine(file)], ..Self::default() }
    }

    fn settled(mut self, id: &str, side: SyncSide) -> Self {
        self.settled.insert(id.to_owned(), side);
        self
    }

    fn plan(&self) -> Plan {
        plan(&Versions {
            file_id: "users",
            base: &self.base,
            theirs: &self.theirs,
            mine: &self.mine,
            settled: &self.settled,
        })
    }
}

const PATH: &str = "/users/${var.userId}";

fn update(method: Method) -> DraftApi {
    draft("updateUser", method, PATH)
}

fn set(key: &'static str, value: &str) -> Step {
    Step::Set { api: "update".to_owned(), key, value: value.to_owned() }
}

#[test]
fn spec_unchanged_and_file_changed_keeps_the_edit() {
    let edited = draft("updateUser", Method::Put, "/people/${var.userId}");
    assert_eq!(Case::new(update(Method::Put), update(Method::Put), &edited).plan(), Plan::default());
}

#[test]
fn spec_changed_and_file_unchanged_updates_the_api() {
    let planned = Case::new(update(Method::Put), update(Method::Patch), &update(Method::Put)).plan();
    assert_eq!(planned.steps, [set("method", "PATCH")]);
    assert_eq!(planned.lines, ["↑ users#update method PUT → PATCH"]);
    assert_eq!(planned.conflicted, Vec::<String>::new());
}

#[test]
fn both_changed_the_same_way_is_nothing_to_do() {
    assert_eq!(Case::new(update(Method::Put), update(Method::Patch), &update(Method::Patch)).plan(), Plan::default());
}

#[test]
fn both_changed_differently_is_a_conflict_left_as_it_is() {
    let planned = Case::new(update(Method::Put), update(Method::Patch), &update(Method::Post)).plan();
    assert_eq!(planned.steps, Vec::<Step>::new());
    assert_eq!(planned.conflicted, ["updateUser"]);
    assert_eq!(
        planned.lines,
        [
            "! users#update conflict: the spec and the file both changed method",
            "  resolve: vc sync users --take spec users#update | --take mine users#update | edit, then --resolved users#update",
        ]
    );
}

#[test]
fn a_conflict_settled_for_the_spec_takes_it_and_for_mine_keeps_the_file() {
    let case = || Case::new(update(Method::Put), update(Method::Patch), &update(Method::Post));
    let spec = case().settled("update", SyncSide::Spec).plan();
    assert_eq!((spec.steps, spec.conflicted.len()), (vec![set("method", "PATCH")], 0));
    assert_eq!(spec.lines, ["↑ users#update method POST → PATCH"]);
    let kept = case().settled("update", SyncSide::Mine).plan();
    assert_eq!((kept.steps.len(), kept.conflicted.len()), (0, 0));
    assert_eq!(kept.lines, ["= users#update kept your method"]);
}

#[test]
fn a_new_operation_adds_an_api_unless_its_id_is_taken() {
    let restore = draft("restore", Method::Post, "/users/${var.userId}/restore");
    let case = Case { theirs: vec![restore.clone()], ..Case::default() };
    let planned = case.plan();
    assert_eq!(planned.steps, [Step::Append(Box::new(restore.clone()))]);
    assert_eq!(planned.lines, ["+ users#restore new operation POST /users/${var.userId}/restore"]);
    let taken = Mine { id: "restore".to_owned(), ..Mine::default() };
    let clash = Case { theirs: vec![restore], mine: vec![taken], ..Case::default() }.plan();
    assert_eq!(clash.steps, Vec::<Step>::new());
    assert_eq!(clash.conflicted, ["restore"]);
    assert_eq!(
        clash.lines,
        ["! users#restore new operation POST /users/${var.userId}/restore: the ID is taken, so add it by hand"]
    );
}

#[test]
fn a_removed_operation_is_kept_and_marked_deprecated_once() {
    let legacy = draft("legacyImport", Method::Post, "/import");
    let case = |deprecated| Case {
        base: vec![legacy.clone()],
        mine: vec![Mine { deprecated, ..mine(&legacy) }],
        ..Case::default()
    };
    let planned = case(false).plan();
    assert_eq!(planned.steps, [Step::Deprecate { api: "legacyImport".to_owned() }]);
    assert_eq!(planned.lines, ["- users#legacyImport removed from the spec: kept, marked deprecated"]);
    assert_eq!(case(true).plan(), Plan::default());
}

#[test]
fn an_api_the_user_removed_stays_removed() {
    let case = Case { base: vec![update(Method::Put)], theirs: vec![update(Method::Patch)], ..Case::default() };
    assert_eq!(case.plan(), Plan::default());
}

#[test]
fn a_new_query_parameter_brings_the_var_it_reads() {
    let list = draft("listUsers", Method::Get, "/users");
    let mut paged = list.clone();
    paged.query.insert("cursor".to_owned(), "${var.cursor}".to_owned());
    paged.vars.insert("cursor".to_owned(), json!("abc"));
    let planned = Case::new(list.clone(), paged.clone(), &list).plan();
    let put = |map, key: &str, value| Step::Put { api: "listUsers".to_owned(), map, key: key.to_owned(), value };
    assert_eq!(planned.steps, [put("query", "cursor", json!("${var.cursor}")), put("vars", "cursor", json!("abc"))]);
    assert_eq!(planned.lines, ["↑ users#listUsers query cursor none → ${var.cursor}"]);
    let mut has_var = mine(&list);
    has_var.vars.push("cursor".to_owned());
    let case = Case { mine: vec![has_var], ..Case::new(list, paged, &draft("x", Method::Get, "/")) };
    assert_eq!(case.plan().steps, [put("query", "cursor", json!("${var.cursor}"))]);
}

#[test]
fn a_dropped_query_parameter_and_a_new_content_type_follow_the_spec() {
    let mut base = draft("listUsers", Method::Post, "/users");
    base.query.insert("page".to_owned(), "${var.page}".to_owned());
    let mut theirs = draft("listUsers", Method::Post, "/users");
    theirs.content_type = Some("application/xml".to_owned());
    let planned = Case::new(base.clone(), theirs, &base).plan();
    let api = || "listUsers".to_owned();
    let header =
        Step::Put { api: api(), map: "headers", key: "content-type".to_owned(), value: json!("application/xml") };
    assert_eq!(planned.steps, [header, Step::Remove { api: api(), map: "query", key: "page".to_owned() }]);
    assert_eq!(
        planned.lines,
        ["↑ users#listUsers content type none → application/xml", "↑ users#listUsers query page ${var.page} → none"]
    );
}
