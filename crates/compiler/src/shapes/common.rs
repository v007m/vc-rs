//! Shapes shared by several file kinds: checks, actions, retries, hooks.

use vc_diagnostics::Code;

use super::types::{Scope, Shape, Ty, Unit, key};

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
        key("status", Ty::Status).doc("Code, list of codes, class (`2xx`) or matcher").example("[200, 201]"),
        key("headers", Ty::MapOf(&Ty::Matcher(Unit::Plain)))
            .doc("Matcher per header; names are case-insensitive").example("{content-type: {contains: json}}"),
        key("body", Ty::Obj(body_expect)).doc("Checks on the body: schema, match, equals"),
        key("size", Ty::Matcher(Unit::Size)).doc("Matcher on the body size").example("{lt: 100kb}"),
        key("timing", Ty::Obj(timing)).doc("Matchers on the request phases").example("{total: {lt: 500ms}}"),
        key("cel", Ty::MapOf(&Ty::Cel)).doc("CEL expressions that must be true, by label")
            .example("{has items: size(response.body.items) > 0}"),
        key("custom", Ty::List(&Ty::Obj(function_ref))).doc("Functions that check the response"),
        key("replace", Ty::Bool).default("false").doc("Drop every inherited check and keep only these"),
    ])
}

shape! {
    body_expect = Shape::new("bodyExpect", "Checks on the response body", &[
        key("schema", Ty::Schema).doc("JSON Schema the body must match, usually `$file`")
            .example("{$file: schemas/user.json}"),
        key("match", Ty::MapOf(&Ty::Matcher(Unit::Plain))).doc("Matcher at a path in the body")
            .example("{items[*].price: {every: {gt: 0}}}"),
        key("equals", Ty::Value).doc("The whole body, compared deeply"),
    ])
}

shape! {
    timing = Shape::new("timing", "Matchers on the request phases", &[
        key("dns", Ty::Matcher(Unit::Duration)).doc("Name lookup"),
        key("connect", Ty::Matcher(Unit::Duration)).doc("TCP connect"),
        key("tls", Ty::Matcher(Unit::Duration)).doc("TLS handshake"),
        key("firstByte", Ty::Matcher(Unit::Duration)).doc("Until the first response byte"),
        key("download", Ty::Matcher(Unit::Duration)).doc("Reading the body"),
        key("total", Ty::Matcher(Unit::Duration)).doc("The whole request").example("{lt: 500ms}"),
    ])
}

shape! {
    function_ref = Shape::new("function", "A function in a code file", &[
        key("function", Ty::Ref).required().doc("`path#export`").example("hooks/checks.ts#isSorted"),
    ])
}

shape! {
    auth_use = Shape::new("authUse", "An auth with vars for it", &[
        key("id", Ty::Ref).required().doc("The auth's ID").example("shopOauth"),
        key("vars", VARS).doc("Vars passed to the auth"),
    ])
}

shape! {
    skip = Shape::new("skip", "Why and until when to skip", &[
        key("reason", Ty::Text).required().doc("Shown in reports").example("flaky upstream, see #123"),
        key("until", Ty::Text).doc("Date after which the skip expires").example("2026-12-01"),
    ])
}

shape! {
    retry = Shape::new("retry", "Re-run the whole test instance after a failure (D126)", &[
        key("attempts", Ty::Int).required().doc("Further attempts after the first").example("2"),
        key("delay", Ty::Duration).default("0s").doc("Pause between attempts").example("1s"),
    ])
}

shape! {
    poll = Shape::new("poll", "Re-send until `until` holds, then check `expect`", &[
        key("until", Ty::Obj(expect)).required().scope(Scope::CALL).doc("Checks that end the polling"),
        key("every", Ty::Duration).required().doc("Pause between calls").example("2s"),
        key("timeout", Ty::Duration).required().doc("Give up after this long").example("1m"),
    ])
}

shape! {
    cache = Shape::new("cache", "Keep the action's saves across runs", &[
        key("for", Ty::Duration).required().doc("How long the result is reused").example("12h"),
        key("check", Ty::Obj(action)).doc("Quick call that confirms the cached data still exists"),
    ])
}

