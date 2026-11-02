//! One HTTP call for one test: render, authorize, hook, send, extract.
use ir::{Extract, FieldName, RequestSpec, Test, UrlPath, Vars};
use ports::{HttpRequest, HttpResponse, PortError};
use serde_json::Value;

use crate::Ports;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CallError {
    #[error(transparent)]
    Port(#[from] PortError),
    #[error("response has no field '{0}' to extract")]
    MissingField(FieldName),
}

pub struct CallOutcome {
    pub response: HttpResponse,
    pub vars: Vars,
}

pub async fn execute_call(ports: &Ports, test: &Test, vars: &Vars) -> Result<CallOutcome, CallError> {
    let request = ports.auth.authorize(render(&test.spec.request, vars)).await?;
    let request = match &test.spec.before_request {
        Some(hook) => ports.hooks.before_request(hook, request).await?,
        None => request,
    };
    let response = ports.transport.send(request).await?;
    let vars = extract(&test.spec.extract, &response.body, vars)?;
    Ok(CallOutcome { response, vars })
}

fn render(spec: &RequestSpec, vars: &Vars) -> HttpRequest {
    HttpRequest {
        method: spec.method,
        path: UrlPath::new(spec.path.render(vars)),
        headers: spec.headers.iter().map(|(name, value)| (name.clone(), value.render(vars))).collect(),
        body: spec.body.clone(),
    }
}

fn extract(rules: &[Extract], body: &Value, vars: &Vars) -> Result<Vars, CallError> {
    let mut out = vars.clone();
    for rule in rules {
        let value = body.get(rule.field.as_str()).ok_or_else(|| CallError::MissingField(rule.field.clone()))?;
        out.insert(rule.var.clone(), value.as_str().map_or_else(|| value.to_string(), str::to_owned));
    }
    Ok(out)
}
