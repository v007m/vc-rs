//! Stub compiler. The real one will parse test files; this builds a fixed demo
//! program so the engine can be exercised end to end.
use ir::{
    Assertion, Extract, FieldName, FileId, FileName, HookRef, IrError, Method, Program, ProgramBuilder, RequestSpec,
    Role, Scope, SetupRef, StatusCode, Template, TestDraft, TestName, VarName,
};
use serde_json::json;

fn call(method: Method, path: &str) -> RequestSpec {
    RequestSpec { method, path: Template::new(path), headers: vec![], body: None }
}

fn test(name: &str, file: FileId, request: RequestSpec, status: u16) -> TestDraft {
    TestDraft {
        name: TestName::new(name),
        file,
        role: Role::Test,
        request,
        setup: None,
        extract: vec![],
        assertions: vec![Assertion::Status(StatusCode::new(status))],
        before_request: None,
    }
}

/// `sessions.vib`: one slow setup-only "create session" shared (file scope)
/// by `shared` tests that read the session back.
fn sessions_file(b: &mut ProgramBuilder, shared: usize) -> Result<(), IrError> {
    let file = b.add_file(FileName::new("sessions.vib"))?;
    let mut create = test("create session", file, call(Method::Post, "/slow/sessions"), 201);
    create.role = Role::SetupOnly;
    create.request.body = Some(json!({ "name": "shared" }));
    create.extract = vec![Extract { var: VarName::new("session_id"), field: FieldName::new("id") }];
    let setup = SetupRef { test: b.add_test(create)?, scope: Scope::File };
    for i in 0..shared {
        let mut read = test(&format!("read session {i}"), file, call(Method::Get, "/sessions/{{session_id}}"), 200);
        read.setup = Some(setup);
        read.assertions
            .push(Assertion::BodyField { field: FieldName::new("id"), expected: Template::new("{{session_id}}") });
        b.add_test(read)?;
    }
    Ok(())
}

/// `items.vib`: independent tests, one of them signed by a TypeScript hook.
fn items_file(b: &mut ProgramBuilder) -> Result<(), IrError> {
    let file = b.add_file(FileName::new("items.vib"))?;
    let mut create = test("create item", file, call(Method::Post, "/items"), 201);
    create.request.body = Some(json!({ "name": "widget" }));
    let mut upload = test("signed upload", file, call(Method::Post, "/uploads"), 201);
    upload.before_request = Some(HookRef { module: FileName::new("sign.ts"), export: VarName::new("addSignature") });
    let missing = test("missing item is 404", file, call(Method::Get, "/items/missing"), 404);
    for t in [create, upload, missing] {
        b.add_test(t)?;
    }
    Ok(())
}

/// # Errors
/// Only if the hard-coded program is internally inconsistent.
pub fn demo_program(shared_setup_tests: usize) -> Result<Program, IrError> {
    let mut b = ProgramBuilder::default();
    sessions_file(&mut b, shared_setup_tests)?;
    items_file(&mut b)?;
    b.build()
}

/// Hook modules referenced by [`demo_program`].
#[must_use]
pub fn demo_hook_modules() -> Vec<FileName> {
    vec![FileName::new("sign.ts")]
}
