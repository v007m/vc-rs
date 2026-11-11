//! Hooks (D172): a `beforeRequest` hook signs the request, a `function:` step saves a value, a
//! custom check judges the response, and a hook past its time or memory budget is stopped and
//! reported with its TypeScript line.

use vc_mock_server::MockServer;

use crate::{CONFIG, Project};

/// `signed#echo`, signed by `hooks/sign.ts#sign`.
const SIGNED: (&str, &str) = (
    "services/signed/signed.api.yaml",
    concat!(
        "baseUrl: ${var.baseUrl}\nbeforeRequest:\n  - function: ../../hooks/sign.ts#sign\n",
        "apis:\n  - id: echo\n    method: POST\n    path: /echo\n",
    ),
);

const SIGN: (&str, &str) = (
    "hooks/sign.ts",
    concat!(
        "import type { BeforeRequestHook } from \"@vibranium/sdk\";\n",
        "import { hmacSha256Hex } from \"vibranium:std\";\n",
        "import { header } from \"./names\";\n\n",
        "export const sign: BeforeRequestHook = ({ request }) => ({\n",
        "  ...request,\n",
        "  headers: { ...request.headers, [header]: hmacSha256Hex(\"key\", request.path) },\n",
        "});\n",
    ),
);

const NAMES: (&str, &str) = ("hooks/names.ts", "export const header: string = \"x-signature\";\n");

const SEED: (&str, &str) = (
    "hooks/seed.ts",
    "export function seed({ vars }: { vars: { word: string } }) {\n  return { orderId: `o-${vars.word}` };\n}\n",
);

const CHECKS: (&str, &str) = (
    "hooks/checks.ts",
    concat!(
        "type Echo = { body: { body: { orderId: string } } };\n",
        "export const echoed = ({ response }: { response: Echo }) =>\n",
        "  response.body.body.orderId === \"o-x\" || { pass: false, message: \"not echoed\" };\n",
    ),
);

#[tokio::test(flavor = "multi_thread")]
async fn hooks_sign_the_request_save_values_and_judge_the_response() {
    let server = MockServer::start().await.unwrap();
    let test = concat!(
        "tests:\n  - id: sendsSignedOrder\n    steps:\n",
        "      - function: ../../hooks/seed.ts#seed\n        provides: [orderId]\n",
        "      - call: signed#echo\n        body: {orderId: '${var.orderId}'}\n",
        "        expect:\n          custom:\n            - function: ../../hooks/checks.ts#echoed\n",
        "          cel:\n            signed: \"size(response.body.headers['x-signature']) == 64\"\n",
    );
    let files = [CONFIG, SIGNED, SIGN, NAMES, SEED, CHECKS, ("services/signed/flow.test.yaml", test)];
    let output = Project::new(&files).run(&server.base_url(), &[]).assert().get_output().clone();
    assert_eq!(output.status.code(), Some(0), "{}", crate::said(&output));
    assert_eq!(server.stats().await.unwrap().count("POST /echo"), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hook_past_its_time_or_memory_budget_is_stopped_with_its_line() {
    let server = MockServer::start().await.unwrap();
    let spin = "export function spin(): boolean {\n  let n: number = 0;\n  while (true) {\n    n++;\n  }\n}\n";
    let hog = "export function hog(): boolean {\n  const kept: number[][] = [];\n  for (;;) {\n    kept.push(new Array(1 << 16).fill(7));\n  }\n}\n";
    let test = concat!(
        "tests:\n  - id: spins\n    call: signed#echo\n",
        "    expect:\n      custom:\n        - function: ../../hooks/spin.ts#spin\n",
        "  - id: hogs\n    call: signed#echo\n",
        "    expect:\n      custom:\n        - function: ../../hooks/hog.ts#hog\n",
    );
    let files = [
        CONFIG,
        SIGNED,
        SIGN,
        NAMES,
        ("hooks/spin.ts", spin),
        ("hooks/hog.ts", hog),
        ("services/signed/budget.test.yaml", test),
    ];
    let output = Project::new(&files).run(&server.base_url(), &[]).assert().get_output().clone();
    let said = crate::said(&output);
    assert_eq!(output.status.code(), Some(1), "{said}");
    assert!(said.contains("V5116"), "{said}");
    let line = |limit: &str| {
        let stopped = format!("{limit} budget and was stopped (line ");
        said.split(stopped.as_str()).nth(1).and_then(|rest| rest.split(')').next()).map(str::to_owned)
    };
    assert_eq!(line("time").as_deref(), Some("3"), "a loop of plain arithmetic names its own line: {said}");
    assert!(matches!(line("memory").as_deref(), Some("3" | "4")), "the loop's TypeScript line: {said}");
}

const SIGN_KEY: &str = "Wm5c-Ry8d-Kp2t";

#[tokio::test(flavor = "multi_thread")]
async fn hooks_read_the_run_s_secrets_and_never_show_them() {
    let server = MockServer::start().await.unwrap();
    let sign = concat!(
        "import { hmacSha256Hex } from \"vibranium:std\";\n",
        "export const sign = ({ request }) =>\n",
        "  ({ ...request, headers: { ...request.headers, \"x-signature\": hmacSha256Hex(secret(\"SIGN_KEY\"), request.path) } });\n",
        "export const seed = () => ({ keyed: secret(\"SIGN_KEY\").length === 14 ? \"yes\" : \"no\" });\n",
    );
    let test = concat!(
        "tests:\n  - id: signsWithTheKey\n    steps:\n",
        "      - function: ../../hooks/sign.ts#seed\n        provides: [keyed]\n",
        "      - call: signed#echo\n        body: {keyed: '${var.keyed}'}\n",
        "        expect:\n          body: {match: {body.keyed: 'yes'}}\n",
        "          cel:\n            signed: \"size(response.body.headers['x-signature']) == 64\"\n",
    );
    let files = [
        CONFIG,
        SIGNED,
        ("hooks/sign.ts", sign),
        (".env.schema", "# the signing key\nSIGN_KEY=\n"),
        ("services/signed/keyed.test.yaml", test),
    ];
    let project = Project::new(&files);
    let mut vc = project.run(&server.base_url(), &[]);
    let output = vc.env("SIGN_KEY", SIGN_KEY).output().unwrap();
    let said = crate::said(&output);
    assert_eq!(output.status.code(), Some(0), "the hooks read the key: {said}");
    assert!(!said.contains(SIGN_KEY), "{said}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hook_that_calls_console_log_fails_its_call_because_hooks_have_no_console_yet() {
    let server = MockServer::start().await.unwrap();
    let api = concat!(
        "baseUrl: ${var.baseUrl}\n",
        "apis:\n  - id: logged\n    method: GET\n    path: /echo\n",
        "    afterResponse:\n      - function: ../../hooks/log.ts#logStatus\n",
    );
    let log = "export function logStatus({ response }: { response: { status: number } }) {\n  console.log(response.status);\n}\n";
    let test = "tests:\n  - id: logsTheStatus\n    call: logged#logged\n";
    let files = [
        CONFIG,
        ("services/logged/logged.api.yaml", api),
        ("hooks/log.ts", log),
        ("services/logged/flow.test.yaml", test),
    ];
    let output = Project::new(&files).run(&server.base_url(), &[]).assert().get_output().clone();
    let said = crate::said(&output);
    assert_eq!(output.status.code(), Some(1), "{said}");
    assert!(said.contains("V5115"), "{said}");
    assert!(said.contains("console is not defined"), "{said}");
}
