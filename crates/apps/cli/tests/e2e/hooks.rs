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
    // QuickJS stops a loop at its back-edge, where a frame's line is its first, unless a native
    // call such as `concat` is running; nearly all of this loop's time is inside `concat`.
    let spin = "export function spin(): boolean {\n  let n: number = 0;\n  while (true) {\n    n = [n].concat([1]).length;\n  }\n}\n";
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
    for limit in ["time", "memory"] {
        let stopped = format!("{limit} budget and was stopped (line ");
        let line = said.split(stopped.as_str()).nth(1).and_then(|rest| rest.split(')').next());
        assert!(matches!(line, Some("3" | "4")), "the loop's TypeScript line for {limit}: {said}");
    }
}
