//! The tests `vc import openapi --tests` writes for the mock users spec create the user each
//! call needs first and remove it afterwards, and check every body against its schema in the
//! snapshot: `vc check` finds nothing to warn about and every test passes.

use vc_mock_server::MockServer;

use crate::{CONFIG, Project, said};

#[tokio::test(flavor = "multi_thread")]
async fn generated_tests_check_clean_and_pass_against_the_spec_s_server() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG]);
    let spec = format!("{}/openapi/users.yaml", server.base_url());
    let output = project.vc(&["import", "openapi", &spec, "--id", "users", "--tests"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let tests = std::fs::read_to_string(project.path().join("services/users/openapi.test.yaml")).unwrap();
    for written in
        ["- call: users#createUser\n        keep: true", "$file: ../../openapi/users.yaml#/components/schemas/User"]
    {
        assert!(tests.contains(written), "{written}: {tests}");
    }
    let check = project.vc(&["check"]).output().unwrap();
    assert_eq!(check.status.code(), Some(0), "{}", said(&check));
    assert!(said(&check).contains("0 errors · 0 warnings"), "{}", said(&check));
    let run = project.vc(&["run"]).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
}

/// `--negative` writes a test the mock fails, as it takes any body; `vc skip` keeps it in the
/// report as skipped, and the rest pass. The first run is one test at a time: run in parallel,
/// `listsUsers` may also fail, when it lists the users the rows made before their cleanup
/// removes them. The users spec has no security, so `--auth` adds nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_negative_test_the_mock_fails_is_skipped_and_the_rest_pass() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG]);
    let spec = format!("{}/openapi/users.yaml", server.base_url());
    let import = ["import", "openapi", &spec, "--id", "users", "--tests", "--negative", "--auth"];
    let output = project.vc(&import).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let tests = std::fs::read_to_string(project.path().join("services/users/openapi.test.yaml")).unwrap();
    assert!(tests.contains("- id: rejectsInvalidCreateUser\n") && !tests.contains("WithoutAuth"), "{tests}");
    assert_eq!(
        tests.matches("      - call: users#createUser\n").count(),
        2,
        "getsUser and deletesUser create one: {tests}"
    );
    let run = project.vc(&["run", "--concurrency", "1"]).output().unwrap();
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    let failed = said(&run).matches("✕ users.openapi#rejectsInvalidCreateUser › ").count();
    assert_eq!(failed, 4, "each row is labelled by its name: {}", said(&run));
    for passed in ["createsUser", "getsUser", "deletesUser", "listsUsers"] {
        assert!(said(&run).contains(&format!("✓ users.openapi#{passed}")), "{}", said(&run));
    }
    assert!(said(&run).contains("4 passed · 4 failed"), "{}", said(&run));
    let reason = "the mock accepts any body; re-enable against the real service";
    let skip = project.vc(&["skip", "users.openapi#rejectsInvalidCreateUser", "--reason", reason]).output().unwrap();
    assert_eq!(skip.status.code(), Some(0), "{}", said(&skip));
    let rerun = project.vc(&["run"]).output().unwrap();
    assert_eq!(rerun.status.code(), Some(0), "{}", said(&rerun));
    let skipped =
        format!("- users.openapi#rejectsInvalidCreateUser — Create user: rejects an invalid body skipped: {reason}");
    assert!(said(&rerun).contains(&skipped), "{}", said(&rerun));
    assert!(said(&rerun).contains("4 passed · 1 skipped"), "{}", said(&rerun));
}

/// A spec served by the mock, with a secured operation and an item nothing in it creates.
const SECURED: &str = "openapi: 3.0.3
info: {title: Me, version: 1.0.0}
servers:
  - url: BASE
components:
  securitySchemes:
    bearer: {type: http, scheme: bearer}
paths:
  /auth/me:
    get:
      operationId: getMe
      security: [{bearer: []}]
      responses:
        '200': {description: Who the token is}
        '401': {description: No token}
  /orders/{orderId}:
    get:
      operationId: getOrder
      parameters:
        - {name: orderId, in: path, required: true, schema: {type: integer}}
      responses:
        '200': {description: The order}
