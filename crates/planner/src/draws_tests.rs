use vc_model::{Instance, InstanceId, Namespace, Needs, Part, PipeCall, Plan, PlannedFile, Template, TestBody, TestId};

use super::generates;
use crate::testing::{Fixture, read, span};

/// A plan running one instance of `test`.
fn planned(fixture: &Fixture, test: TestId) -> Plan {
    let instance =
        Instance { id: InstanceId::new(0), test, example_row: None, repeat_index: 0, needs: Needs::default() };
    let file = PlannedFile { id: fixture.file, before_all: vec![], instances: vec![instance], after_all: vec![] };
    Plan { files: vec![file], ..Plan::default() }
}

/// `${<ns>.x.token}`, through `pipe` when given.
fn template(ns: Namespace, pipe: Option<&str>) -> Template {
    let Some(Part::Ref(mut reference)) = read("x").parts.pop() else { panic!("`read` ends in a reference") };
    reference.ns = ns;
    let call = |name: &str| PipeCall { name: name.to_owned(), args: Vec::new(), name_span: span(), span: span() };
    reference.pipes = pipe.map(call).into_iter().collect();
    Template { parts: vec![Part::Ref(reference)] }
}

/// Whether a plan running test `a` generates data once a header of `on`'s call is `header`.
fn generated(header: Template, on: &str) -> bool {
    let mut fixture = Fixture::new();
    let (a, b) = (fixture.test("a", &[]), fixture.test("b", &[]));
    let test = if on == "a" { a } else { b };
    let TestBody::Call(call) = fixture.program.tests[&test].body.clone() else { panic!("a test of one call") };
    fixture.action_mut(call).headers.insert("x-data".to_owned(), header);
    let plan = planned(&fixture, a);
    generates(&fixture.program, &plan, |pipe| ["random", "shuffle"].contains(&pipe))
}

#[test]
fn a_plan_generates_data_through_fake_or_a_drawing_pipe_it_runs() {
    assert!(generated(template(Namespace::Fake, None), "a"), "a `${{fake.…}}` the plan runs");
    assert!(generated(template(Namespace::Var, Some("shuffle")), "a"), "a pipe that draws");
    assert!(!generated(template(Namespace::Var, Some("upper")), "a"), "a pipe that does not draw");
    assert!(!generated(template(Namespace::Var, None), "a"), "a plain var");
    assert!(!generated(template(Namespace::Fake, None), "b"), "a `${{fake.…}}` in a test the plan does not run");
}

#[test]
fn the_run_scope_counts_too() {
    let mut fixture = Fixture::new();
    let a = fixture.test("a", &[]);
    fixture.program.config.defaults.headers.insert("x-who".to_owned(), template(Namespace::Fake, None));
    assert!(generates(&fixture.program, &planned(&fixture, a), |_| false), "a config default header");
}
