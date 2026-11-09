//! A 3.1 spec as a draft api file: one api per operation.

use indexmap::IndexMap;
use oas3::Spec;
use oas3::spec::{ObjectOrReference, Operation, Parameter, ParameterIn};
use serde_json::{Value, json};
use vc_interfaces::{DraftApi, DraftGroup, ImportNote, ImportOptions};
use vc_model::Method;

use super::values::{body, generated, sample};
use crate::naming::{camel, from_route};
use crate::url::split_origin;

/// One spec, read for one draft.
pub struct Reader<'a> {
    /// The spec, typed.
    pub spec: &'a Spec,
    /// The same spec as JSON, for following schema references.
    pub doc: &'a Value,
    /// What was asked for.
    pub opts: &'a ImportOptions,
}

/// An operation the draft takes: where it is and how the spec describes it.
pub struct Route<'a> {
    pub path: String,
    pub method: Method,
    pub op: &'a Operation,
}

impl Reader<'_> {
    /// The operations the draft takes, in spec order, and a note for each one it leaves out
    /// because no api method fits.
    pub fn routes(&self) -> (Vec<Route<'_>>, Vec<ImportNote>) {
        let mut routes = Vec::new();
        let mut notes = Vec::new();
        for (path, method, op) in self.spec.operations() {
            let Some(known) = Method::named(method.as_str()) else {
                let message = format!("skipped: an api cannot use {method}");
                notes.push(ImportNote { at: format!("{method} {path}"), message });
                continue;
            };
            let route = Route { path, method: known, op };
            if self.wanted(&route) {
                routes.push(route);
            }
        }
        (routes, notes)
    }

    /// `--include` keeps operations with one of its tags or path prefixes; `--only` keeps the
    /// operations it names by `operationId` or draft ID.
    fn wanted(&self, route: &Route<'_>) -> bool {
        let include = &self.opts.include;
        let included = include.is_empty()
            || include.iter().any(|want| route.op.tags.contains(want) || route.path.starts_with(want.as_str()));
        let only = &self.opts.only;
        let named = only.is_empty()
            || only.iter().any(|want| route.op.operation_id.as_ref() == Some(want) || *want == id(route));
        included && named
    }

    /// The api file the routes fill.
    pub fn group(&self, source: &str, routes: &[Route<'_>]) -> (DraftGroup, Vec<ImportNote>) {
        let (base_url, var, origin) = self.server(source);
        let mut notes = Vec::new();
        let mut apis: Vec<DraftApi> = Vec::new();
        for route in routes {
            let mut api = self.api(route, &mut notes);
            while apis.iter().any(|seen| seen.id == api.id) {
                api.id.push('2');
            }
            apis.push(api);
        }
        let name = Some(self.spec.info.title.clone()).filter(|title| !title.is_empty());
        let config_vars = IndexMap::from([(var, json!(origin))]);
        let group = DraftGroup {
            file_id: self.opts.file_id.clone(),
            name,
            base_url: Some(base_url),
            config_vars,
            apis,
            tests: Vec::new(),
        };
        (group, notes)
    }

    /// `baseUrl`, the config var it reads and that var's value: the first server's origin, or
    /// the origin the spec was fetched from for a server URL that is only a path.
    fn server(&self, source: &str) -> (String, String, String) {
        let var = format!("{}Url", camel(&self.opts.file_id));
        let url = self.spec.servers.first().map(|server| expand(&server.url, &server.variables)).unwrap_or_default();
        let (origin, path) = match split_origin(&url) {
            Some((origin, path)) => (origin.to_owned(), path.to_owned()),
            None => (split_origin(source).map_or("http://localhost", |(origin, _)| origin).to_owned(), url),
        };
        let base_url = format!("${{var.{var}}}{}", path.trim_end_matches('/'));
        (base_url, var, origin)
    }

    fn api(&self, route: &Route<'_>, notes: &mut Vec<ImportNote>) -> DraftApi {
        let at = format!("{} {}", route.method.name(), route.path);
        let params = self.parameters(route, notes, &at);
        let mut api = DraftApi {
            id: id(route),
            name: route.op.summary.clone(),
            method: route.method,
            path: templated(&route.path),
            operation_id: route.op.operation_id.clone(),
            ..DraftApi::default()
        };
        for param in &params {
            self.parameter(&mut api, param);
        }
        notes.extend(self.request_body(&mut api, route, &at));
        api
    }

    /// The path item's parameters and the operation's, the operation's winning on a clash.
    fn parameters(&self, route: &Route<'_>, notes: &mut Vec<ImportNote>, at: &str) -> Vec<Parameter> {
        let shared =
            self.spec.paths.as_ref().and_then(|paths| paths.get(&route.path)).map(|item| item.parameters.as_slice());
        let mut params: Vec<Parameter> = Vec::new();
        for param in shared.unwrap_or_default().iter().chain(&route.op.parameters) {
            match self.resolved(param) {
                Some(param) => {
                    params.retain(|seen| (seen.name.as_str(), seen.location) != (param.name.as_str(), param.location));
                    params.push(param);
                }
                None => notes.push(ImportNote {
                    at: at.to_owned(),
                    message: "skipped a parameter its $ref does not name".to_owned(),
                }),
            }
        }
        params
    }

    fn resolved(&self, param: &ObjectOrReference<Parameter>) -> Option<Parameter> {
        param.resolve(self.spec).ok()
    }

    /// A path parameter becomes a var; a query parameter that is required or has a default
    /// becomes a var and a `query` entry reading it.
    fn parameter(&self, api: &mut DraftApi, param: &Parameter) {
        let schema = param.schema.as_ref().and_then(|schema| serde_json::to_value(schema).ok()).unwrap_or(Value::Null);
        let shown = param.example.clone().or_else(|| sample(super::values::resolve(self.doc, &schema)));
        let var = camel(&param.name);
        match param.location {
            ParameterIn::Path => {
                api.vars.insert(var, shown.unwrap_or_else(|| generated(self.doc, &param.name, &schema)));
            }
            ParameterIn::Query if param.required == Some(true) || shown.is_some() => {
                api.query.insert(param.name.clone(), format!("${{var.{var}}}"));
                api.vars.insert(var, shown.unwrap_or_else(|| generated(self.doc, &param.name, &schema)));
            }
            _ => {}
        }
    }

    /// The body for the request's JSON content (else its first content type): the media
    /// type's example when it shows one, else the schema's required fields as vars. A note for
    /// each body field that shares a name with a parameter.
    fn request_body(&self, api: &mut DraftApi, route: &Route<'_>, at: &str) -> Vec<ImportNote> {
        let Ok(Some(request)) = route.op.request_body(self.spec) else { return Vec::new() };
        let content =
            request.content.iter().find(|(ct, _)| *ct == "application/json").or_else(|| request.content.iter().next());
        let Some((ct, media)) = content else { return Vec::new() };
        let media = serde_json::to_value(media).unwrap_or(Value::Null);
        let example = media.get("example").cloned().or_else(|| first_example(&media));
        let (sent, vars) = example.map_or_else(
            || body(self.doc, media.get("schema").unwrap_or(&Value::Null)),
            |example| (example, IndexMap::new()),
        );
        api.body = Some(sent);
        api.content_type = (ct != "application/json").then(|| ct.clone());
        merge_vars(api, vars, at)
    }
}

