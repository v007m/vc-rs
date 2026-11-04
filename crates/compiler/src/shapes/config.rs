//! `vibranium.config.yaml`.

use vc_diagnostics::Code;

use super::common;
use super::common::{ACTIONS, METHODS, TEMPLATES, TEXTS, VARS};
use super::types;
use super::types::{Scope, Shape, Ty};

/// The report files `reports.formats` can name; html comes in M3.
const REPORT_FORMATS: &[&str] = &["ctrf", "junit", "html"];

/// The auth types (see Authentication).
const AUTH_TYPES: &[&str] = &[
    "basic",
    "bearer",
    "apiKey",
    "oauth2ClientCredentials",
    "oauth2Password",
    "oauth2AuthorizationCode",
    "oauth2Refresh",
    "flow",
    "plugin",
];

shape! {
    config = Shape::new("config", "The project config, `vibranium.config.yaml`", &[
        types::key("syntaxVersion", Ty::Int).missing(Code::V1003).doc("File-format version").example("2"),
        types::key("project", Ty::Text).default("folder name").doc("Display name in reports and on the server"),
        types::key("paths", Ty::Obj(paths)).doc("Where definition files are found"),
        types::key("defaults", Ty::Obj(defaults)).doc("Defaults for every request and test"),
        types::key("http", Ty::Obj(common::http)).doc("Transport settings"),
        types::key("vars", VARS).doc("Project-wide values"),
        types::key("profiles", Ty::List(&Ty::Obj(profile))).default("required, at least one (V3101)")
            .doc("Named sets of var overrides"),
        types::key("auth", Ty::List(&Ty::Obj(auth))).doc("Auth definitions"),
        types::key("secrets", Ty::Obj(secrets)).default("`env` with `.env.schema`").doc("Where `${secret.*}` comes from"),
        types::key("generators", Ty::List(&Ty::Obj(generator))).doc("Named fake-data generators"),
        types::key("datasets", Ty::List(&Ty::Obj(dataset))).default("every file in `datasets/`").doc("Your own data lists"),
        types::key("reports", Ty::Obj(reports)).default("CTRF only").doc("Output formats and folder"),
        types::key("history", Ty::Obj(history)).doc("Run history and retention"),
        types::key("alerts", Ty::List(&Ty::Obj(alert))).doc("Notifications"),
        types::key("plugins", Ty::List(&Ty::Obj(plugin))).doc("Third-party plugins"),
        types::key("redact", Ty::Obj(redact)).default("auth headers and secrets").doc("Extra values to mask"),
        types::key("beforeRun", ACTIONS).doc("Actions run once before any test starts"),
        types::key("afterRun", ACTIONS).doc("Actions run once after the last test"),
    ])
}

shape! {
    paths = Shape::new("paths", "Where definition files are found", &[
        types::key("include", TEXTS).doc("Globs of files to read").example("[services/**]"),
        types::key("exclude", TEXTS).default("node_modules, .vibranium").doc("Globs of files to leave out"),
    ])
}

shape! {
    defaults = Shape::new("defaults", "Defaults for every request and test", &[
        types::key("timeout", Ty::Duration).default("30s").doc("Per request"),
        types::key("concurrency", Ty::Int).default("16").doc("Test instances in flight"),
        types::key("perHost", Ty::Int).default("8").doc("Connections per host"),
        types::key("retry", Ty::Obj(common::retry)).doc("Applied to every test unless overridden (D126)"),
        types::key("headers", TEMPLATES).doc("Sent with every request"),
    ])
}

shape! {
    profile = Shape::new("profile", "A named set of var overrides", &[
        types::key("id", Ty::Id).required().doc("Lowercase profile name").example("staging"),
        types::key("name", Ty::Text).doc("Shown in reports"),
        types::key("default", Ty::Bool).default("false").doc("Active when no profile is named"),
        types::key("vars", VARS).doc("Values that replace the config's vars"),
        types::key("agents", Ty::Value).doc("What agents may do under this profile"),
    ])
}