shape! {
    cleanup = Shape::new("cleanup", "How to undo a successful call", &[
        key("call", Ty::Ref).required().doc("The api that undoes it").example("users#delete"),
        key("vars", VARS).doc("Vars for that call, templated over the response"),
    ])
}

shape! {
    profile_vars = Shape::new("profileVars", "A profile's overrides of this file's vars", &[
        key("id", Ty::Ref).required().doc("A profile defined in the config").example("staging"),
        key("vars", VARS).doc("Values that replace the file's vars under that profile"),
    ])
}

shape! {
    /// Keys a test and a `call` or `test` action share.
    request = Shape::new("request", "Values and checks for a call", &[
        key("vars", VARS).doc("Values for this call"),
        key("auth", AUTH).doc("Auth ID, `none`, or `id` + `vars`"),
        key("headers", TEMPLATES).doc("Merged over the api's headers"),
        key("query", TEMPLATES).doc("Merged over the api's query"),
        key("body", Ty::Value).doc("Replaces the api's body"),
        key("expect", Ty::Obj(expect)).scope(Scope::CALL).doc("Merged with the api's checks"),
        key("save", VARS).scope(Scope::CALL).doc("Values named for later steps, templated over `response`"),
        key("poll", Ty::Obj(poll)).doc("`until`, `every`, `timeout`"),
        key("retry", Ty::Obj(retry)).doc("`attempts`, `delay`"),
        key("timeout", Ty::Duration).doc("Time limit").example("30s"),
    ])
}

shape! {
    /// One entry of a lifecycle block or of `steps`.
    action = Shape::new("action", "One entry of a lifecycle block or of a test's steps", &[
        key("id", Ty::Id).doc("Names its saved values in shared blocks").example("catalog"),
        key("call", Ty::Ref).doc("The api to call").example("users#create"),
        key("test", Ty::Ref).doc("A test whose calls to run").example("shared.flows#checksOut"),
        key("function", Ty::Ref).doc("`path#export` to run").example("hooks/seed.ts#seedDb"),
        key("command", Ty::Template).doc("A shell command to run").example("docker compose up -d --wait"),
        key("keep", Ty::Bool).default("false").doc("Skip the api's cleanup"),
        key("cache", Ty::Obj(cache)).doc("Reuse the saves across runs"),
        key("repeat", Ty::Int).doc("Run the action this many times").example("10"),
        key("forEach", Ty::Value).doc("Run once per element of a list").example("${dataset.capitals}"),
        key("parallel", Ty::Int).default("1").doc("Iterations at a time").example("5"),
        key("provides", TEXTS).doc("Names a `function` or `test` action saves (D128)"),
    ])
    .mixins(&[request])
    .one_of(&["call", "test", "function", "command"], Code::V2201)
    .scopes(&[("forEach", Scope::FOR_EACH), ("repeat", Scope::LOOP)])
}

shape! {
    http = Shape::new("http", "Transport settings", &[
        key("retry", Ty::Obj(http_retry)).default("attempts: 0").doc("Retry one request on a transport failure (D143)"),
    ])
}

shape! {
    http_retry = Shape::new("httpRetry", "Transport retry of one request (D143)", &[
        key("attempts", Ty::Int).default("0").doc("Further attempts; 0 is off").example("2"),
        key("on", Ty::List(&Ty::Either(&[Ty::Int, Ty::Enum(&["connect", "timeout"])])))
            .default("[connect, timeout, 502, 503, 504]").doc("Failures and statuses that are retried"),
        key("methods", Ty::List(&Ty::Enum(METHODS))).default("[GET, HEAD, OPTIONS, PUT, DELETE]")
            .doc("Methods that are retried"),
        key("backoff", Ty::Obj(backoff)).doc("Pause between attempts"),
    ])
}

shape! {
    backoff = Shape::new("backoff", "Exponential backoff", &[
        key("base", Ty::Duration).default("100ms").doc("First pause"),
        key("max", Ty::Duration).default("2s").doc("Longest pause"),
        key("jitter", Ty::Bool).default("true").doc("Randomise each pause"),
    ])
}
