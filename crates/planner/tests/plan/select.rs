use std::collections::BTreeSet;

use vc_planner::{PlanError, Selection};

use crate::common::{ids, planned, project, skipped, slots, tests, try_plan};

const RUNNABLE: &[&str] = &["paysOrder", "voidsOrder", "listsUsers", "createsUser", "readsWarm"];

fn tags(exprs: &[&str]) -> Selection {
    Selection { tags: exprs.iter().map(|expr| (*expr).to_owned()).collect(), ..Selection::default() }
}

fn paths(paths: &[&str]) -> Selection {
    Selection { paths: paths.iter().map(|path| (*path).to_owned()).collect(), ..Selection::default() }
}

fn changed(files: &[&str]) -> Selection {
    Selection {
        changed: Some(files.iter().map(|file| (*file).to_owned()).collect::<BTreeSet<_>>()),
        ..Selection::default()
    }
}

/// A selector case: its name, the selection, the tests it plans and the tests it reports skipped.
type Row = (&'static str, Selection, &'static [&'static str], &'static [&'static str]);

#[test]
fn each_selector_kind_picks_its_tests() {
    let program = project();
    let table: Vec<Row> = vec![
        ("everything", Selection::default(), RUNNABLE, &["runsOld", "refundsOrder"]),
        ("a test ID", ids(&["users.crud#readsWarm"]), &["readsWarm"], &[]),
        ("two test IDs", ids(&["orders.checkout#paysOrder", "users.crud#readsWarm"]), &["paysOrder", "readsWarm"], &[]),
        ("a file ID", ids(&["users.crud"]), &["listsUsers", "createsUser", "readsWarm"], &[]),
        ("a skipped file", ids(&["legacy.old"]), &[], &["runsOld"]),
        ("a glob over IDs", ids(&["orders.*"]), &["paysOrder", "voidsOrder"], &["refundsOrder"]),
        ("a glob over tests", ids(&["*#creates*"]), &["createsUser"], &[]),
        ("a path given as an ID", ids(&["services/orders"]), &["paysOrder", "voidsOrder"], &["refundsOrder"]),
        ("a folder", paths(&["services/orders/"]), &["paysOrder", "voidsOrder"], &["refundsOrder"]),
        ("a file", paths(&["./services/users/crud.test.yaml"]), &["listsUsers", "createsUser", "readsWarm"], &[]),
        ("the root", paths(&["."]), RUNNABLE, &["runsOld", "refundsOrder"]),
        ("a tag", tags(&["slow"]), &["createsUser"], &["refundsOrder"]),
        ("a tag expression", tags(&["checkout & !slow"]), &["paysOrder", "voidsOrder"], &[]),
        ("two tag flags", tags(&["smoke", "!slow"]), &["listsUsers", "readsWarm"], &[]),
        (
            "grep",
            Selection { grep: Some("ORDER".into()), ..Selection::default() },
            &["paysOrder", "voidsOrder"],
            &["refundsOrder"],
        ),
        ("a changed api file", changed(&["users"]), &["listsUsers", "createsUser", "readsWarm"], &["runsOld"]),
        ("a changed test file", changed(&["orders.checkout"]), &["paysOrder", "voidsOrder"], &["refundsOrder"]),
        ("an ID and a filter", Selection { tags: vec!["slow".into()], ..ids(&["users.crud"]) }, &["createsUser"], &[]),
    ];
    for (case, selection, expected, expected_skipped) in table {
        let plan = planned(&program, &selection);
        assert_eq!(tests(&program, &plan), expected, "{case}");
        assert_eq!(skipped(&program, &plan), expected_skipped, "{case}");
    }
}

#[test]
fn include_skipped_plans_skipped_tests_and_files() {
    let program = project();
    let plan = planned(&program, &Selection { include_skipped: true, ..Selection::default() });
    let all = ["runsOld", "paysOrder", "refundsOrder", "voidsOrder", "listsUsers", "createsUser", "readsWarm"];
    assert_eq!(tests(&program, &plan), all);
    assert_eq!(plan.skipped, []);
}

#[test]
fn an_instance_selector_picks_one_instance() {
    let program = project();
    let plan = planned(&program, &ids(&["users.crud#listsUsers:2"]));
    assert_eq!(slots(&program, &plan, "users.crud#listsUsers"), [(None, 1)]);
    let both = planned(&program, &ids(&["users.crud#listsUsers:2", "users.crud#listsUsers:1"]));
    assert_eq!(slots(&program, &both, "users.crud#listsUsers"), [(None, 0), (None, 1)]);
    let whole = planned(&program, &ids(&["users.crud#listsUsers:2", "users.crud"]));
    assert_eq!(slots(&program, &whole, "users.crud#listsUsers"), [(None, 0), (None, 1)]);
}

#[test]
fn a_selection_that_matches_nothing_plans_nothing() {
    let program = project();
    let plan = planned(&program, &tags(&["nope"]));
    assert_eq!((plan.instance_count(), plan.before_run.len(), plan.after_run.len()), (0, 0, 0));
}

#[test]
fn bad_selectors_are_errors_with_help() {
    let program = project();
    let unknown = try_plan(&program, &ids(&["users.crud#listUsers"])).unwrap_err();
    let PlanError::UnknownSelector { suggestions, .. } = &unknown else { panic!("{unknown:?}") };
    assert_eq!(suggestions.first().map(String::as_str), Some("users.crud#listsUsers"));
    assert_eq!(unknown.help().as_deref(), Some("did you mean `users.crud#listsUsers`?"));
    let file = try_plan(&program, &ids(&["users.crd"])).unwrap_err();
    assert!(
        matches!(file, PlanError::UnknownSelector { ref suggestions, .. } if suggestions.contains(&"users.crud".to_owned()))
    );
}

#[test]
fn bad_ordinals_globs_tag_expressions_and_paths_are_errors() {
    let program = project();
    let errors = [
        (
            ids(&["users.crud#listsUsers:3"]),
            PlanError::NoSuchInstance { selector: "users.crud#listsUsers:3".into(), count: 2 },
        ),
        (
            ids(&["users.crud#listsUsers:0"]),
            PlanError::NoSuchInstance { selector: "users.crud#listsUsers:0".into(), count: 2 },
        ),
    ];
    for (selection, expected) in errors {
        assert_eq!(try_plan(&program, &selection), Err(expected));
    }
    assert!(matches!(try_plan(&program, &ids(&["users.[a"])), Err(PlanError::BadGlob { .. })));
    assert!(matches!(try_plan(&program, &tags(&["smoke slow"])), Err(PlanError::BadTagExpr { .. })));
    assert!(matches!(try_plan(&program, &paths(&["services/nope"])), Err(PlanError::UnknownSelector { .. })));
}