shape! {
    auth = Shape::new("auth", "An auth definition; the keys used depend on `type`", &[
        types::key("id", Ty::Id).required().doc("Referenced as `auth: <id>`").example("shopOauth"),
        types::key("name", Ty::Template).doc("Display name; for `apiKey`, the header or query name"),
        types::key("type", Ty::Enum(AUTH_TYPES)).required().doc("The scheme"),
        types::key("vars", VARS).doc("Values for the auth's templates"),
        types::key("username", Ty::Template).doc("`basic`, `oauth2Password`"),
        types::key("password", Ty::Template).doc("`basic`, `oauth2Password`"),
        types::key("token", Ty::Template).doc("`bearer`: a fixed token"),
        types::key("in", Ty::Enum(&["header", "query"])).doc("`apiKey`: where the key goes"),
        types::key("value", Ty::Template).doc("`apiKey`: the key"),
        types::key("tokenUrl", Ty::Template).doc("`oauth2*`: token endpoint"),
        types::key("authorizeUrl", Ty::Template).doc("`oauth2AuthorizationCode`: authorize endpoint"),
        types::key("clientId", Ty::Template).doc("`oauth2*`: client ID"),
        types::key("clientSecret", Ty::Template).doc("`oauth2*`: client secret, where the server wants one"),
        types::key("refreshToken", Ty::Template).doc("`oauth2Refresh`: long-lived refresh token"),
        types::key("scopes", TEXTS).doc("`oauth2*`: scopes requested"),
        types::key("audience", Ty::Template).doc("`oauth2ClientCredentials`: audience"),
        types::key("pkce", Ty::Bool).doc("`oauth2AuthorizationCode`: use PKCE"),
        types::key("redirectPort", Ty::Int).doc("`oauth2AuthorizationCode`: local redirect port"),
        types::key("cache", Ty::Bool).default("false").doc("Keep the credentials across runs, encrypted"),
        types::key("steps", Ty::List(&Ty::Obj(flow_step))).doc("`flow`: the requests that produce credentials"),
        types::key("apply", Ty::Obj(apply)).doc("`flow`: what is added to each request"),
        types::key("expiresIn", Ty::Duration).doc("`flow`: when the credentials go stale; a template is fine"),
        types::key("refreshBefore", Ty::Duration).default("1m").doc("`flow`: re-run this long before expiry"),
        types::key("refreshOn", Ty::List(&Ty::Int)).default("[401]").doc("`flow`: statuses that re-run the flow"),
        types::key("plugin", Ty::Ref).doc("`plugin`: the auth plugin"),
        types::key("config", Ty::Value).doc("`plugin`: its settings"),
    ])
}

shape! {
    flow_step = Shape::new("flowStep", "One request of a flow auth", &[
        types::key("id", Ty::Id).doc("Names the step").example("login"),
        types::key("method", Ty::Enum(METHODS)).default("GET").doc("HTTP method"),
        types::key("url", Ty::Template).required().doc("The whole URL").example("${var.identityUrl}/login"),
        types::key("headers", TEMPLATES).doc("Request headers"),
        types::key("query", TEMPLATES).doc("Query string"),
        types::key("body", Ty::Value).doc("Request body"),
        types::key("expect", Ty::Obj(common::expect)).scope(Scope::CALL).doc("Checks on the response"),
        types::key("save", VARS).scope(Scope::CALL).doc("Values for later steps and `apply`"),
    ])
}

shape! {
    apply = Shape::new("apply", "What a flow auth adds to each request", &[
        types::key("headers", TEMPLATES).doc("Headers"),
        types::key("query", TEMPLATES).doc("Query parameters"),
        types::key("cookies", TEMPLATES).doc("Cookies"),
    ])
}

shape! {
    secrets = Shape::new("secrets", "Where secrets come from", &[
        types::key("provider", Ty::Enum(&["env", "varlock"])).default("env").doc("The secrets provider (D163)"),
        types::key("schema", Ty::Text).default(".env.schema").doc("The file that declares the secret names"),
    ])
}

