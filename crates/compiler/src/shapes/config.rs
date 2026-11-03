//! `vibranium.config.yaml`.

use vc_diagnostics::Code;

use super::common::{ACTIONS, METHODS, TEMPLATES, TEXTS, VARS, expect, http, retry};
use super::types::{Scope, Shape, Ty, key};

/// The auth types (see Authentication).
const AUTH_TYPES: &[&str] = &[
    "basic",
    "bearer",
    "apiKey",
    "oauth2ClientCredentials",
    "oauth2AuthorizationCode",
    "oauth2Refresh",
    "flow",
    "plugin",
];

shape! {
    config = Shape::new("config", "The project config, `vibranium.config.yaml`", &[
        key("syntaxVersion", Ty::Int).missing(Code::V1003).doc("File-format version").example("2"),
        key("project", Ty::Text).default("folder name").doc("Display name in reports and on the server"),
        key("paths", Ty::Obj(paths)).doc("Where definition files are found"),
        key("defaults", Ty::Obj(defaults)).doc("Defaults for every request and test"),
        key("http", Ty::Obj(http)).doc("Transport settings"),
        key("vars", VARS).doc("Project-wide values"),
        key("profiles", Ty::List(&Ty::Obj(profile))).default("required, at least one (V3101)")
            .doc("Named sets of var overrides"),
        key("auth", Ty::List(&Ty::Obj(auth))).doc("Auth definitions"),
        key("secrets", Ty::Obj(secrets)).default("Varlock with `.env.schema`").doc("Where `${secret.*}` comes from"),
        key("generators", Ty::List(&Ty::Obj(generator))).doc("Named fake-data generators"),
        key("datasets", Ty::List(&Ty::Obj(dataset))).default("every file in `datasets/`").doc("Your own data lists"),
        key("reports", Ty::Obj(reports)).default("CTRF only").doc("Output formats and folder"),
        key("history", Ty::Obj(history)).doc("Run history and retention"),
        key("alerts", Ty::List(&Ty::Obj(alert))).doc("Notifications"),
        key("plugins", Ty::List(&Ty::Obj(plugin))).doc("Third-party plugins"),
        key("redact", Ty::Obj(redact)).default("auth headers and secrets").doc("Extra values to mask"),
        key("beforeRun", ACTIONS).doc("Actions run once before any test starts"),
        key("afterRun", ACTIONS).doc("Actions run once after the last test"),
    ])
}

shape! {
    paths = Shape::new("paths", "Where definition files are found", &[
        key("include", TEXTS).doc("Globs of files to read").example("[services/**]"),
        key("exclude", TEXTS).default("node_modules, .vibranium").doc("Globs of files to leave out"),
    ])
}

shape! {
    defaults = Shape::new("defaults", "Defaults for every request and test", &[
        key("timeout", Ty::Duration).default("30s").doc("Per request"),
        key("concurrency", Ty::Int).default("16").doc("Test instances in flight"),
        key("perHost", Ty::Int).default("8").doc("Connections per host"),
        key("retry", Ty::Obj(retry)).doc("Applied to every test unless overridden (D126)"),
        key("headers", TEMPLATES).doc("Sent with every request"),
    ])
}

shape! {
    profile = Shape::new("profile", "A named set of var overrides", &[
        key("id", Ty::Id).required().doc("Lowercase profile name").example("staging"),
        key("name", Ty::Text).doc("Shown in reports"),
        key("default", Ty::Bool).default("false").doc("Active when no profile is named"),
        key("vars", VARS).doc("Values that replace the config's vars"),
        key("agents", Ty::Value).doc("What agents may do under this profile"),
    ])
}

shape! {
    auth = Shape::new("auth", "An auth definition; the keys used depend on `type`", &[
        key("id", Ty::Id).required().doc("Referenced as `auth: <id>`").example("shopOauth"),
        key("name", Ty::Template).doc("Display name; for `apiKey`, the header or query name"),
        key("type", Ty::Enum(AUTH_TYPES)).required().doc("The scheme"),
        key("vars", VARS).doc("Values for the auth's templates"),
        key("username", Ty::Template).doc("`basic`"),
        key("password", Ty::Template).doc("`basic`"),
        key("token", Ty::Template).doc("`bearer`: a fixed token"),
        key("in", Ty::Enum(&["header", "query"])).doc("`apiKey`: where the key goes"),
        key("value", Ty::Template).doc("`apiKey`: the key"),
        key("tokenUrl", Ty::Template).doc("`oauth2*`: token endpoint"),
        key("authorizeUrl", Ty::Template).doc("`oauth2AuthorizationCode`: authorize endpoint"),
        key("clientId", Ty::Template).doc("`oauth2*`: client ID"),
        key("clientSecret", Ty::Template).doc("`oauth2ClientCredentials`: client secret"),
        key("refreshToken", Ty::Template).doc("`oauth2Refresh`: long-lived refresh token"),
        key("scopes", TEXTS).doc("`oauth2*`: scopes requested"),
        key("audience", Ty::Template).doc("`oauth2ClientCredentials`: audience"),
        key("pkce", Ty::Bool).doc("`oauth2AuthorizationCode`: use PKCE"),
        key("redirectPort", Ty::Int).doc("`oauth2AuthorizationCode`: local redirect port"),
        key("cache", Ty::Bool).default("false").doc("Keep the credentials across runs, encrypted"),
        key("steps", Ty::List(&Ty::Obj(flow_step))).doc("`flow`: the requests that produce credentials"),
        key("apply", Ty::Obj(apply)).doc("`flow`: what is added to each request"),
        key("expiresIn", Ty::Duration).doc("`flow`: when the credentials go stale; a template is fine"),
        key("refreshBefore", Ty::Duration).default("1m").doc("`flow`: re-run this long before expiry"),
        key("refreshOn", Ty::List(&Ty::Int)).default("[401]").doc("`flow`: statuses that re-run the flow"),
        key("plugin", Ty::Ref).doc("`plugin`: the auth plugin"),
        key("config", Ty::Value).doc("`plugin`: its settings"),
    ])
}

