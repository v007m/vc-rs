//! The API client's `ExplainApi` (D245): an api as `vc call` would send it, as data. It gives the
//! vars the api uses, each with the layers that set it, the headers it inherits, its auth and a
//! curl command. Every value is masked as a run's are (D229), and the token is `$TOKEN`.

use std::fmt::Write as _;

use vc_interfaces::ConfigEntry;
use vc_model::{Api, AuthRef, BodySpec, Namespace, PartSpec, Program, Reference, Segment, Template, TemplateValue};

use super::call::{CALL, Called};
use super::{Explainer, unknown};
use crate::call::given;
use crate::config_values::{ConfigValue, Masker, shown_unless_secret};
use crate::plan::{self, profile_flag};
use crate::shown::{self, Shown, Var};
use crate::{CallRequest, Engine, EngineError};

/// A header an api is sent with, and the level that set it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SentHeader {
    pub name: String,
    /// Masked, when [`Engine::explain_api`] gives it.
    pub value: String,
    /// `vibranium.config.yaml`, `file users`, `api users#get` or `call`.
    pub from: String,
}

/// The auth an api is sent with: named and described, never its token or a secret's value.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuthShown {
    pub id: String,
    /// Its `type`: `oauth2ClientCredentials`, `bearer`, a plugin's.
    pub kind: String,
    /// Its `tokenUrl` as written, masked, when it has one.
    pub token_url: Option<String>,
    /// The secrets its settings read, by name.
    pub secrets: Vec<String>,
}

/// An api as `vc call` would send it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ApiExplained {
    /// Each var the api uses, directly or through another var, with its bare name as the key and
    /// the layers that set it, the value in force first.
    pub vars: Vec<ConfigValue>,
    /// The headers it is sent with that it does not write itself.
    pub headers: Vec<SentHeader>,
    pub auth: Option<AuthShown>,
    /// The request as a curl command, without its body, the auth's token as `$TOKEN`.
    pub curl: String,
    pub notices: Vec<String>,
}

impl Engine {
    /// The api `request.api` as `vc call` would send it, with the `name=value` vars of
    /// `request.vars` at call level, under its profiles and settings.
    ///
    /// # Errors
    ///
    /// [`EngineError::Invalid`] when the project has errors, [`EngineError::Setting`] for a bad
    /// setting, var or an unknown profile, and [`EngineError::Selection`] when no api has the ID.
    pub async fn explain_api(&self, request: &CallRequest) -> Result<ApiExplained, EngineError> {
        let flags: Vec<ConfigEntry> = profile_flag(&request.profiles).into_iter().collect();
        let mut layered = self.layered(&request.sets, &flags)?;
        let program = layered.program.clone();
        let api = program.apis.values().find(|api| program.str(api.id.sym()) == Some(request.api.as_str()));
        let api = api.ok_or_else(|| unknown(&program, &request.api))?;
        let given = given(&request.vars)?;
        let (active, names) = plan::profiles(&program, layered.settings.profiles.as_ref())?;
        let masker = self.masker(&mut layered).await;
        let explainer = Explainer { program: &program, active: &active, names: &names, entries: &layered.entries };
        let called = Called { api, action: None };
        let mut vars = explainer.config_vars();
        explainer.api_vars(api, &mut vars);
        for (name, value) in given {
            let value = shown::literal(value.as_str().unwrap_or_default());
            shown::set(&mut vars, Var { name, value, origin: CALL.to_owned(), saved: false, replaced: Vec::new() });
        }
        let shown = Shown { program: &program, vars: &vars };
        let sent = masked_headers(&masker, explainer.sent_headers(&called, &shown)).await;
        let auth = explainer.auth_shown(&called, &masker).await;
        let url = masker.text(&url(&program, api, &shown)).await;
        let curl = curl(api, &url, &sent) + if auth.is_some() { BEARER } else { "" };
        let headers = sent.into_iter().filter(|header| !writes(api, &header.name)).collect();
        let vars = layers(&masker, &program, used(&explainer, &called, vars)).await;
        Ok(ApiExplained { vars, headers, auth, curl, notices: layered.notices })
    }
}

impl Explainer<'_> {
    /// The auth `called` is sent with, when it uses one.
    async fn auth_shown(&self, called: &Called<'_>, masker: &Masker) -> Option<AuthShown> {
        let (AuthRef::Use { id, .. }, _) = self.auth_used(called)? else { return None };
        let def = self.program.auth.get(id)?;
        let written = Shown { program: self.program, vars: &[] };
        let token_url = match def.settings.get("tokenUrl") {
            Some(url) => Some(masker.text(&shown_unless_secret(&written, url)).await),
            None => None,
        };
        let mut secrets = Vec::new();
        for reference in def.settings.values().flat_map(TemplateValue::references) {
            if let Some(name) = named(reference, Namespace::Secret).filter(|name| !secrets.contains(name)) {
                secrets.push(name);
            }
        }
        Some(AuthShown { id: self.text(id.sym()), kind: def.kind.clone(), token_url, secrets })
    }

    /// The references `called`'s base URL, path, query, headers and body make.
    fn request_references<'r>(&'r self, called: &Called<'r>) -> Vec<&'r Reference> {
        let api = called.api;
        let group = self.program.api_groups.get(&api.group);
        let mut found: Vec<&Reference> = api.path.references().collect();
        found.extend(group.and_then(|group| group.base_url.as_ref()).into_iter().flat_map(Template::references));
        found.extend(api.query.values().flat_map(Template::references));
        for (_, headers) in self.header_levels(called) {
            found.extend(headers.values().flat_map(Template::references));
        }
        body_references(&api.body, &mut found);
        found
    }
}

