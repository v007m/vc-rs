use std::io::Read as _;
use std::path::PathBuf;

use flate2::read::GzDecoder;
use serde_json::Value;
use vc_interfaces::Artifact;

use super::*;
use crate::testing::{self, Capture, Saved};
use crate::view::ExchangeView;

/// The scripted run, with `body` as the response of the failed test's exchange, twice.
fn snapshot(body: &str) -> RunSnapshot {
    let mut snapshot = testing::snapshot();
    let exchange = ExchangeView { response_body: Some(body.to_owned()), ..ExchangeView::default() };
    snapshot.tests[1].exchanges = vec![exchange.clone(), exchange];
    snapshot
}

fn rendered(layout: HtmlLayout, snapshot: &RunSnapshot) -> (Vec<Artifact>, Saved) {
    let (capture, captured) = Capture::new();
    let artifacts = HtmlFormat::new(layout).render(snapshot, capture.as_ref()).unwrap();
    (artifacts, captured.take().1)
}

#[test]
fn writes_a_page_and_its_data_folder() {
    let (artifacts, saved) = rendered(HtmlLayout::Folder, &snapshot("{\"x\":1}"));
    assert_eq!(artifacts, [Artifact::new("html", PathBuf::from("out/index.html"))]);
    let names: Vec<&str> = saved.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names[..3], ["data/summary.js", "data/index.js", "data/tests/0.js"]);
    assert_eq!(names.iter().filter(|name| name.starts_with("data/bodies/")).count(), 1, "one body, stored once");
    let (_, page) = saved.last().unwrap();
    assert!(page.contains("<title>Vibranium run 7</title>") && page.contains("VIB.start();"), "{page}");
    assert!(!page.contains("<!--VIB:") && !page.contains("vib-data\">"), "no placeholder or bundle left");
    assert!(saved[0].1.starts_with("VIB.load(\"summary\",{"), "{}", saved[0].1);
}

#[test]
fn never_lets_data_close_its_script() {
    let (_, saved) = rendered(HtmlLayout::Folder, &snapshot("</script><script>alert(1)</script>"));
    let body = saved.iter().find(|(name, _)| name.starts_with("data/bodies/")).unwrap();
    assert!(!body.1.contains('<'), "{}", body.1);
    assert!(body.1.contains("\\u003c/script>"), "{}", body.1);
}

/// The data a single-file page packs into its `vib-data` script.
fn packed_data(page: &str) -> Value {
    let start = page.find("id=\"vib-data\">").unwrap() + "id=\"vib-data\">".len();
    let packed = &page[start..start + page[start..].find('<').unwrap()];
    let bytes = base64::engine::general_purpose::STANDARD.decode(packed).unwrap();
    let mut json = String::new();
    GzDecoder::new(bytes.as_slice()).read_to_string(&mut json).unwrap();
    serde_json::from_str(&json).unwrap()
}

#[test]
fn bundles_everything_into_one_page() {
    let (artifacts, saved) = rendered(HtmlLayout::Single, &snapshot("{\"x\":1}"));
    assert_eq!(artifacts, [Artifact::new("html", PathBuf::from("out/report.html"))]);
    assert_eq!(saved.len(), 1);
    let data = packed_data(&saved[0].1);
    assert_eq!(data["summary"]["run"]["id"], 7);
    assert_eq!(data["index"].as_array().unwrap().len(), 6);
    assert_eq!(data["index"][2][3], "blocked");
    let test = &data["tests/0"]["1"];
    let hash = test["exchanges"][0]["responseBody"].as_str().unwrap();
    assert_eq!(data[format!("bodies/{hash}")], "{\"x\":1}");
    assert_eq!(test["checks"][1]["label"], "body.match.errors[0].field");
    assert_eq!(data["summary"]["lifecycle"][1]["label"], "cleanup seed");
}

#[test]
fn shows_each_file_profile_and_test_name_beside_its_id() {
    let (_, saved) = rendered(HtmlLayout::Single, &snapshot("{\"x\":1}"));
    let data = packed_data(&saved[0].1);
    assert_eq!(data["summary"]["files"][0], "users.crud — Crud", "the file's `name` follows its ID");
    assert_eq!(data["summary"]["run"]["profiles"][0], "dev \"Local dev\"");
    assert_eq!(data["index"][0][1], Value::Null, "an unnamed test shows its label");
    assert_eq!(data["index"][4][1], "users.crud#listsUsers [repeat 2] — Lists every user");
}

/// What a 10,000-test page loads before any test is opened (D169): the summary and the index.
#[test]
fn keeps_the_first_load_of_a_large_run_small() {
    let (_, saved) = rendered(HtmlLayout::Folder, &crate::sample_snapshot(10_000));
    let first_load = ["data/summary.js", "data/index.js"];
    let first: usize =
        saved.iter().filter(|(name, _)| first_load.contains(&name.as_str())).map(|(_, text)| text.len()).sum();
    assert!(first < 1_500_000, "summary + index of 10k tests: {first} bytes");
    assert!(saved.iter().filter(|(name, _)| name.starts_with("data/tests/")).count() > 1, "tests load in chunks");
}