shape! {
    flow_step = Shape::new("flowStep", "One request of a flow auth", &[
        key("id", Ty::Id).doc("Names the step").example("login"),
        key("method", Ty::Enum(METHODS)).default("GET").doc("HTTP method"),
        key("url", Ty::Template).required().doc("The whole URL").example("${var.identityUrl}/login"),
        key("headers", TEMPLATES).doc("Request headers"),
        key("query", TEMPLATES).doc("Query string"),
        key("body", Ty::Value).doc("Request body"),
        key("expect", Ty::Obj(expect)).scope(Scope::CALL).doc("Checks on the response"),
        key("save", VARS).scope(Scope::CALL).doc("Values for later steps and `apply`"),
    ])
}

shape! {
    apply = Shape::new("apply", "What a flow auth adds to each request", &[
        key("headers", TEMPLATES).doc("Headers"),
        key("query", TEMPLATES).doc("Query parameters"),
        key("cookies", TEMPLATES).doc("Cookies"),
    ])
}

shape! {
    secrets = Shape::new("secrets", "Where secrets come from", &[
        key("provider", Ty::Text).default("varlock").doc("The secrets provider"),
        key("schema", Ty::Text).default(".env.schema").doc("The schema file"),
    ])
}

shape! {
    generator = Shape::new("generator", "A named fake-data generator; write one kind key", &[
        key("id", Ty::Id).required().doc("Used as `${fake.<id>}`").example("teamName"),
        key("fromRegExp", Ty::Text).doc("Text matching a pattern").example("\"[A-Z][a-z]{4,12} Team\""),
        key("oneOf", Ty::List(&Ty::Value)).doc("One of the values").example("[PRIVATE, PUBLIC]"),
        key("int", Ty::Obj(int_range)).doc("A whole number in a range"),
        key("lorem", Ty::Obj(lorem)).doc("Placeholder text"),
    ])
}

shape! {
    int_range = Shape::new("intRange", "A range of whole numbers", &[
        key("min", Ty::Int).doc("Smallest value"),
        key("max", Ty::Int).doc("Largest value"),
    ])
}

shape! {
    lorem = Shape::new("lorem", "Placeholder text", &[key("words", Ty::Int).doc("How many words")])
}

shape! {
    dataset = Shape::new("dataset", "A data list kept outside `datasets/`", &[
        key("id", Ty::Id).required().doc("Used as `${dataset.<id>}`").example("worldCities"),
        key("file", Ty::Text).required().doc("CSV, JSON or YAML file").example("../shared-data/world-cities.csv"),
    ])
}

shape! {
    reports = Shape::new("reports", "Report formats and folder", &[
        key("dir", Ty::Text).doc("Output folder"),
        key("formats", TEXTS).default("[ctrf]").doc("Report formats"),
        key("html", Ty::Obj(html)).doc("HTML report settings"),
    ])
}

shape! {
    html = Shape::new("html", "HTML report settings", &[
        key("layout", Ty::Enum(&["folder", "single"])).default("folder").doc("An index plus data files, or one file"),
    ])
}

shape! {
    history = Shape::new("history", "Run history and retention", &[
        key("bodies", Ty::Text).default("failures").doc("Which bodies are kept"),
        key("maxBodySize", Ty::Size).doc("Largest body kept").example("1mb"),
        key("retain", Ty::Obj(retain)).doc("How much history is kept"),
    ])
}

shape! {
    retain = Shape::new("retain", "How much history is kept", &[
        key("runs", Ty::Int).doc("Runs kept"),
        key("days", Ty::Int).doc("Days kept"),
    ])
}

shape! {
    alert = Shape::new("alert", "A notification rule", &[
        key("on", Ty::List(&Ty::Enum(&["failure", "recovery", "flaky", "threshold"]))).doc("Events that notify"),
        key("profiles", TEXTS).doc("Only runs under these profiles"),
        key("tags", TEXTS).doc("Only tests with these tags"),
        key("notify", Ty::List(&VARS)).doc("Channels: webhook, slack, teams, pagerduty, email"),
    ])
}

shape! {
    plugin = Shape::new("plugin", "A third-party plugin", &[
        key("id", Ty::Id).required().doc("Referenced by `plugin:`").example("acmeAuth"),
        key("source", Ty::Text).required().doc("Where it comes from").example("npm:@acme/vibranium-auth@2"),
        key("config", Ty::Value).doc("Its settings"),
    ])
}

shape! {
    redact = Shape::new("redact", "Extra values to mask", &[
        key("headers", TEXTS).doc("Header names"),
        key("fields", TEXTS).doc("Body field names"),
    ])
}
