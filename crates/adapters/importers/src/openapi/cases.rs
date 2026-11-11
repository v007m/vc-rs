//! The tests `--tests`, `--negative` and `--auth` add: per api, a call with what it needs
//! created first and the response it should get, the bodies its schema rejects, and a call
//! with no auth.

use serde_json::{Value, json};
use vc_interfaces::{DraftApi, DraftBefore, DraftTest};
use vc_model::Method;

use super::creators::{Link, Needs, provide};
use super::draft::{Reader, Route};
use super::responses::{escaped, rejected, success};
use crate::naming::{capitalised, sentence, third_person};

/// The body a negative test sends: its row's.
const ROW_BODY: &str = "${example.body}";

/// One api, as its tests are drafted.
struct Case<'c> {
    file_id: &'c str,
    route: &'c Route<'c>,
    api: &'c DraftApi,
    needs: &'c Needs,
}

impl Reader<'_> {
    /// The tests for `apis`, made from `routes` in the same order. An api that creates what a
    /// test needs first gets the `save` the test reads and the `cleanup` that removes it.
    pub fn tests(&self, file_id: &str, routes: &[Route<'_>], apis: &mut [DraftApi]) -> Vec<DraftTest> {
        let needs: Vec<Needs> = routes.iter().map(|route| self.needs(routes, route)).collect();
        let mut tests = Vec::new();
        let mut used: Vec<&Link> = Vec::new();
        for ((route, api), needs) in routes.iter().zip(apis.iter()).zip(&needs) {
            let drafted = self.cases(&Case { file_id, route, api, needs });
            if let Needs::Made(links) = needs
                && drafted.iter().any(|test| !test.before.is_empty())
            {
                used.extend(links);
            }
            tests.extend(drafted);
        }
        for link in used {
            provide(apis, routes, link);
        }
        tests
    }

    fn cases(&self, case: &Case<'_>) -> Vec<DraftTest> {
        let mut tests = Vec::new();
        if self.opts.tests {
            tests.push(self.happy(case));
        }
        if self.opts.negative {
            tests.extend(self.negative(case));
        }
        if self.opts.auth && self.secured(case.route) {
            tests.push(case.no_auth());
        }
        tests
    }

    /// The call as the spec describes its success: the first 2xx, and the body's schema.
    fn happy(&self, case: &Case<'_>) -> DraftTest {
        let schema = self.success_schema(case.route).map(|(pointer, _)| pointer);
        DraftTest {
            id: third_person(&case.api.id),
            name: case.label(),
            status: success(case.route),
            schema,
            ..case.needing()
        }
    }

    /// One instance per row of bodies the schema rejects; none for an api without a JSON body.
    fn negative(&self, case: &Case<'_>) -> Option<DraftTest> {
        let examples = self.invalid_bodies(case.route, case.api);
        (!examples.is_empty()).then(|| DraftTest {
            id: format!("rejectsInvalid{}", capitalised(&case.api.id)),
            name: format!("{}: rejects an invalid body", case.label()),
            body: Some(json!(ROW_BODY)),
            examples,
            status: rejected(case.route),
            ..case.needing()
        })
    }

    /// Whether the operation needs credentials: its own `security`, else the spec's, names a
    /// scheme. `security: []` on the operation opts it out.
    fn secured(&self, route: &Route<'_>) -> bool {
        let own = format!("/paths/{}/{}/security", escaped(&route.path), route.method.name().to_ascii_lowercase());
        let security = self.doc.pointer(&own).or_else(|| self.doc.get("security"));
        security.and_then(Value::as_array).is_some_and(|requirements| !requirements.is_empty())
    }
}

impl Case<'_> {
    /// What the tests are called after: the operation's summary without its full stop, else
    /// the api's ID as a sentence.
    fn label(&self) -> String {
        let summary = self.api.name.as_deref().map(|name| name.trim_end_matches('.')).filter(|name| !name.is_empty());
        summary.map_or_else(|| sentence(&self.api.id), str::to_owned)
    }

    /// The call with no auth, which a secured api refuses.
    fn no_auth(&self) -> DraftTest {
        DraftTest {
            id: format!("rejects{}WithoutAuth", capitalised(&self.api.id)),
            name: format!("{}: needs a signed-in caller", self.label()),
            call: self.call(),
            no_auth: true,
            status: 401,
            ..DraftTest::default()
        }
    }

    /// The call with what it needs created first, or skipped with why it cannot be. A
    /// `DELETE` keeps the item it removes, so no cleanup removes it again.
    fn needing(&self) -> DraftTest {
        let mut test = DraftTest { call: self.call(), ..DraftTest::default() };
        match self.needs {
            Needs::Missing(reason) => test.skip = Some(reason.clone()),
            Needs::Made(links) => {
                let call = |id: &str| format!("{}#{id}", self.file_id);
                test.before = links.iter().map(|link| DraftBefore { call: call(&link.id), keep: false }).collect();
                if self.route.method == Method::Delete
                    && let Some(last) = test.before.last_mut()
                {
                    last.keep = true;
                }
            }
        }
        test
    }

    fn call(&self) -> String {
        format!("{}#{}", self.file_id, self.api.id)
    }
}
