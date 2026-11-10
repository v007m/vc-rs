//! What the web UI shows of a listed definition beyond `vc list`: the apis a test calls, its
//! step count, and an api's declared vars.

use vc_model::{ActionId, ActionKind, Api, Names, Program, Test, TestBody};

use crate::shown::Shown;

/// An api a test calls.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ApiCall {
    /// Its full ID: `users#getUser`.
    pub api: String,
    /// `GET`, `POST` and so on.
    pub method: String,
    /// Its path as written: `/users/${var.userId}`.
    pub path: String,
}

/// The apis `test`'s call or steps call, in order; steps that call no api are left out.
pub fn calls(program: &Program, test: &Test) -> Vec<ApiCall> {
    body(test).iter().filter_map(|action| called(program, *action)).collect()
}

/// `test`'s step count; 0 for a test with one `call`.
pub fn steps(test: &Test) -> u32 {
    match &test.body {
        TestBody::Call(_) => 0,
        TestBody::Steps(steps) => u32::try_from(steps.len()).unwrap_or(u32::MAX),
    }
}

/// The names of the vars `api` declares, in the order written.
pub fn vars(program: &Program, api: &Api) -> Vec<String> {
    api.vars.values.keys().map(|name| program.str(name.sym()).unwrap_or_default().to_owned()).collect()
}

/// The api `action` calls, with its method and path template, when it calls one: what the web
/// UI's run view shows of an action.
#[must_use]
pub fn action_call(names: &Names, action: ActionId) -> Option<ApiCall> {
    called(names.program(), action)
}

fn body(test: &Test) -> &[ActionId] {
    match &test.body {
        TestBody::Call(action) => std::slice::from_ref(action),
        TestBody::Steps(steps) => steps,
    }
}

fn called(program: &Program, action: ActionId) -> Option<ApiCall> {
    let found = program.actions.get(usize::try_from(action.index()).ok()?)?;
    let ActionKind::Call(id) = &found.kind else { return None };
    let api = program.apis.get(id)?;
    let path = Shown { program, vars: &[] }.template(&api.path);
    let method = api.method.name().to_owned();
    Some(ApiCall { api: program.str(id.sym()).unwrap_or_default().to_owned(), method, path })
}
