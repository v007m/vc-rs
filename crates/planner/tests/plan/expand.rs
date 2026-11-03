use std::collections::BTreeMap;

use vc_model::{Examples, Program, Template, TemplateValue};
use vc_planner::{PlanError, PlanOptions, Selection, plan};

use crate::common::{actions, ids, planned, project, project_with, skipped, slots, test_id, tests, today, try_plan};

fn options(change: impl FnOnce(&mut PlanOptions)) -> PlanOptions {
    let mut options = today();
    change(&mut options);
    options
}

fn with_examples(examples: Examples) -> Program {
    let mut program = project();
    let test = test_id(&program, "users.crud#listsUsers");
    // The compiler drops inline `examples` (with m1-lint); remove when that lands.
    program.tests[&test].examples = Some(examples);
    program
}

#[test]
fn repeat_runs_each_test_that_many_times_and_the_flag_overrides_it() {
    let program = project();
    let once = planned(&program, &ids(&["users.crud"]));
    assert_eq!(slots(&program, &once, "users.crud#listsUsers"), [(None, 0), (None, 1)]);
    assert_eq!(slots(&program, &once, "users.crud#readsWarm"), [(None, 0)]);
    let thrice = plan(&program, &ids(&["users.crud"]), &options(|options| options.repeat = Some(3))).unwrap();
    assert_eq!(slots(&program, &thrice, "users.crud#listsUsers"), [(None, 0), (None, 1), (None, 2)]);
    assert_eq!(slots(&program, &thrice, "users.crud#readsWarm"), [(None, 0), (None, 1), (None, 2)]);
    let ids: Vec<u32> =
        thrice.files.iter().flat_map(|file| &file.instances).map(|instance| instance.id.index()).collect();
    assert_eq!(ids, (0..9).collect::<Vec<_>>());
}

#[test]
fn example_rows_multiply_repeats_with_rows_outermost() {
    let program = with_examples(Examples::Rows(vec![TemplateValue::Null, TemplateValue::Null]));
    let plan = planned(&program, &ids(&["users.crud#listsUsers"]));
    assert_eq!(
        slots(&program, &plan, "users.crud#listsUsers"),
        [(Some(0), 0), (Some(0), 1), (Some(1), 0), (Some(1), 1)]
    );
    let third = planned(&program, &ids(&["users.crud#listsUsers:3"]));
    assert_eq!(slots(&program, &third, "users.crud#listsUsers"), [(Some(1), 0)]);
}

#[test]
fn rows_from_a_file_come_preloaded_in_the_options() {
    let program = with_examples(Examples::File(Template::default()));
    let selection = ids(&["users.crud#listsUsers"]);
    assert_eq!(try_plan(&program, &selection), Err(PlanError::RowsNotLoaded { test: "users.crud#listsUsers".into() }));
    let test = test_id(&program, "users.crud#listsUsers");
    let loaded = options(|options| options.example_rows = BTreeMap::from([(test, 1)]));
    let plan = plan(&program, &selection, &loaded).unwrap();
    assert_eq!(slots(&program, &plan, "users.crud#listsUsers"), [(Some(0), 0), (Some(0), 1)]);
}

#[test]
fn skips_hold_until_their_date_and_a_file_skip_wins() {
    let program = project();
    let forever = plan(&program, &Selection::default(), &PlanOptions::default()).unwrap();
    assert_eq!(skipped(&program, &forever), ["runsOld", "refundsOrder", "voidsOrder"]);
    let reasons: Vec<&str> = forever.skipped.iter().map(|skipped| skipped.reason.as_str()).collect();
    assert_eq!(reasons, ["retired", "flaky", "gone"]);
    let on_the_day =
        plan(&program, &Selection::default(), &options(|options| options.today = Some("2000-01-01".into())));
    assert_eq!(skipped(&program, &on_the_day.unwrap()), ["runsOld", "refundsOrder", "voidsOrder"]);
    let after = planned(&program, &Selection::default());
    assert_eq!(skipped(&program, &after), ["runsOld", "refundsOrder"]);
}

#[test]
fn a_test_that_is_not_standalone_is_left_out_entirely() {
    let mut program = project();
    let test = test_id(&program, "orders.checkout#refundsOrder");
    program.tests[&test].standalone = false;
    let plan = planned(&program, &ids(&["orders.checkout"]));
    assert_eq!((tests(&program, &plan), skipped(&program, &plan)), (vec!["paysOrder", "voidsOrder"], vec![]));
}

#[test]
fn lifecycle_is_planned_only_where_an_instance_runs() {
    let program = project();
    let plan = planned(&program, &ids(&["orders.checkout#paysOrder"]));
    assert_eq!(actions(&program, plan.before_run.iter().map(|planned| planned.action)), ["warm", "cold"]);
    assert_eq!(actions(&program, plan.after_run.iter().copied()), ["bye"]);
    let [file] = &plan.files[..] else { panic!("one file") };
    assert_eq!(actions(&program, file.before_all.iter().map(|planned| planned.action)), ["prime"]);
    let none = planned(&program, &ids(&["legacy.old"]));
    assert!(none.before_run.is_empty() && none.after_run.is_empty() && none.files.is_empty());
    let everything = planned(&program, &Selection::default());
    let files: Vec<usize> = everything.files.iter().map(|file| file.instances.len()).collect();
    assert_eq!(files, [2, 4], "no planned file for the skipped one");
}

#[test]
fn each_instance_needs_the_shared_actions_it_reads() {
    let program = project();
    let plan = planned(&program, &Selection::default());
    let needs = |full_id: &str| {
        let test = test_id(&program, full_id);
        let instance = plan.files.iter().flat_map(|file| &file.instances).find(|instance| instance.test == test);
        actions(&program, instance.unwrap().needs.iter())
    };
    assert_eq!(needs("users.crud#listsUsers"), Vec::<&str>::new());
    assert_eq!(needs("users.crud#createsUser"), ["seed"]);
    assert_eq!(needs("users.crud#readsWarm"), ["warm"]);
    assert_eq!(needs("orders.checkout#paysOrder"), ["cold", "prime"]);
}

#[test]
fn a_function_or_command_cannot_be_planned_yet() {
    let each =
        "name: Hooks\nbeforeEach:\n  - function: hooks/a.ts#reset\ntests:\n  - id: listsAgain\n    call: users#list\n";
    let program = project_with(&[("services/users/hooks.test.yaml", each)]);
    let error = try_plan(&program, &ids(&["users.hooks"])).unwrap_err();
    assert!(matches!(error, PlanError::NotSupportedYet { kind: "function", .. }), "{error:?}");
    assert!(try_plan(&program, &ids(&["users.crud"])).is_ok(), "only planned actions are checked");
}
