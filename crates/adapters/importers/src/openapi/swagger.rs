//! Swagger 2.0 to the `OpenAPI` 3.0 layout: servers, components, request bodies and response
//! content. The 3.0 schema keywords left over are then upgraded with the 3.0 documents.

use serde_json::{Map, Value, json};

/// The keys of a 2.0 parameter that belong in its 3.0 `schema`.
const SCHEMA_KEYS: [&str; 16] = [
    "type",
    "format",
    "items",
    "enum",
    "default",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "minLength",
    "maxLength",
    "pattern",
    "minItems",
    "maxItems",
    "uniqueItems",
    "multipleOf",
];

/// The 2.0 sections `$ref`s point into, and the 3.0 components that replace them.
const SECTIONS: [(&str, &str); 4] = [
    ("definitions", "schemas"),
    ("parameters", "parameters"),
    ("responses", "responses"),
    ("securityDefinitions", "securitySchemes"),
];

/// The operation keys of a path item.
const METHODS: [&str; 7] = ["get", "put", "post", "delete", "options", "head", "patch"];

/// `doc`, a Swagger 2.0 document, in the `OpenAPI` 3.0 layout.
pub fn to_openapi3(mut doc: Map<String, Value>) -> Map<String, Value> {
    let consumes = media_types(doc.remove("consumes").as_ref());
    let produces = media_types(doc.remove("produces").as_ref());
    let shared = doc.get("parameters").and_then(Value::as_object).cloned().unwrap_or_default();
    let wire = Wire { consumes, produces, shared };
    let mut out = Map::new();
    out.insert("openapi".to_owned(), json!("3.0.3"));
    out.insert("servers".to_owned(), servers(&mut doc));
    out.insert("components".to_owned(), wire.components(&mut doc).into());
    if let Some(Value::Object(paths)) = doc.remove("paths") {
        let paths = paths.into_iter().map(|(path, item)| (path, wire.path_item(item))).collect::<Map<_, _>>();
        out.insert("paths".to_owned(), paths.into());
    }
    doc.remove("swagger");
    out.extend(doc);
    out.values_mut().for_each(rewrite_refs);
    out
}

fn media_types(value: Option<&Value>) -> Vec<String> {
    let types: Vec<String> =
        value.and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).map(str::to_owned).collect();
    if types.is_empty() { vec!["application/json".to_owned()] } else { types }
}

fn servers(doc: &mut Map<String, Value>) -> Value {
    let host = doc.remove("host").and_then(|host| host.as_str().map(str::to_owned));
    let base = doc.remove("basePath").and_then(|base| base.as_str().map(str::to_owned)).unwrap_or_default();
    let schemes = doc.remove("schemes");
    let scheme = schemes.as_ref().and_then(Value::as_array).and_then(|all| all.first()).and_then(Value::as_str);
    let url = match host {
        Some(host) => format!("{}://{host}{base}", scheme.unwrap_or("https")),
        None if base.is_empty() => "/".to_owned(),
        None => base,
    };
    json!([{ "url": url }])
}

/// The entries of section `key`, converted, without those `convert` drops.
fn section(
    doc: &mut Map<String, Value>,
    key: &str,
    convert: impl Fn(Value) -> Option<Value>,
) -> Option<Map<String, Value>> {
    let Some(Value::Object(section)) = doc.remove(key) else { return None };
    Some(section.into_iter().filter_map(|(name, value)| convert(value).map(|value| (name, value))).collect())
}

fn is_body(param: &Value) -> bool {
    matches!(param.get("in").and_then(Value::as_str), Some("body" | "formData"))
}

/// A 2.0 non-body parameter or header: its schema keys move into `schema`.
fn parameter(value: Value) -> Value {
    let Value::Object(mut param) = value else { return value };
    let mut schema = Map::new();
    for key in SCHEMA_KEYS {
        if let Some(value) = param.remove(key) {
            schema.insert(key.to_owned(), value);
        }
    }
    if schema.get("type").and_then(Value::as_str) == Some("file") {
        schema.insert("type".to_owned(), json!("string"));
        schema.insert("format".to_owned(), json!("binary"));
    }
    if let Some(format) = param.remove("collectionFormat") {
        param.insert("explode".to_owned(), json!(format == "multi"));
    }
    if !schema.is_empty() {
        param.insert("schema".to_owned(), schema.into());
    }
    param.into()
}

fn security_scheme(value: Value) -> Value {
    let Value::Object(mut scheme) = value else { return value };
    match scheme.get("type").and_then(Value::as_str) {
        Some("basic") => json!({ "type": "http", "scheme": "basic" }),
        Some("oauth2") => {
            let flow = match scheme.remove("flow").as_ref().and_then(Value::as_str) {
                Some("implicit") => "implicit",
                Some("password") => "password",
                Some("application") => "clientCredentials",
                _ => "authorizationCode",
            };
            let mut flows = Map::new();
            for key in ["authorizationUrl", "tokenUrl", "scopes"] {
                if let Some(value) = scheme.remove(key) {
                    flows.insert(key.to_owned(), value);
                }
            }
            scheme.insert("flows".to_owned(), json!({ flow: flows }));
            scheme.into()
        }
        _ => scheme.into(),
    }
}

/// What a 2.0 operation sends and receives by default, and the shared parameters it may name.
struct Wire {
    consumes: Vec<String>,
    produces: Vec<String>,
    shared: Map<String, Value>,
}

