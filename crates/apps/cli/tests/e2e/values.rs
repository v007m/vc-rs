//! Computed bodies: `$extend` merges changes into an earlier response (and fails on a base
//! that is no object), `$remove` drops fields,
//! and `$cel` computes a value, all as the server receives them. A `forEach` over no items saves
//! each name as an empty list. A dataset outside the project
//! is read from a `datasets.allowRoots` folder, and from nowhere else. A templated status is read
//! as a written one, from a number, text, a class or a list.

use vc_mock_server::MockServer;

use crate::{CONFIG, Project, SCRIPTED};

#[tokio::test(flavor = "multi_thread")]
async fn an_extended_body_keeps_the_original_with_the_changes_and_cel_computes_values() {
    let server = MockServer::start().await.unwrap();
    let test = concat!(
        "tests:\n  - id: resendsChangedOrder\n    vars:\n      price: 21\n      total: 150.5\n    steps:\n",
        "      - id: original\n        call: scripted#echo\n",
        "        body: {id: 7, items: [a, b], shipping: {city: Oslo, zip: '0150', geo: {lat: 1}}}\n",
        "      - call: scripted#echo\n        body:\n          $extend: ${original.body.body}\n",
        "          shipping:\n            city: Bergen\n          doubled:\n            $cel: var.price * 2\n",
        "          tier:\n            $cel: \"var.total > 100.0 ? 'gold' : 'standard'\"\n",
        "          $remove: [id, shipping.geo]\n",
        "        expect:\n          body:\n            match:\n",
        "              body.items: [a, b]\n              body.shipping: {city: Bergen, zip: '0150'}\n",
        "              body.doubled: 42\n              body.tier: gold\n",
        "          cel:\n            dropped: \"!has(response.body.body.id) && !has(response.body.body.shipping.geo)\"\n",
    );
    let project = Project::new(&[CONFIG, SCRIPTED, ("services/scripted/values.test.yaml", test)]);
    let output = project.run(&server.base_url(), &[]).assert().get_output().clone();
    assert_eq!(output.status.code(), Some(0), "{}", crate::said(&output));
    assert_eq!(server.stats().await.unwrap().count("POST /echo"), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_extend_whose_base_renders_no_object_fails_the_call_and_sends_nothing() {
    let server = MockServer::start().await.unwrap();
    let test = concat!(
        "tests:\n  - id: extendsNumber\n    vars:\n      n: 5\n    call: scripted#echo\n",
        "    body:\n      $extend: ${var.n}\n      a: 1\n",
    );
    let project = Project::new(&[CONFIG, SCRIPTED, ("services/scripted/values.test.yaml", test)]);
    let output = project.run(&server.base_url(), &[]).output().unwrap();
    let said = crate::said(&output);
    assert_eq!(output.status.code(), Some(1), "{said}");
    assert!(
        said.contains("V5111") && said.contains("`$extend` needs an object to start from, but its base is a number"),
        "{said}"
    );
    assert_eq!(server.stats().await.unwrap().count("POST /echo"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_for_each_over_an_empty_list_saves_each_name_as_an_empty_list() {
    let server = MockServer::start().await.unwrap();
    let test = concat!(
        "tests:\n  - id: sendsWhatNoTurnSaved\n    vars:\n      none: []\n    steps:\n",
        "      - call: scripted#echo\n        forEach: ${var.none}\n        save:\n          got: ${response.status}\n",
        "      - call: scripted#echo\n        body: {got: ${var.got}}\n",
        "        expect:\n          cel:\n            empty: response.body.body.got == []\n",
    );
    let project = Project::new(&[CONFIG, SCRIPTED, ("services/scripted/values.test.yaml", test)]);
    let output = project.run(&server.base_url(), &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", crate::said(&output));
    assert_eq!(server.stats().await.unwrap().count("POST /echo"), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_dataset_outside_the_project_is_read_only_under_an_allowed_root() {
    let server = MockServer::start().await.unwrap();
    let test = concat!(
        "tests:\n  - id: sendsEachCity\n    examples: ${dataset.cities}\n    call: scripted#echo\n",
        "    body: {city: '${example.city}'}\n    expect:\n      body: {match: {body.city: '${example.city}'}}\n",
    );
    let project = Project::new(&[SCRIPTED, ("services/scripted/cities.test.yaml", test)]);
    let shared = project.home().join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    std::fs::write(shared.join("cities.csv"), "city\nOslo\nBergen\n").unwrap();
    let home = project.home().file_name().unwrap().to_string_lossy().into_owned();
    let config = |root: &str| {
        format!(
            "{}datasets:\n  allowRoots: [{root}]\n  files:\n    - id: cities\n      file: ../{home}/shared/cities.csv\n",
            CONFIG.1
        )
    };
    project.write("vibranium.config.yaml", &config(&format!("../{home}/shared")));
    let output = project.run(&server.base_url(), &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", crate::said(&output));
    assert_eq!(server.stats().await.unwrap().count("POST /echo"), 2);
    project.write("vibranium.config.yaml", &config(&format!("../{home}/elsewhere")));
    std::fs::create_dir_all(project.home().join("elsewhere")).unwrap();
    let output = project.vc(&["check"]).output().unwrap();
    let said = crate::said(&output);
    assert!(said.contains("V1012") && said.contains("cities.csv"), "{said}");
}

/// `coded#answer` answers with the status its `code` var names.
const CODED: (&str, &str) = (
    "services/coded/coded.api.yaml",
    "baseUrl: ${var.baseUrl}\napis:\n  - id: answer\n    path: /status/${var.code}\n    vars: {code: 200}\n",
);

#[tokio::test(flavor = "multi_thread")]
async fn a_templated_status_takes_each_rows_code() {
    let server = MockServer::start().await.unwrap();
    let test = |rows: &str| {
        format!(
            "tests:\n  - id: answersWithTheRowsStatus\n    examples:\n{rows}    call: coded#answer\n    vars: {{code: '${{example.code}}'}}\n    expect:\n      status: ${{example.status}}\n"
        )
    };
    let rows = "      - {code: 415, status: 415}\n      - {code: 201, status: 201}\n";
    let project = Project::new(&[CONFIG, CODED, ("services/coded/rows.test.yaml", &test(rows))]);
    let output = project.run(&server.base_url(), &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", crate::said(&output));
    project.write("services/coded/rows.test.yaml", &test("      - {code: 200, status: 415}\n"));
    let output = project.run(&server.base_url(), &[]).output().unwrap();
    let said = crate::said(&output);
    assert_eq!(output.status.code(), Some(1), "{said}");
    assert!(said.contains("expect.status") && said.contains("415"), "{said}");
}

/// A CSV row's status is text (`"201"`), and a row may hold a class or a list: each is read as
/// a written status would be. A value that is no status fails the check and says so.
#[tokio::test(flavor = "multi_thread")]
async fn a_templated_status_reads_text_classes_and_lists_from_rows() {
    let server = MockServer::start().await.unwrap();
    let test = |rows: &str| {
        format!(
            "tests:\n  - id: answersWithTheRowsStatus\n    examples: {rows}\n    call: coded#answer\n    vars: {{code: '${{example.code}}'}}\n    expect:\n      status: ${{example.status}}\n"
        )
    };
    let config = format!("{}datasets:\n  files:\n    - id: codes\n      file: data/codes.csv\n", CONFIG.1);
    let csv = "code,status\n201,201\n404,4xx\n";
    let rows = test("${dataset.codes}");
    let files = [(CONFIG.0, config.as_str()), CODED, ("data/codes.csv", csv), ("services/coded/rows.test.yaml", &rows)];
    let project = Project::new(&files);
    let output = project.run(&server.base_url(), &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "text and class rows pass: {}", crate::said(&output));
    project.write("services/coded/rows.test.yaml", &test("[{code: 404, status: [200, 4xx]}]"));
    let output = project.run(&server.base_url(), &[]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "a list row passes: {}", crate::said(&output));
    project
        .write("services/coded/rows.test.yaml", &test("[{code: 200, status: [201, 4xx]}, {code: 200, status: nope}]"));
    let output = project.run(&server.base_url(), &[]).output().unwrap();
    let said = crate::said(&output);
    assert_eq!(output.status.code(), Some(1), "{said}");
    assert!(said.contains("expect.status") && said.contains("is not a code from 100 to 599"), "{said}");
}
