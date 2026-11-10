//! `*.test.yaml`.

use vc_diagnostics::Code;

use super::common;
use super::common::{ACTIONS, PROFILES, SKIP, TEXTS, VARS};
use super::types;
use super::types::{Scope, Shape, Ty};

shape! {
    test_file = Shape::new("test", "A test file, `services/<service>/<area>.test.yaml`", &[
        types::key("id", Ty::Text).default("path-derived").doc("File ID; must match the path (D139)").example("users.crud"),
        types::key("name", Ty::Text).doc("Shown in reports"),
        types::key("tags", TEXTS).doc("Added to every test in the file").example("[smoke]"),
        types::key("vars", VARS).doc("File-level vars"),
        types::key("profiles", PROFILES).doc("Per-profile overrides of the file's vars"),
        types::key("beforeAll", ACTIONS).doc("Run once before the file's tests"),
        types::key("afterAll", ACTIONS).doc("Run once after the file's tests"),
        types::key("beforeEach", ACTIONS).doc("Run before every test instance"),
        types::key("afterEach", ACTIONS).doc("Run after every test instance"),
        types::key("tests", Ty::List(&Ty::Obj(test))).doc("The tests"),
        types::key("skip", SKIP).doc("Skip every test in the file"),
    ])
}

shape! {
    test = Shape::new("testEntry", "One test: a call or steps, with checks", &[
        types::key("id", Ty::Id).required().doc("Third-person verb, such as `rejectsDeclinedCard`").example("getsCreatedUser"),
        types::key("name", Ty::Text).default("id").doc("Shown in reports"),
        types::key("tags", TEXTS).doc("Added to the file's tags"),
        types::key("before", ACTIONS).doc("Run before this test, inside `beforeEach`"),
        types::key("after", ACTIONS).doc("Run after this test, inside `afterEach`"),
        types::key("call", Ty::Ref).doc("The api to call").example("users#get"),
        types::key("steps", ACTIONS).doc("Several actions in order"),
        types::key("examples", Ty::Value).doc("One test instance per row: a list of maps, `$file` or a template"),
        types::key("repeat", Ty::Int).default("1").doc("Run the whole test this many times"),
        types::key("skip", SKIP).doc("Skip with a reason, optionally until a date"),
        types::key("standalone", Ty::Bool).default("true").doc("`false`: runs only when reused with `test:`"),
    ])
    .mixins(&[common::request])
    .one_of(&["call", "steps"], Code::V2201)
    .scopes(&[("examples", Scope::EXAMPLES), ("repeat", Scope::LOOP)])
}
