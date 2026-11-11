//! Shapes shared by several file kinds: checks, actions, retries, hooks.

use vc_diagnostics::Code;

use super::types;
use super::types::{Scope, Shape, Ty, Unit};

/// The HTTP methods.
pub const METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

/// `vars`: any values, templates allowed.
pub const VARS: Ty = Ty::MapOf(&Ty::Value);
/// `headers` and `query`: templates by name.
pub const TEMPLATES: Ty = Ty::MapOf(&Ty::Template);
/// A list of plain text.
pub const TEXTS: Ty = Ty::List(&Ty::Text);
/// `auth`: an ID, `none`, or an ID with vars.
pub const AUTH: Ty = Ty::Either(&[Ty::Ref, Ty::Obj(auth_use)]);
/// `skip`: a reason, or a reason and a date.
pub const SKIP: Ty = Ty::Either(&[Ty::Text, Ty::Obj(skip)]);
/// A list of actions.
pub const ACTIONS: Ty = Ty::List(&Ty::Obj(action));
/// `beforeRequest` and `afterResponse`: a list of functions.
pub const HOOKS: Ty = Ty::List(&Ty::Obj(function_ref));
/// Per-profile overrides of a file's vars.
pub const PROFILES: Ty = Ty::List(&Ty::Obj(profile_vars));

shape! {
    /// `expect`: the checks (D123).
    expect = Shape::new("expect", "Checks on a response; each value is a literal (equals) or a matcher", &[
        types::key("status", Ty::Status).doc("Code, list of codes, class (`2xx`) or matcher").example("[200, 201]"),
        types::key("headers", Ty::MapOf(&Ty::Matcher(Unit::Plain)))
            .doc("Matcher per header; names are case-insensitive").example("{content-type: {contains: json}}"),
        types::key("body", Ty::Obj(body_expect)).doc("Checks on the body: schema, match, equals"),
        types::key("size", Ty::Matcher(Unit::Size)).doc("Matcher on the body size").example("{lt: 100kb}"),
        types::key("timing", Ty::Obj(timing)).doc("Matchers on the request phases").example("{total: {lt: 500ms}}"),
        types::key("cel", Ty::MapOf(&Ty::Cel)).doc("CEL expressions that must be true, by label")
            .example("{has items: size(response.body.items) > 0}"),
        types::key("custom", Ty::List(&Ty::Obj(function_ref))).doc("Functions that check the response"),
        types::key("replace", Ty::Bool).default("false").doc("Drop every inherited check and keep only these"),
    ])
}

shape! {
    body_expect = Shape::new("bodyExpect", "Checks on the response body", &[
        types::key("schema", Ty::Schema).doc("JSON Schema the body must match, usually `$file`")
            .example("{$file: schemas/user.json}"),
        types::key("match", Ty::MapOf(&Ty::Matcher(Unit::Plain))).doc("Matcher at a path in the body")
            .example("{items[*].price: {every: {gt: 0}}}"),
        types::key("equals", Ty::Value).doc("The whole body, compared deeply"),
    ])
}

shape! {
    timing = Shape::new("timing", "Matchers on the request phases", &[
        types::key("dns", Ty::Matcher(Unit::Duration)).doc("Name lookup"),
        types::key("connect", Ty::Matcher(Unit::Duration)).doc("TCP connect"),
        types::key("tls", Ty::Matcher(Unit::Duration)).doc("TLS handshake"),
        types::key("firstByte", Ty::Matcher(Unit::Duration)).doc("Until the first response byte"),
        types::key("download", Ty::Matcher(Unit::Duration)).doc("Reading the body"),
        types::key("total", Ty::Matcher(Unit::Duration)).doc("The whole request").example("{lt: 500ms}"),
    ])
}

shape! {
    function_ref = Shape::new("function", "A function in a code file", &[
        types::key("function", Ty::Ref).required().doc("`path#export`").example("hooks/checks.ts#isSorted"),
    ])
}

shape! {
    auth_use = Shape::new("authUse", "An auth with vars for it", &[
        types::key("id", Ty::Ref).required().doc("The auth's ID").example("shopOauth"),
        types::key("vars", VARS).doc("Vars passed to the auth"),
    ])
}

shape! {
    skip = Shape::new("skip", "Why and until when to skip", &[
        types::key("reason", Ty::Text).required().doc("Shown in reports").example("flaky upstream, see issue 123"),
        types::key("until", Ty::Date).doc("Last day of the skip; after it the test runs again and `vc check` warns (V3104)").example("2026-12-01"),
    ])
}

shape! {
    retry = Shape::new("retry", "Re-run the whole test instance after a failure (D126)", &[
        types::key("attempts", Ty::Int).required().min(0).doc("Further attempts after the first").example("2"),
        types::key("delay", Ty::Duration).default("0s").doc("Pause between attempts").example("1s"),
    ])
}

