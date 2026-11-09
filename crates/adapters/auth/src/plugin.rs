//! `plugin`: the credentials an auth plugin's `auth/credentials` returns.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::{Value, json};
use vc_interfaces::{AuthError, AuthProvider, AuthRequest, CALL_DEADLINE, Credentials, PluginHost, Secret};

use crate::token::required;

/// `plugin`: calls `auth/credentials` on the plugin `plugin` names, with the definition's ID and
/// its `config`, and applies the `headers`, `query` and `cookies` it returns for `expiresIn`
/// seconds.
#[derive(Clone, Default)]
pub struct PluginAuth {
    plugins: BTreeMap<String, Arc<dyn PluginHost>>,
}

impl PluginAuth {
    /// No plugins.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// These plugins, with `host` added under its manifest's ID.
    #[must_use]
    pub fn with(mut self, host: Arc<dyn PluginHost>) -> Self {
        self.plugins.insert(host.manifest().id.clone(), host);
        self
    }
}

impl fmt::Debug for PluginAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.plugins.keys()).finish()
    }
}

#[async_trait]
impl AuthProvider for PluginAuth {
    async fn credentials(&self, request: &AuthRequest) -> Result<Credentials, AuthError> {
        let id = required(request, "plugin")?;
        let failed = |reason: String| AuthError::Plugin { auth: request.auth.clone(), reason };
        let host = self.plugins.get(id).ok_or_else(|| failed(format!("no auth plugin `{id}` is ready")))?;
        let config = request.settings.get("config").cloned().unwrap_or(Value::Null);
        let params = json!({"auth": request.auth, "config": config});
        let answer = host.call("auth/credentials", params, CALL_DEADLINE).await.map_err(|e| failed(e.to_string()))?;
        credentials(&answer).ok_or_else(|| failed("`auth/credentials` returned no headers, query or cookies".into()))
    }
}

/// What `auth/credentials` answered, when it holds at least one value.
fn credentials(answer: &Value) -> Option<Credentials> {
    let mut credentials = Credentials::default();
    credentials.headers = pairs(answer, "headers");
    credentials.query = pairs(answer, "query");
    credentials.cookies = pairs(answer, "cookies");
    credentials.expires_in = answer.get("expiresIn").and_then(Value::as_u64).map(Duration::from_secs);
    let empty = credentials.values().next().is_none();
    (!empty).then_some(credentials)
}

/// The text values of the object `key` of `answer`.
fn pairs(answer: &Value, key: &str) -> Vec<(String, Secret)> {
    let Some(object) = answer.get(key).and_then(Value::as_object) else { return Vec::new() };
    object.iter().filter_map(|(name, value)| Some((name.clone(), Secret::new(value.as_str()?)))).collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::{Map, Value, json};
    use vc_interfaces::{
        AuthError, AuthProvider, AuthRequest, PluginKind, PluginManifest, PluginPermissions, Secret, Secrets,
    };
    use vc_testkit::{FakePlugin, auth_provider_suite};

    use super::PluginAuth;

    fn manifest() -> PluginManifest {
        PluginManifest {
            id: "acmeSso".into(),
            version: "1.0.0".into(),
            kind: PluginKind::Auth,
            command: "acme-sso".into(),
            args: Vec::new(),
            permissions: PluginPermissions { secrets: vec!["ssoKey".into()], ..PluginPermissions::default() },
        }
    }

    fn auth() -> PluginAuth {
        let mut secrets = Secrets::new();
        secrets.insert("ssoKey", Secret::new("s3cret-key"));
        let plugin = FakePlugin::new(manifest(), &secrets).on("auth/credentials", |params, secrets| {
            let tenant = params["config"]["tenant"].as_str().unwrap_or_default();
            let key = secrets.get("ssoKey").map(Secret::expose).unwrap_or_default();
            Ok(json!({"headers": {"authorization": format!("SSO {tenant}:{key}")}, "expiresIn": 60}))
        });
        PluginAuth::new().with(Arc::new(plugin))
    }

    fn request(settings: &Value) -> AuthRequest {
        let settings: Map<String, Value> = settings.as_object().cloned().unwrap_or_default();
        AuthRequest::new("partnerSso", "plugin", settings)
    }

    #[tokio::test]
    async fn an_auth_plugin_meets_the_auth_provider_contract() {
        let served = request(&json!({"plugin": "acmeSso", "config": {"tenant": "eu"}}));
        auth_provider_suite(&auth(), &served, &request(&json!({"plugin": "other"}))).await;
        let credentials = auth().credentials(&served).await.unwrap();
        assert_eq!(credentials.values().collect::<Vec<_>>(), ["SSO eu:s3cret-key"]);
        assert_eq!(credentials.expires_in, Some(Duration::from_secs(60)));
    }

    #[tokio::test]
    async fn a_missing_plugin_or_empty_answer_is_named() {
        let missing = auth().credentials(&request(&json!({}))).await.unwrap_err();
        assert_eq!(missing, AuthError::Missing { auth: "partnerSso".into(), key: "plugin".into() });
        let unknown = auth().credentials(&request(&json!({"plugin": "other"}))).await.unwrap_err();
        assert_eq!(unknown.to_string(), "the `partnerSso` auth plugin failed: no auth plugin `other` is ready");
        let empty = PluginAuth::new().with(Arc::new(
            FakePlugin::new(manifest(), &Secrets::new()).on("auth/credentials", |_, _| Ok(json!({"headers": {}}))),
        ));
        let error = empty.credentials(&request(&json!({"plugin": "acmeSso"}))).await.unwrap_err();
        assert!(error.to_string().ends_with("returned no headers, query or cookies"), "{error}");
        assert_eq!(format!("{:?}", auth()), r#"{"acmeSso"}"#);
    }
}
