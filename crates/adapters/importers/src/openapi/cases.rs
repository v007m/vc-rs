//! The tests `--tests`, `--negative` and `--auth` add: one call per api and the status it
//! should get.

use indexmap::IndexMap;
use serde_json::Value;
use vc_interfaces::{DraftApi, DraftTest};

use super::draft::{Reader, Route};
use crate::naming::{capitalised, third_person};

impl Reader<'_> {
    /// The tests for `apis`, made from `routes` in the same order.
    pub fn tests(&self, file_id: &str, routes: &[Route<'_>], apis: &[DraftApi]) -> Vec<DraftTest> {
        let mut tests = Vec::new();
        for (route, api) in routes.iter().zip(apis) {
            let call = format!("{file_id}#{}", api.id);
            let case = |id: String, name: String, status: u16| DraftTest {
                id,
                name,
                call: call.clone(),
                status,
                ..DraftTest::default()
            };
            let label = api.name.clone().unwrap_or_else(|| api.id.clone());
            if self.opts.tests {
                let status = success(route);
                tests.push(case(third_person(&api.id), format!("{label} returns {status}"), status));
            }
            if let Some(field) = self.opts.negative.then(|| first_body_var(api)).flatten() {
                let id = format!("rejects{}Without{}", capitalised(&api.id), capitalised(&field));
                let mut test = case(id, format!("{label} without {field} returns 400"), 400);
                test.vars = IndexMap::from([(field, Value::Null)]);
                tests.push(test);
            }
            if self.opts.auth && self.secured(route) {
                let id = format!("rejects{}WithoutAuth", capitalised(&api.id));
                let mut test = case(id, format!("{label} without auth returns 401"), 401);
                test.no_auth = true;
                tests.push(test);
            }
        }
        tests
    }

    /// Whether the operation needs credentials: its own `security`, else the spec's, names a
    /// scheme. `security: []` on the operation opts it out.
    fn secured(&self, route: &Route<'_>) -> bool {
        let escaped = route.path.replace('~', "~0").replace('/', "~1");
        let own = format!("/paths/{escaped}/{}/security", route.method.name().to_ascii_lowercase());
        let security = self.doc.pointer(&own).or_else(|| self.doc.get("security"));
        security.and_then(Value::as_array).is_some_and(|requirements| !requirements.is_empty())
    }
}

/// The first 2xx status the operation lists; 200 when it lists none.
fn success(route: &Route<'_>) -> u16 {
    let codes = route.op.responses.iter().flat_map(|responses| responses.keys());
    let first = codes.filter(|code| code.starts_with('2')).map(|code| code.parse().unwrap_or(200)).next();
    first.unwrap_or(200)
}

/// The first body field the api sends from a var, as `--negative` leaves it out.
fn first_body_var(api: &DraftApi) -> Option<String> {
    let body = api.body.as_ref()?.as_object()?;
    body.values()
        .filter_map(Value::as_str)
        .find_map(|text| text.strip_prefix("${var.")?.strip_suffix('}').map(str::to_owned))
}