impl Wire {
    fn components(&self, doc: &mut Map<String, Value>) -> Map<String, Value> {
        let mut components = Map::new();
        let mut put = |name: &str, section: Option<Map<String, Value>>| {
            if let Some(section) = section {
                components.insert(name.to_owned(), section.into());
            }
        };
        put("schemas", section(doc, "definitions", Some));
        put("parameters", section(doc, "parameters", |param| (!is_body(&param)).then(|| parameter(param))));
        put("responses", section(doc, "responses", |res| Some(response(res, &self.produces))));
        put("securitySchemes", section(doc, "securityDefinitions", |scheme| Some(security_scheme(scheme))));
        components
    }

    fn path_item(&self, item: Value) -> Value {
        let Value::Object(item) = item else { return item };
        item.into_iter()
            .map(|(key, value)| {
                let value = match key.as_str() {
                    "parameters" => Value::Array(self.inline(value).into_iter().map(parameter).collect()),
                    method if METHODS.contains(&method) => self.operation(value),
                    _ => value,
                };
                (key, value)
            })
            .collect::<Map<_, _>>()
            .into()
    }

    fn operation(&self, value: Value) -> Value {
        let Value::Object(mut op) = value else { return value };
        let consumes = op.remove("consumes").map_or_else(|| self.consumes.clone(), |types| media_types(Some(&types)));
        let produces = op.remove("produces").map_or_else(|| self.produces.clone(), |types| media_types(Some(&types)));
        let params = self.inline(op.remove("parameters").unwrap_or_default());
        let (body, rest): (Vec<Value>, Vec<Value>) = params.into_iter().partition(is_body);
        if !rest.is_empty() {
            op.insert("parameters".to_owned(), rest.into_iter().map(parameter).collect());
        }
        if let Some(request) = request_body(&body, &consumes) {
            op.insert("requestBody".to_owned(), request);
        }
        if let Some(Value::Object(responses)) = op.remove("responses") {
            let responses =
                responses.into_iter().map(|(code, res)| (code, response(res, &produces))).collect::<Map<_, _>>();
            op.insert("responses".to_owned(), responses.into());
        }
        op.into()
    }

    /// The parameters, with references to shared body parameters replaced by the parameter,
    /// since 3.0 has no body parameters to point to.
    fn inline(&self, params: Value) -> Vec<Value> {
        let Value::Array(params) = params else { return Vec::new() };
        params
            .into_iter()
            .map(|param| {
                let name = param.get("$ref").and_then(Value::as_str).and_then(|at| at.strip_prefix("#/parameters/"));
                match name.and_then(|name| self.shared.get(name)) {
                    Some(shared) if is_body(shared) => shared.clone(),
                    _ => param,
                }
            })
            .collect()
    }
}

fn request_body(params: &[Value], consumes: &[String]) -> Option<Value> {
    if let Some(body) = params.iter().find(|param| param.get("in").and_then(Value::as_str) == Some("body")) {
        let schema = body.get("schema").cloned().unwrap_or_else(|| json!({}));
        let content: Map<String, Value> = consumes.iter().map(|ct| (ct.clone(), json!({ "schema": schema }))).collect();
        let required = body.get("required").cloned().unwrap_or(Value::Bool(false));
        let mut request = json!({ "content": content, "required": required });
        if let (Some(description), Value::Object(request)) = (body.get("description"), &mut request) {
            request.insert("description".to_owned(), description.clone());
        }
        return Some(request);
    }
    form_body(params, consumes)
}

fn form_body(params: &[Value], consumes: &[String]) -> Option<Value> {
    if params.is_empty() {
        return None;
    }
    let mut properties = Map::new();
    let mut required = Vec::new();
    for param in params {
        let Some(name) = param.get("name").and_then(Value::as_str) else { continue };
        if param.get("required") == Some(&Value::Bool(true)) {
            required.push(json!(name));
        }
        let schema = parameter(param.clone()).get("schema").cloned().unwrap_or_else(|| json!({}));
        properties.insert(name.to_owned(), schema);
    }
    let multipart = consumes.iter().any(|ct| ct == "multipart/form-data")
        || properties.values().any(|schema| schema.get("format") == Some(&json!("binary")));
    let ct = if multipart { "multipart/form-data" } else { "application/x-www-form-urlencoded" };
    let schema = json!({ "type": "object", "properties": properties, "required": required });
    Some(json!({ "content": { ct: { "schema": schema } } }))
}

fn response(value: Value, produces: &[String]) -> Value {
    let Value::Object(mut res) = value else { return value };
    if let Some(Value::Object(headers)) = res.remove("headers") {
        res.insert(
            "headers".to_owned(),
            headers.into_iter().map(|(k, v)| (k, parameter(v))).collect::<Map<_, _>>().into(),
        );
    }
    let examples = res.remove("examples");
    if let Some(schema) = res.remove("schema") {
        let content: Map<String, Value> = produces
            .iter()
            .map(|ct| {
                let mut media = json!({ "schema": schema });
                if let (Some(example), Value::Object(media)) =
                    (examples.as_ref().and_then(|all| all.get(ct)), &mut media)
                {
                    media.insert("example".to_owned(), example.clone());
                }
                (ct.clone(), media)
            })
            .collect();
        res.insert("content".to_owned(), content.into());
    }
    res.into()
}

/// Points every `$ref` into a 2.0 section at the 3.0 component that replaced it.
fn rewrite_refs(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(at)) = map.get_mut("$ref") {
                for (old, new) in SECTIONS {
                    if let Some(rest) = at.strip_prefix(&format!("#/{old}/")) {
                        *at = format!("#/components/{new}/{rest}");
                        break;
                    }
                }
            }
            map.values_mut().for_each(rewrite_refs);
        }
        Value::Array(items) => items.iter_mut().for_each(rewrite_refs),
        _ => {}
    }
}

#[cfg(test)]
mod tests;