shape! {
    poll = Shape::new("poll", "Re-send until `until` holds, then check `expect`", &[
        types::key("until", Ty::Obj(expect)).required().scope(Scope::CALL).doc("Checks that end the polling"),
        types::key("every", Ty::Duration).required().doc("Pause between calls").example("2s"),
        types::key("timeout", Ty::Duration).required().doc("Give up after this long").example("1m"),
    ])
}

shape! {
    cache = Shape::new("cache", "Keep the action's saves across runs", &[
        types::key("for", Ty::Duration).required().doc("How long the result is reused").example("12h"),
        types::key("check", Ty::Obj(action)).doc("Quick call that confirms the cached data still exists"),
    ])
}

shape! {
    cleanup = Shape::new("cleanup", "How to undo a successful call", &[
        types::key("call", Ty::Ref).required().doc("The api that undoes it").example("users#delete"),
        types::key("vars", VARS).doc("Vars for that call, templated over the response"),
    ])
}

shape! {
    profile_vars = Shape::new("profileVars", "A profile's overrides of this file's vars", &[
        types::key("id", Ty::Ref).required().doc("A profile defined in the config").example("staging"),
        types::key("vars", VARS).doc("Values that replace the file's vars under that profile"),
    ])
}

shape! {
    /// Keys a test and a `call` or `test` action share.
    request = Shape::new("request", "Values and checks for a call", &[
        types::key("vars", VARS).doc("Values for this call"),
        types::key("auth", AUTH).doc("Auth ID, `none`, or `id` + `vars`"),
        types::key("headers", TEMPLATES).doc("Merged over the api's headers"),
        types::key("query", TEMPLATES).doc("Merged over the api's query"),
        types::key("body", Ty::Value).doc("Replaces the api's body"),
        types::key("expect", Ty::Obj(expect)).scope(Scope::CALL).doc("Merged with the api's checks"),
        types::key("save", VARS).scope(Scope::CALL).doc("Values named for later steps, templated over `response`"),
        types::key("poll", Ty::Obj(poll)).doc("`until`, `every`, `timeout`"),
        types::key("retry", Ty::Obj(retry)).doc("`attempts`, `delay`"),
        types::key("timeout", Ty::Duration).doc("Time limit").example("30s"),
    ])
}

shape! {
    /// One entry of a lifecycle block or of `steps`.
    action = Shape::new("action", "One entry of a lifecycle block or of a test's steps", &[
        types::key("id", Ty::Id).doc("Names its saved values in shared blocks").example("catalog"),
        types::key("call", Ty::Ref).doc("The api to call").example("users#create"),
        types::key("test", Ty::Ref).doc("A test whose calls to run").example("shared.flows#checksOut"),
        types::key("function", Ty::Ref).doc("`path#export` to run").example("hooks/seed.ts#seedDb"),
        types::key("command", Ty::Template).doc("A shell command to run").example("docker compose up -d --wait"),
        types::key("keep", Ty::Bool).default("false").doc("Skip the api's cleanup"),
        types::key("cache", Ty::Obj(cache)).doc("Reuse the saves across runs"),
        types::key("repeat", Ty::Int).min(1).doc("Run the action this many times").example("10"),
        types::key("forEach", Ty::Value).doc("Run once per element of a list").example("${dataset.capitals}"),
        types::key("parallel", Ty::Int).default("1").min(1).doc("Iterations at a time, with `repeat` or `forEach`").example("5"),
        types::key("provides", TEXTS).doc("Names a `function` or `test` action saves (D128)"),
    ])
    .mixins(&[request])
    .one_of(&["call", "test", "function", "command"], Code::V2201)
    .scopes(&[("forEach", Scope::FOR_EACH), ("repeat", Scope::LOOP)])
}

shape! {
    http = Shape::new("http", "Transport settings", &[
        types::key("retry", Ty::Obj(http_retry)).default("attempts: 0").doc("Retry one request on a transport failure (D143)"),
    ])
}

shape! {
    http_retry = Shape::new("httpRetry", "Transport retry of one request (D143)", &[
        types::key("attempts", Ty::Int).default("0").min(0).doc("Further attempts; 0 is off").example("2"),
        types::key("on", Ty::List(&Ty::Either(&[Ty::Int, Ty::Enum(&["connect", "timeout"])])))
            .default("[connect, timeout, 502, 503, 504]").doc("Failures and statuses that are retried"),
        types::key("methods", Ty::List(&Ty::Enum(METHODS))).default("[GET, HEAD, OPTIONS, PUT, DELETE]")
            .doc("Methods that are retried"),
        types::key("backoff", Ty::Obj(backoff)).doc("Pause between attempts"),
    ])
}

shape! {
    backoff = Shape::new("backoff", "Exponential backoff", &[
        types::key("base", Ty::Duration).default("100ms").doc("First pause"),
        types::key("max", Ty::Duration).default("2s").doc("Longest pause"),
        types::key("jitter", Ty::Bool).default("true").doc("Randomise each pause"),
    ])
}
