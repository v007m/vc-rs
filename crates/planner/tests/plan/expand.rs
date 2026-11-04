use std::collections::BTreeMap;

use vc_model::Program;
use vc_planner::{PlanError, PlanOptions, Selection};

use crate::common;

fn options(change: impl FnOnce(&mut PlanOptions)) -> PlanOptions {
    let mut options = common::today();
    change(&mut options);
    options
}

const ROWS: &str = "name: Rows
tests:
  - id: listsPages
    call: users#list
    repeat: 2
    examples:
      - {page: 1}
      - {page: 2}
  - id: listsFilePages
    call: users#list
    repeat: 2
    examples: {$file: data/pages.csv}
";

fn with_examples() -> Program {
    common::project_with(&[("services/users/rows.test.yaml", ROWS)])
}

#[test]
fn repeat_runs_each_test_that_many_times_and_the_flag_overrides_it() {
    let program = common::project();
    let once = common::planned(&program, &common::ids(&["users.crud"]));
    assert_eq!(common::slots(&program, &once, "users.crud#listsUsers"), [(None, 0), (None, 1)]);
    assert_eq!(common::slots(&program, &once, "users.crud#readsWarm"), [(None, 0)]);
    let thrice =
        vc_planner::plan(&program, &common::ids(&["users.crud"]), &options(|options| options.repeat = Some(3)))
            .unwrap();
    assert_eq!(common::slots(&program, &thrice, "users.crud#listsUsers"), [(None, 0), (None, 1), (None, 2)]);
    assert_eq!(common::slots(&program, &thrice, "users.crud#readsWarm"), [(None, 0), (None, 1), (None, 2)]);
    let ids: Vec<u32> =
        thrice.files.iter().flat_map(|file| &file.instances).map(|instance| instance.id.index()).collect();
    assert_eq!(ids, (0..9).collect::<Vec<_>>());
}

#[test]
fn files_plan_in_path_order_whatever_order_they_were_read_in() {
    let program = common::project_with(&[(
        "services/accounts/a.test.yaml",
        "name: A\ntests:\n  - id: listsA\n    call: users#list\n",
    )]);
    let plan = common::planned(&program, &Selection::default());
    let files: Vec<&str> = plan.files.iter().map(|file| common::name(&program, file.id.sym())).collect();
    assert_eq!(files, ["accounts.a", "orders.checkout", "users.crud"]);
}

#[test]
fn example_rows_multiply_repeats_with_rows_outermost() {
    let program = with_examples();
    let plan = common::planned(&program, &common::ids(&["users.rows#listsPages"]));
    assert_eq!(
        common::slots(&program, &plan, "users.rows#listsPages"),
        [(Some(0), 0), (Some(0), 1), (Some(1), 0), (Some(1), 1)]
    );
    let third = common::planned(&program, &common::ids(&["users.rows#listsPages:3"]));
    assert_eq!(common::slots(&program, &third, "users.rows#listsPages"), [(Some(1), 0)]);
}

#[test]
fn rows_from_a_file_come_preloaded_in_the_options() {
    let program = with_examples();
    let selection = common::ids(&["users.rows#listsFilePages"]);
    assert_eq!(
        common::try_plan(&program, &selection),
        Err(PlanError::RowsNotLoaded { test: "users.rows#listsFilePages".into() })
    );
    let test = common::test_id(&program, "users.rows#listsFilePages");
    let loaded = options(|options| options.example_rows = BTreeMap::from([(test, 1)]));
    let plan = vc_planner::plan(&program, &selection, &loaded).unwrap();
    assert_eq!(common::slots(&program, &plan, "users.rows#listsFilePages"), [(Some(0), 0), (Some(0), 1)]);
}

#[test]
fn skips_hold_until_their_date_and_a_file_skip_wins() {
    let program = common::project();
    let forever = vc_planner::plan(&program, &Selection::default(), &PlanOptions::default()).unwrap();
    assert_eq!(common::skipped(&program, &forever), ["runsOld", "refundsOrder", "voidsOrder"]);
    let reasons: Vec<&str> = forever.skipped.iter().map(|skipped| skipped.reason.as_str()).collect();
    assert_eq!(reasons, ["retired", "flaky", "gone"]);
    let on_the_day = vc_planner::plan(
        &program,
        &Selection::default(),
        &options(|options| options.today = Some("2000-01-01".into())),
    );
    assert_eq!(common::skipped(&program, &on_the_day.unwrap()), ["runsOld", "refundsOrder", "voidsOrder"]);
    let after = common::planned(&program, &Selection::default());
    assert_eq!(common::skipped(&program, &after), ["runsOld", "refundsOrder"]);
}

#[test]
fn a_test_that_is_not_standalone_is_left_out_entirely() {
    let mut program = common::project();
    let test = common::test_id(&program, "orders.checkout#refundsOrder");
    program.tests[&test].standalone = false;
    let plan = common::planned(&program, &common::ids(&["orders.checkout"]));
    assert_eq!(
        (common::tests(&program, &plan), common::skipped(&program, &plan)),
        (vec!["paysOrder", "voidsOrder"], vec![])
    );
}

#[test]
fn lifecycle_is_planned_only_where_an_instance_runs() {
    let program = common::project();
    let plan = common::planned(&program, &common::ids(&["orders.checkout#paysOrder"]));
    assert_eq!(common::actions(&program, plan.before_run.iter().map(|planned| planned.action)), ["warm", "cold"]);
    assert_eq!(common::actions(&program, plan.after_run.iter().copied()), ["bye"]);
    let [file] = &plan.files[..] else { panic!("one file") };
    assert_eq!(common::actions(&program, file.before_all.iter().map(|planned| planned.action)), ["prime"]);
    let none = common::planned(&program, &common::ids(&["legacy.old"]));
    assert!(none.before_run.is_empty() && none.after_run.is_empty() && none.files.is_empty());
    let everything = common::planned(&program, &Selection::default());
    let files: Vec<usize> = everything.files.iter().map(|file| file.instances.len()).collect();
    assert_eq!(files, [2, 4], "no planned file for the skipped one");
}

#[test]
fn each_instance_needs_the_shared_actions_it_reads() {
    let program = common::project();
    let plan = common::planned(&program, &Selection::default());
    let needs = |full_id: &str| {
        let test = common::test_id(&program, full_id);
        let instance = plan.files.iter().flat_map(|file| &file.instances).find(|instance| instance.test == test);
        common::actions(&program, instance.unwrap().needs.iter())
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
    let program = common::project_with(&[("services/users/hooks.test.yaml", each)]);
    let error = common::try_plan(&program, &common::ids(&["users.hooks"])).unwrap_err();
    assert!(matches!(error, PlanError::NotSupportedYet { kind: "function", .. }), "{error:?}");
    assert!(common::try_plan(&program, &common::ids(&["users.crud"])).is_ok(), "only planned actions are checked");
}
