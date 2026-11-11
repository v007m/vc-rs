use vc_model::{ActionId, ActionKind, Instance, InstanceId, Needs, Plan, PlannedAction, PlannedFile, TestBody};

use super::{bodies, bodies_from};
use crate::testing::Fixture;

/// Puts `action` somewhere in a program whose plan runs test `a` alone, with test `b` beside it.
type Place = fn(&mut Fixture, ActionId, &mut Plan);

/// A plan running one instance of `test`.
fn planned(fixture: &Fixture, test: vc_model::TestId) -> Plan {
    let instance =
        Instance { id: InstanceId::new(0), test, example_row: None, repeat_index: 0, needs: Needs::default() };
    let file = PlannedFile { id: fixture.file, before_all: vec![], instances: vec![instance], after_all: vec![] };
    Plan { files: vec![file], ..Plan::default() }
}

/// Whether `bodies` reaches an action once `place` has put it somewhere.
fn reached(place: Place) -> bool {
    let mut fixture = Fixture::new();
    let a = fixture.test("a", &[]);
    fixture.test("b", &[]);
    let mut plan = planned(&fixture, a);
    let action = fixture.call(None);
    place(&mut fixture, action, &mut plan);
    bodies(&fixture.program, &plan).contains(&action)
}

fn test_b(fixture: &Fixture) -> vc_model::TestId {
    vc_model::TestId::new(fixture.sym("users.crud#b"))
}

#[test]
fn a_plan_reaches_its_own_actions_and_what_they_lead_to_only() {
    let rows: [(&str, Place, bool); 10] = [
        ("an action nothing names", |_, _, _| {}, false),
        ("an unselected test's call", |f, x, _| f.test_mut(test_b(f)).body = TestBody::Call(x), false),
        ("an unselected test's after action", |f, x, _| f.test_mut(test_b(f)).after.push(x), false),
        ("a beforeRun action", |_, x, p| p.before_run.push(PlannedAction { action: x, needs: Needs::default() }), true),
        (
            "a beforeAll action",
            |_, x, p| p.files[0].before_all.push(PlannedAction { action: x, needs: Needs::default() }),
            true,
        ),
        ("an afterAll action", |_, x, p| p.files[0].after_all.push(x), true),
        ("an afterRun action", |_, x, p| p.after_run.push(x), true),
        ("a beforeEach action", |f, x, _| f.file_mut().lifecycle.before_each.push(x), true),
        ("an api cleanup", |f, x, _| f.program.apis[&f.api].cleanup = Some(x), true),
        (
            "a test reused with `test:`",
            |f, x, _| {
                let b = test_b(f);
                f.test_mut(b).body = TestBody::Call(x);
                let reuse = f.action(ActionKind::Test(b), None);
                let a = vc_model::TestId::new(f.sym("users.crud#a"));
                f.test_mut(a).after.push(reuse);
            },
            true,
        ),
    ];
    for (what, place, expected) in rows {
        assert_eq!(reached(place), expected, "{what}");
    }
}

#[test]
fn a_start_reaches_itself_and_its_api_cleanup() {
    let mut fixture = Fixture::new();
    let (call, cleanup, other) = (fixture.call(None), fixture.call(None), fixture.call(None));
    fixture.program.apis[&fixture.api].cleanup = Some(cleanup);
    let reached = bodies_from(&fixture.program, &[call]);
    assert_eq!(reached.into_iter().collect::<Vec<_>>(), [call, cleanup]);
    assert!(!bodies_from(&fixture.program, &[]).contains(&other));
}