";

/// `--auth` writes a test per secured operation that the server's 401 passes, and a test of an
/// item no operation creates is skipped, saying how to run it.
#[tokio::test(flavor = "multi_thread")]
async fn an_auth_test_passes_on_the_server_s_401_and_a_test_nothing_can_set_up_is_skipped() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG]);
    project.write("specs/me.yaml", &SECURED.replace("BASE", &server.base_url()));
    let output = project.vc(&["import", "openapi", "specs/me.yaml", "--tests", "--auth"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let tests = std::fs::read_to_string(project.path().join("services/me/openapi.test.yaml")).unwrap();
    let reason = "needs an existing order: set vars.orderId to one and remove this skip";
    for written in ["- id: rejectsGetMeWithoutAuth\n", "auth: none", "status: 401", reason] {
        assert!(tests.contains(written), "{written}: {tests}");
    }
    let run = project.vc(&["run", "me.openapi#rejectsGetMeWithoutAuth", "me.openapi#getsOrder"]).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    assert!(said(&run).contains("✓ me.openapi#rejectsGetMeWithoutAuth"), "{}", said(&run));
    assert!(said(&run).contains(&format!("- me.openapi#getsOrder — Get order skipped: {reason}")), "{}", said(&run));
}

/// A spec of a user's orders, the user under a generic `{id}`, served by the mock.
const NESTED: &str = "openapi: 3.0.3
info: {title: Shop, version: 1.0.0}
servers:
  - url: BASE
components:
  schemas:
    Item:
      type: object
      required: [id]
      properties: {id: {type: integer}}
    New:
      type: object
      required: [note]
      properties: {note: {type: string}}
  parameters:
    id: {name: id, in: path, required: true, schema: {type: integer}}
    orderId: {name: orderId, in: path, required: true, schema: {type: integer}}
paths:
  /users:
    post:
      operationId: createUser
      requestBody: {required: true, content: {application/json: {schema: {$ref: '#/components/schemas/New'}}}}
      responses:
        '201': {description: The user, content: {application/json: {schema: {$ref: '#/components/schemas/Item'}}}}
  /users/{id}/orders:
    parameters: [{$ref: '#/components/parameters/id'}]
    post:
      operationId: createOrder
      requestBody: {required: true, content: {application/json: {schema: {$ref: '#/components/schemas/New'}}}}
      responses:
        '201': {description: The order, content: {application/json: {schema: {$ref: '#/components/schemas/Item'}}}}
  /users/{id}/orders/{orderId}:
    parameters: [{$ref: '#/components/parameters/id'}, {$ref: '#/components/parameters/orderId'}]
    get:
      operationId: getOrder
      responses:
        '200': {description: The order, content: {application/json: {schema: {$ref: '#/components/schemas/Item'}}}}
    delete:
      operationId: deleteOrder
      responses:
        '204': {description: Gone}
";

/// A test of an order creates its user, then the order under that user: the order's creator
/// reads the user's generic `{id}` through the var the test passes.
#[tokio::test(flavor = "multi_thread")]
async fn a_nested_creator_reads_the_outer_item_s_generic_id_and_the_tests_pass() {
    let server = MockServer::start().await.unwrap();
    let project = Project::new(&[CONFIG]);
    project.write("specs/shop.yaml", &NESTED.replace("BASE", &server.base_url()));
    let output = project.vc(&["import", "openapi", "specs/shop.yaml", "--tests"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let tests = std::fs::read_to_string(project.path().join("services/shop/openapi.test.yaml")).unwrap();
    assert!(tests.contains("      - call: shop#createUser\n      - call: shop#createOrder\n"), "{tests}");
    assert!(tests.contains("id: ${var.userId}"), "{tests}");
    let run = project.vc(&["run"]).output().unwrap();
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    for passed in ["createsUser", "createsOrder", "getsOrder", "deletesOrder"] {
        assert!(said(&run).contains(&format!("✓ shop.openapi#{passed}")), "{}", said(&run));
    }
}