/// Adds the body's vars to `api`, a parameter's var winning, with a note for each clash.
fn merge_vars(api: &mut DraftApi, vars: IndexMap<String, Value>, at: &str) -> Vec<ImportNote> {
    let mut notes = Vec::new();
    for (var, value) in vars {
        if api.vars.contains_key(&var) {
            let message = format!("body field {var} shares a name with a parameter");
            notes.push(ImportNote { at: at.to_owned(), message });
        }
        api.vars.entry(var).or_insert(value);
    }
    notes
}

/// The ID an operation's api gets: its `operationId` in camelCase, else its method and path.
pub fn id(route: &Route<'_>) -> String {
    let named = route.op.operation_id.as_deref().map(camel).filter(|id| !id.is_empty());
    named.unwrap_or_else(|| from_route(route.method.name(), &route.path))
}

/// `/users/{user_id}` as `/users/${var.userId}`.
fn templated(path: &str) -> String {
    let mut out = String::new();
    let mut rest = path;
    while let Some((before, after)) = rest.split_once('{') {
        let Some((name, tail)) = after.split_once('}') else { break };
        out.push_str(before);
        out.push_str("${var.");
        out.push_str(&camel(name));
        out.push('}');
        rest = tail;
    }
    out + rest
}

fn first_example(media: &Value) -> Option<Value> {
    let examples = media.get("examples").and_then(Value::as_object)?;
    examples.values().find_map(|example| example.get("value")).cloned()
}

/// A server URL with its `{variables}` replaced by their defaults.
fn expand(url: &str, variables: &oas3::Map<String, oas3::spec::ServerVariable>) -> String {
    variables
        .iter()
        .fold(url.to_owned(), |url, (name, variable)| url.replace(&format!("{{{name}}}"), &variable.default))
}

#[cfg(test)]
mod tests;