shape! {
    generator = Shape::new("generator", "A named fake-data generator; write one kind key", &[
        types::key("id", Ty::Id).required().doc("Used as `${fake.<id>}`").example("teamName"),
        types::key("fromRegExp", Ty::Text).doc("Text matching a pattern").example("\"[A-Z][a-z]{4,12} Team\""),
        types::key("oneOf", Ty::List(&Ty::Value)).doc("One of the values").example("[PRIVATE, PUBLIC]"),
        types::key("int", Ty::Obj(int_range)).doc("A whole number in a range"),
        types::key("lorem", Ty::Obj(lorem)).doc("Placeholder text"),
    ])
}

shape! {
    int_range = Shape::new("intRange", "A range of whole numbers", &[
        types::key("min", Ty::Int).doc("Smallest value"),
        types::key("max", Ty::Int).doc("Largest value"),
    ])
}

shape! {
    lorem = Shape::new("lorem", "Placeholder text", &[types::key("words", Ty::Int).doc("How many words")])
}

shape! {
    dataset = Shape::new("dataset", "A data list kept outside `datasets/`", &[
        types::key("id", Ty::Id).required().doc("Used as `${dataset.<id>}`").example("worldCities"),
        types::key("file", Ty::Text).required().doc("CSV, JSON or YAML file").example("../shared-data/world-cities.csv"),
    ])
}

shape! {
    reports = Shape::new("reports", "Report formats and folder", &[
        types::key("dir", Ty::Text).doc("Output folder"),
        types::key("formats", Ty::List(&Ty::Enum(REPORT_FORMATS))).default("[ctrf]").doc("Report formats"),
        types::key("html", Ty::Obj(html)).doc("HTML report settings"),
    ])
}

shape! {
    html = Shape::new("html", "HTML report settings", &[
        types::key("layout", Ty::Enum(&["folder", "single"])).default("folder").doc("An index plus data files, or one file"),
    ])
}

shape! {
    history = Shape::new("history", "Run history and retention", &[
        types::key("bodies", Ty::Text).default("failures").doc("Which bodies are kept"),
        types::key("maxBodySize", Ty::Size).doc("Largest body kept").example("1mb"),
        types::key("retain", Ty::Obj(retain)).doc("How much history is kept"),
    ])
}

shape! {
    retain = Shape::new("retain", "How much history is kept", &[
        types::key("runs", Ty::Int).doc("Runs kept"),
        types::key("days", Ty::Int).doc("Days kept"),
    ])
}

shape! {
    alert = Shape::new("alert", "A notification rule", &[
        types::key("on", Ty::List(&Ty::Enum(&["failure", "recovery", "flaky", "threshold"]))).doc("Events that notify"),
        types::key("profiles", TEXTS).doc("Only runs under these profiles"),
        types::key("tags", TEXTS).doc("Only tests with these tags"),
        types::key("notify", Ty::List(&VARS)).doc("Channels: webhook, slack, teams, pagerduty, email"),
    ])
}

shape! {
    plugin = Shape::new("plugin", "A third-party plugin", &[
        types::key("id", Ty::Id).required().doc("Referenced by `plugin:`").example("acmeAuth"),
        types::key("source", Ty::Text).required().doc("Where it comes from").example("npm:@acme/vibranium-auth@2"),
        types::key("config", Ty::Value).doc("Its settings"),
    ])
}

shape! {
    redact = Shape::new("redact", "Extra values to mask", &[
        types::key("headers", TEXTS).doc("Header names"),
        types::key("fields", TEXTS).doc("Body field names"),
        types::key("rules", Ty::List(&Ty::Obj(redact_rule))).doc("Your own masking rules (D171)"),
    ])
}

shape! {
    redact_rule = Shape::new("redactRule", "One masking rule; write `regex` or `cel`", &[
        types::key("regex", Ty::Text).doc("Masks every match in any text").example("\"\\d{3}-\\d{2}-\\d{4}\""),
        types::key("cel", Ty::Cel).scope(Scope::FOR_EACH)
            .doc("Masks a JSON field when true; `item.name` and `item.value` are the field").example("item.name == 'ssn'"),
    ])
}
