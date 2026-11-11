//! `vibranium.config.yaml` from v1's `config.json`: the default account's URL as `apiUrl`, the
//! env vars as vars or generators, and each account with credentials as an auth whose values
//! are `${secret.…}` names, never the values v1 kept in the file.

use serde_json::{Map, Value, json};
use vc_model::secret_name;

use super::source::Source;
use super::template::Templates;
use crate::naming::camel;

/// The v2 config's path.
pub const CONFIG_PATH: &str = "vibranium.config.yaml";

/// A v1 account method with a v2 auth type: its credentials as `(v2 key, secret suffix)`, and
/// its plain settings as `(v2 key, v1 key)`.
struct AuthKind {
    method: &'static str,
    kind: &'static str,
    secrets: &'static [(&'static str, &'static str)],
    plain: &'static [(&'static str, &'static str)],
}

const AUTHS: [AuthKind; 2] = [
    AuthKind {
        method: "basic",
        kind: "basic",
        secrets: &[("username", "USERNAME"), ("password", "PASSWORD")],
        plain: &[],
    },
    AuthKind {
        method: "client-credentials",
        kind: "oauth2ClientCredentials",
        secrets: &[("clientId", "CLIENT_ID"), ("clientSecret", "CLIENT_SECRET")],
        plain: &[("tokenUrl", "oauth_url")],
    },
];

/// The accounts of a v1 config.
pub struct Accounts<'s> {
    accounts: Option<&'s Map<String, Value>>,
    default: Option<&'s str>,
}

impl<'s> Accounts<'s> {
    /// The accounts in `config`.
    pub fn new(config: &'s Value) -> Self {
        let accounts = config.get("accounts").and_then(Value::as_object);
        Self { accounts, default: config.get("default_account").and_then(Value::as_str) }
    }

    /// The account an endpoint is sent as: its `system`, or the default account.
    pub fn of<'e>(&self, endpoint: &'e Value) -> Option<&'e str>
    where
        's: 'e,
    {
        endpoint.get("system").and_then(Value::as_str).or(self.default)
    }

    /// The v2 auth ID of `account`, when its method has a v2 auth type.
    pub fn auth(&self, account: &str) -> Option<String> {
        self.kind(account).map(|_| camel(account))
    }

    /// The `variables` of `account`.
    pub fn variables(&self, account: &str) -> Option<&'s Map<String, Value>> {
        self.get(account)?.get("variables").and_then(Value::as_object)
    }

    fn get(&self, account: &str) -> Option<&'s Value> {
        self.accounts?.get(account)
    }

    fn kind(&self, account: &str) -> Option<&'static AuthKind> {
        let method = self.get(account)?.get("method").and_then(Value::as_str)?;
        AUTHS.iter().find(|kind| kind.method == method)
    }
}

/// The config: project name, defaults, vars, the default profile, auth, secrets, generators.
/// Returns it with the secret names `.env.schema` declares.
pub fn config(source: &Source, templates: &mut Templates) -> (Value, Vec<String>) {
    let config = &source.config;
    let project = source.scenarios.iter().find_map(|scenario| scenario.json.get("app")?.as_str()).unwrap_or("project");
    let mut fields = Map::new();
    fields.insert("syntaxVersion".to_owned(), json!(2));
    fields.insert("project".to_owned(), json!(project));
    if let Some(concurrency) = config.pointer("/executor/max_parallel_executors") {
        fields.insert("defaults".to_owned(), json!({"concurrency": concurrency}));
    }
    fields.insert("vars".to_owned(), Value::Object(vars(config, templates)));
    fields.insert("profiles".to_owned(), json!([{"id": "default", "default": true}]));
    let (auth, secrets) = auth(config, templates);
    if !auth.is_empty() {
        fields.insert("auth".to_owned(), Value::Array(auth));
        fields.insert("secrets".to_owned(), json!({"provider": "env", "schema": ".env.schema"}));
    }
    let generators: Vec<Value> =
        templates.generators.iter().map(|(id, regex)| json!({"id": id, "fromRegExp": regex})).collect();
    if !generators.is_empty() {
        fields.insert("generators".to_owned(), Value::Array(generators));
    }
    (Value::Object(fields), secrets)
}

/// `apiUrl` from the default account, then the env vars that are not generators.
fn vars(config: &Value, templates: &mut Templates) -> Map<String, Value> {
    let accounts = Accounts::new(config);
    let url = accounts.default.and_then(|name| accounts.get(name)?.get("api_url")?.as_str());
    let mut vars = Map::new();
    vars.insert("apiUrl".to_owned(), json!(url.unwrap_or("http://localhost")));
    for (name, value) in config.get("env_vars").and_then(Value::as_object).into_iter().flatten() {
        if !templates.is_generated(name) {
            let value = templates.value(value, "env_vars");
            vars.insert(camel(name), value);
        }
    }
    vars
}

