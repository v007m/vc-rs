//! D262: `vc new test` sets each path var the config leaves unset. A var the api gives a value
//! keeps it, with its type: an imported api's value comes from the spec's schema (an integer, a
//! uuid, a boolean). A var with no value anywhere gets the `replace-me` placeholder. Either way
//! the new file passes `vc check`.

use crate::{CONFIG, Project, checks_clean, said};

/// A spec whose one operation has an integer, a uuid and a boolean path parameter.
const SPEC: (&str, &str) = (
    "openapi/shop.yaml",
    "openapi: 3.1.0\ninfo: {title: Shop, version: '1'}\nservers:\n  - url: http://localhost\npaths:\n\
     \x20 /orders/{orderId}/items/{itemId}/{gift}:\n    get:\n      operationId: getItem\n      parameters:\n\
     \x20       - {name: orderId, in: path, required: true, schema: {type: integer, minimum: 1, maximum: 9}}\n\
     \x20       - {name: itemId, in: path, required: true, schema: {type: string, format: uuid}}\n\
     \x20       - {name: gift, in: path, required: true, schema: {type: boolean}}\n\
     \x20     responses: {'200': {description: ok}}\n",
);

/// The new test's `vars:` line for `name`.
fn var_line<'t>(text: &'t str, name: &str) -> &'t str {
    let start = format!("      {name}: ");
    text.lines().find(|line| line.starts_with(&start)).unwrap_or_else(|| panic!("no {name}:\n{text}"))
}

#[test]
fn new_test_for_an_imported_api_sets_each_path_var_to_a_value_of_its_type() {
    let project = Project::new(&[CONFIG, SPEC]);
    let imported = project.vc(&["import", "openapi", SPEC.0, "--id", "shop"]).output().unwrap();
    assert_eq!(imported.status.code(), Some(0), "{}", said(&imported));
    let output = project.vc(&["new", "test", "shop.items#getsItem", "--from", "shop#getItem"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let text = project.read("services/shop/items.test.yaml");
    let sample = "  # replace: the api's sample value";
    let integer = var_line(&text, "orderId");
    assert!(integer.contains("fake.number.int(1, 9)") && integer.ends_with(sample), "an integer:\n{text}");
    let uuid = var_line(&text, "itemId");
    assert!(uuid.contains("fake.string.uuid") && uuid.ends_with(sample), "a uuid:\n{text}");
    assert_eq!(var_line(&text, "gift"), format!("      gift: true{sample}"), "a boolean:\n{text}");
    checks_clean(&project);
}

#[test]
fn new_test_keeps_an_api_values_type_as_written() {
    let api = (
        "services/stock/stock.api.yaml",
        "baseUrl: ${var.baseUrl}\napis:\n  - id: shelf\n    path: /shelves/${var.shelfNo}/${var.open}/${var.lotId}\n\
         \x20   vars: {shelfNo: 7, open: false, lotId: 00000000-0000-0000-0000-000000000000}\n",
    );
    let project = Project::new(&[CONFIG, api]);
    project.vc(&["new", "test", "stock.shelves#getsShelf", "--from", "stock#shelf"]).assert().code(0);
    let text = project.read("services/stock/shelves.test.yaml");
    let sample = "  # replace: the api's sample value";
    assert_eq!(var_line(&text, "shelfNo"), format!("      shelfNo: 7{sample}"), "an integer:\n{text}");
    assert_eq!(var_line(&text, "open"), format!("      open: false{sample}"), "a boolean:\n{text}");
    let lot = format!("      lotId: 00000000-0000-0000-0000-000000000000{sample}");
    assert_eq!(var_line(&text, "lotId"), lot, "a uuid:\n{text}");
    checks_clean(&project);
}

#[test]
fn new_test_gives_a_var_with_no_value_anywhere_the_placeholder() {
    let api = (
        "services/stock/stock.api.yaml",
        "baseUrl: ${var.baseUrl}\napis:\n  - id: bin\n    path: /bins/${var.binId}\n",
    );
    let project = Project::new(&[CONFIG, api]);
    project.vc(&["new", "test", "stock.bins#getsBin", "--from", "stock#bin"]).assert().code(0);
    let text = project.read("services/stock/bins.test.yaml");
    assert_eq!(var_line(&text, "binId"), "      binId: replace-me  # replace with a real binId", "{text}");
    checks_clean(&project);
}
