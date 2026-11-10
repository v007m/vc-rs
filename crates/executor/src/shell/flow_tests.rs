use std::sync::Arc;

use vc_adapter_http::{HttpOptions, HttpTransport};
use vc_mock_server::MockServer;
use vc_model::{BodySpec, Method, RunStatus, TemplateValue, Totals};

use super::{Executor, RunOptions};
use crate::fixture::Fixture;
use crate::{ActionExecutors, EventBus, HostLimits, PipelineParts};

#[tokio::test]
async fn a_test_creates_reads_by_the_saved_id_and_cleans_up_against_the_mock_server() {
    let server = MockServer::start().await.unwrap();
    let mut fixture = Fixture::new();
    fixture.group_mut().base_url = Some(fixture.template(&server.base_url()));
    fixture.program.config.vars.values = fixture.vars(&[("name", "Ada")]);
    let delete = fixture.call("/users");
    let path = fixture.template("/users/${var.userId}");
    (fixture.api_mut(delete).method, fixture.api_mut(delete).path) = (Method::Delete, path);
    fixture.action_mut(delete).vars.values = fixture.vars(&[("userId", "${response.body.id}")]);
    let create = fixture.creating("/users", delete);
    let body = TemplateValue::Map([("name".to_owned(), fixture.value("${var.name}"))].into());
    (fixture.api_mut(create).method, fixture.api_mut(create).body) = (Method::Post, BodySpec::Json(body));
    fixture.action_mut(create).save.values = fixture.vars(&[("userId", "${response.body.id}")]);
    let read = fixture.call("/users");
    fixture.api_mut(read).path = fixture.template("/users/${var.userId}");
    let test = fixture.test(vec![create, read], None);
    let plan = Arc::new(fixture.plan(&[], &[(test, 1)]));
    let transport = Arc::new(HttpTransport::new(&HttpOptions::default()).unwrap());
    let executors = ActionExecutors::standard(PipelineParts::new(transport, HostLimits::spawn(8)));
    let executor = Executor::new(fixture.cx(executors), RunOptions::default());
    let summary = executor.start(plan, EventBus::spawn()).unwrap().finish().await.unwrap();
    let totals = Totals { passed: 1, ..Totals::default() };
    assert_eq!((summary.status, summary.totals), (RunStatus::Passed, totals));
    let stats = server.stats().await.unwrap();
    let counts = ["POST /users", "GET /users/{id}", "DELETE /users/{id}"].map(|route| stats.count(route));
    assert_eq!(counts, [1, 1, 1]);
    server.shutdown().await.unwrap();
}