/// One auth per account with a v2 auth type, and the secret names they read.
fn auth(config: &Value, templates: &mut Templates) -> (Vec<Value>, Vec<String>) {
    let accounts = Accounts::new(config);
    let (mut auth, mut secrets) = (Vec::new(), Vec::new());
    for (name, account) in accounts.accounts.into_iter().flatten() {
        let Some(kind) = accounts.kind(name) else {
            let method = account.get("method").and_then(Value::as_str).unwrap_or("none");
            if method != "none" {
                templates.note(name, format!("account method `{method}` has no v2 auth type; add the auth by hand"));
            }
            continue;
        };
        let mut fields = Map::new();
        fields.insert("id".to_owned(), json!(camel(name)));
        fields.insert("type".to_owned(), json!(kind.kind));
        for (key, from) in kind.plain {
            fields.extend(account.get(from).map(|value| ((*key).to_owned(), value.clone())));
        }
        for (key, suffix) in kind.secrets {
            let secret = format!("{}_{suffix}", secret_name(name));
            fields.insert((*key).to_owned(), json!(format!("${{secret.{secret}}}")));
            secrets.push(secret);
        }
        auth.push(Value::Object(fields));
    }
    (auth, secrets)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{Accounts, config};
    use crate::v1::source::Source;
    use crate::v1::template::Templates;

    const CONFIG: &str = r#"{
        "accounts": {
            "public": {"api_url": "http://api.example", "method": "none"},
            "shop_admin": {"api_url": "http://api.example", "method": "basic",
                "credentials": {"username": "admin", "password": "hunter2"}},
            "partner": {"method": "client-credentials", "oauth_url": "http://api.example/token",
                "credentials": {"clientid": "c", "secret": "s3cret"}, "variables": {"tenant": "acme"}},
            "legacy": {"method": "saml"}
        },
        "default_account": "public",
        "env_vars": {"short_text": "[a-z]{5}", "currency": "EUR"},
        "executor": {"max_parallel_executors": 8}
    }"#;

    fn source() -> Source {
        Source::read(&BTreeMap::from([("config.json".to_owned(), CONFIG.to_owned())])).unwrap()
    }

    #[test]
    fn accounts_become_auth_reading_secrets_never_the_values() {
        let source = source();
        let mut templates = Templates::new(source.config.get("env_vars").and_then(|env| env.as_object()));
        let (doc, secrets) = config(&source, &mut templates);
        let auth = &doc["auth"];
        assert_eq!(
            auth[0],
            json!({"id": "partner", "type": "oauth2ClientCredentials", "tokenUrl": "http://api.example/token",
            "clientId": "${secret.PARTNER_CLIENT_ID}", "clientSecret": "${secret.PARTNER_CLIENT_SECRET}"})
        );
        assert_eq!(auth[1]["password"], json!("${secret.SHOP_ADMIN_PASSWORD}"));
        assert_eq!(
            secrets,
            ["PARTNER_CLIENT_ID", "PARTNER_CLIENT_SECRET", "SHOP_ADMIN_USERNAME", "SHOP_ADMIN_PASSWORD"]
        );
        assert!(!doc.to_string().contains("hunter2") && !doc.to_string().contains("s3cret"));
        assert!(templates.notes.iter().any(|note| note.at == "legacy" && note.message.contains("saml")));
    }

    #[test]
    fn the_default_account_url_and_env_vars_become_vars_and_generators() {
        let source = source();
        let mut templates = Templates::new(source.config.get("env_vars").and_then(|env| env.as_object()));
        let (doc, _) = config(&source, &mut templates);
        assert_eq!(doc["vars"], json!({"apiUrl": "http://api.example", "currency": "EUR"}));
        assert_eq!(doc["generators"], json!([{"id": "shortText", "fromRegExp": "[a-z]{5}"}]));
        assert_eq!(doc["defaults"], json!({"concurrency": 8}));
        assert_eq!(doc["secrets"], json!({"provider": "env", "schema": ".env.schema"}));
    }

    #[test]
    fn an_endpoint_is_sent_as_its_system_or_the_default_account() {
        let source = source();
        let accounts = Accounts::new(&source.config);
        let partner = json!({"system": "partner"});
        assert_eq!(accounts.of(&partner), Some("partner"));
        assert_eq!(accounts.of(&json!({})), Some("public"));
        assert_eq!(accounts.auth("partner").as_deref(), Some("partner"));
        assert_eq!(accounts.auth("public"), None);
        assert_eq!(accounts.variables("partner"), json!({"tenant": "acme"}).as_object());
    }
}
