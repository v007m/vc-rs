//! `awsSigV4` against the mock's `/aws/…`, which checks each signature server side as AWS does
//! (D457): a hashed body and a session token for `execute-api`, `UNSIGNED-PAYLOAD` and a path
//! with reserved characters, sent in S3's encoding, for `s3`, and a wrong secret refused with neither key shown anywhere.

use vc_mock_server::{AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY, MockHandle, MockServer};

use crate::redaction::{assert_hidden, files, history_text};
use crate::{Project, said};

const SESSION_TOKEN: &str = "Zq7Lp2Vx9Rk4Tw8MhC5n";

/// `orders#create` (a JSON body, a query and a header) and `reports#put` (an S3 upload), each
/// expecting the mock to have checked its signature.
const APIS: &str = concat!(
    "baseUrl: ${var.baseUrl}/aws\nauth: ordersIam\napis:\n",
    "  - id: create\n    method: POST\n    path: /orders/o-1\n    query: {b: '2', a: 'one two'}\n",
    "    headers: {x-trace: '  t1   t2 '}\n    body: {item: book}\n",
    "    expect:\n      status: 200\n      body: {match: {service: execute-api, region: eu-west-1}}\n",
    "  - id: put\n    method: PUT\n    path: /reports/daily+v1=a.csv\n    auth: reportsBucket\n    body: 'a,b'\n",
    "    expect:\n      status: 200\n      body: {match: {service: s3, region: us-east-1}}\n",
);

const TESTS: &str =
    "tests:\n  - id: createsAnOrder\n    call: orders#create\n  - id: putsAReport\n    call: orders#put\n";

fn project() -> Project {
    let config = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\nvars: {baseUrl: 'http://x'}\n\
                  history: {bodies: all}\nauth:\n\
                  \x20 - id: ordersIam\n    type: awsSigV4\n    region: eu-west-1\n    service: execute-api\n\
                  \x20   accessKeyId: ${secret.AWS_ACCESS_KEY_ID}\n    secretAccessKey: ${secret.AWS_SECRET_ACCESS_KEY}\n\
                  \x20   sessionToken: ${secret.AWS_SESSION_TOKEN}\n\
                  \x20 - id: reportsBucket\n    type: awsSigV4\n    region: us-east-1\n    service: s3\n\
                  \x20   accessKeyId: ${secret.AWS_ACCESS_KEY_ID}\n    secretAccessKey: ${secret.AWS_SECRET_ACCESS_KEY}\n\
                  \x20   payload: unsigned\n";
    let schema = "AWS_ACCESS_KEY_ID=\n# @sensitive\nAWS_SECRET_ACCESS_KEY=\n# @sensitive\nAWS_SESSION_TOKEN=\n";
    Project::new(&[
        ("vibranium.config.yaml", config),
        ("services/orders/orders.api.yaml", APIS),
        ("services/orders/signed.test.yaml", TESTS),
        (".env.schema", schema),
    ])
}

/// `vc run` against `server` with the secret access key `secret`.
fn run(project: &Project, server: &MockHandle, secret: &str) -> std::process::Output {
    let mut vc = project.run(&server.base_url(), &[]);
    vc.env("AWS_ACCESS_KEY_ID", AWS_ACCESS_KEY_ID).env("AWS_SECRET_ACCESS_KEY", secret);
    vc.env("AWS_SESSION_TOKEN", SESSION_TOKEN).output().unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_mock_accepts_what_aws_sigv4_signs() {
    let server = MockServer::start().await.unwrap();
    let project = project();
    let run = run(&project, &server, AWS_SECRET_ACCESS_KEY);
    assert_eq!(run.status.code(), Some(0), "{}", said(&run));
    let history = history_text(&project.path().join(".vibranium/history.db"));
    assert!(history.contains("x-amz-security-token"), "the session token was signed");
    let mut seen = vec![("history.db rows".to_owned(), history), ("vc output".to_owned(), said(&run))];
    files(project.path().join(".vibranium").as_path(), &mut seen);
    assert_hidden(&seen, &[AWS_SECRET_ACCESS_KEY, SESSION_TOKEN, AWS_ACCESS_KEY_ID]);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wrong_secret_is_refused_and_shown_nowhere() {
    let server = MockServer::start().await.unwrap();
    let project = project();
    let wrong = "Nf3Hd8Qs1Bm6Yc0Xe5Ju";
    let run = run(&project, &server, wrong);
    assert_eq!(run.status.code(), Some(1), "{}", said(&run));
    let history = history_text(&project.path().join(".vibranium/history.db"));
    assert!(history.contains("SignatureDoesNotMatch"), "the mock refused the signature");
    let mut seen = vec![("history.db rows".to_owned(), history), ("vc output".to_owned(), said(&run))];
    files(project.path().join(".vibranium").as_path(), &mut seen);
    assert_hidden(&seen, &[wrong, SESSION_TOKEN, AWS_ACCESS_KEY_ID]);
}
