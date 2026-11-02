//! `*.test.yaml`.

use vc_diagnostics::Code;

use super::common::{ACTIONS, PROFILES, SKIP, TEXTS, VARS, request};
use super::types::{Scope, Shape, Ty, key};

shape! {
    test_file = Shape::new("test", "A test file, `services/<service>/<area>.test.yaml`", &[
        key("id", Ty::Text).default("path-derived").doc("File ID; must match the path (D139)").example("users.crud"),
        key("name", Ty::Text).doc("Shown in reports"),
        key("tags", TEXTS).doc("Added to every test in the file").example("[smoke]"),
        key("vars", VARS).doc("File-level vars"),
        key("profiles", PROFILES).doc("Per-profile overrides of the file's vars"),
        key("beforeAll", ACTIONS).doc("Run once before the file's tests"),
        key("afterAll", ACTIONS).doc("Run once after the file's tests"),
        key("beforeEach", ACTIONS).doc("Run before every test instance"),
        key("afterEach", ACTIONS).doc("Run after every test instance"),
        key("tests", Ty::List(&Ty::Obj(test))).doc("The tests"),
        key("skip", SKIP).doc("Skip every test in the file"),
    ])
}

shape! {
    test = Shape::new("testEntry", "One test: a call or steps, with checks", &[
        key("id", Ty::Id).required().doc("Third-person verb, such as `rejectsDeclinedCard`").example("getsCreatedUser"),
        key("name", Ty::Text).default("id").doc("Shown in reports"),
        key("tags", TEXTS).doc("Added to the file's tags"),
        key("before", ACTIONS).doc("Run before this test, inside `beforeEach`"),
        key("after", ACTIONS).doc("Run after this test, inside `afterEach`"),
        key("call", Ty::Ref).doc("The api to call").example("users#get"),
        key("steps", ACTIONS).doc("Several actions in order"),
        key("examples", Ty::Value).doc("One test instance per row: a list of maps, `$file` or a template"),
        key("repeat", Ty::Int).default("1").doc("Run the whole test this many times"),
        key("skip", SKIP).doc("Skip with a reason, optionally until a date"),
        key("standalone", Ty::Bool).default("true").doc("`false`: runs only when reused with `test:`"),
    ])
    .mixins(&[request])
    .one_of(&["call", "steps"], Code::V2201)
    .scopes(&[("examples", Scope::EXAMPLES), ("repeat", Scope::LOOP)])
}