/// The vars of `vars` that `called` uses, directly or through another var, in their order.
fn used(explainer: &Explainer<'_>, called: &Called<'_>, vars: Vec<Var>) -> Vec<Var> {
    let mut names: Vec<String> = Vec::new();
    let references = explainer.request_references(called);
    add_vars(&mut names, references.into_iter());
    let mut at = 0;
    while let Some(name) = names.get(at).cloned() {
        at += 1;
        if let Some(var) = vars.iter().find(|var| var.name == name) {
            add_vars(&mut names, var.value.references().into_iter());
        }
    }
    vars.into_iter().filter(|var| names.contains(&var.name)).collect()
}

/// Adds the name of each `${var.*}` in `references` that `names` does not have.
fn add_vars<'r>(names: &mut Vec<String>, references: impl Iterator<Item = &'r Reference>) {
    for name in references.filter_map(|reference| named(reference, Namespace::Var)) {
        if !names.contains(&name) {
            names.push(name);
        }
    }
}

/// The name `reference` reads in `namespace`: `x` for `${var.x.y}`.
fn named(reference: &Reference, namespace: Namespace) -> Option<String> {
    let Some(Segment::Key(name)) = reference.path.segments.first() else { return None };
    (reference.ns == namespace).then(|| name.clone())
}

/// Adds the references a body makes to `found`.
fn body_references<'b>(body: &'b BodySpec, found: &mut Vec<&'b Reference>) {
    match body {
        BodySpec::None => {}
        BodySpec::Json(value) => value.collect_references(found),
        BodySpec::Text(template) | BodySpec::File(template) | BodySpec::Base64(template) => {
            found.extend(template.references());
        }
        BodySpec::Extend { base, changes, .. } => {
            found.extend(base.references());
            changes.collect_references(found);
        }
        BodySpec::Form(fields) => fields.values().for_each(|value| value.collect_references(found)),
        BodySpec::Multipart(parts) => {
            for part in parts.values() {
                match part {
                    PartSpec::Value(value) => value.collect_references(found),
                    PartSpec::File(template) => found.extend(template.references()),
                }
            }
        }
    }
}

/// Each var with its layers, written as its levels write them, masked, the value in force first.
async fn layers(masker: &Masker, program: &Program, vars: Vec<Var>) -> Vec<ConfigValue> {
    let written = Shown { program, vars: &[] };
    let mut values = Vec::with_capacity(vars.len());
    for var in vars {
        let replaced = var.replaced.iter().rev().map(|(value, origin)| (value, origin.clone()));
        let lowest_first = replaced.chain(std::iter::once((&var.value, var.origin.clone())));
        let layers = lowest_first.map(|(value, origin)| (shown_unless_secret(&written, value), origin)).collect();
        values.push(masker.value(var.name.clone(), layers).await);
    }
    values
}

/// `sent` with each value masked.
async fn masked_headers(masker: &Masker, sent: Vec<SentHeader>) -> Vec<SentHeader> {
    let mut headers = Vec::with_capacity(sent.len());
    for header in sent {
        let value = masker.masked(&header.name, header.value, &header.from).await;
        headers.push(SentHeader { value, ..header });
    }
    headers
}

/// Whether `api` writes the header `name` itself, in any case.
fn writes(api: &Api, name: &str) -> bool {
    api.headers.keys().any(|written| written.eq_ignore_ascii_case(name))
}

/// The base URL, the path and the query, the vars filled in.
fn url(program: &Program, api: &Api, shown: &Shown<'_>) -> String {
    let base = program.api_groups.get(&api.group).and_then(|group| group.base_url.as_ref());
    let mut url = base.map(|base| shown.template(base)).unwrap_or_default();
    url.push_str(&shown.template(&api.path));
    let query: Vec<String> =
        api.query.iter().map(|(name, value)| format!("{name}={}", shown.template(value))).collect();
    if !query.is_empty() {
        url.push('?');
        url.push_str(&query.join("&"));
    }
    url
}

/// The header that stands for the auth's token, in double quotes so a shell fills in `$TOKEN`.
const BEARER: &str = " \\\n  -H \"Authorization: Bearer $TOKEN\"";

/// `curl -X GET 'url'` and a `-H` for each header.
fn curl(api: &Api, url: &str, headers: &[SentHeader]) -> String {
    let method = format!("{:?}", api.method).to_uppercase();
    let mut curl = format!("curl -X {method} {}", quoted(url));
    for header in headers {
        let _ = write!(curl, " \\\n  -H {}", quoted(&format!("{}: {}", header.name, header.value)));
    }
    curl
}

/// `text` in single quotes, for a POSIX shell.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

#[cfg(test)]
#[path = "explain_api_tests.rs"]
mod tests;
