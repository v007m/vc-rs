//! The vars `vc new` sets. D262: `vc new test` sets each path var that nothing else sets. A var
//! the api gives a value keeps it, with its type: an imported api's value comes from the spec's
//! schema (an integer, a uuid, a boolean). A var with no value anywhere gets the `replace-me`
//! placeholder. Either way the new file passes `vc check`. A new api file from `--path` gets
//! its own base-URL var.

use crate::{CONFIG, Project, checks_clean, said};

/// A spec whose one operation has an integer, a uuid and a boolean path parameter.
const SPEC: (&str, &str) = (
    "specs/shop.yaml",
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

/// `users#get` calls `/users/${var.userId}`; `vars` is the api file's text before `apis:`.
fn users_api(vars: &str) -> (&'static str, String) {
    let text = format!("baseUrl: ${{var.baseUrl}}\n{vars}apis:\n  - id: get\n    path: /users/${{var.userId}}\n");
    ("services/users/users.api.yaml", text)
}

/// `vc new test users.crud#getsUser --from users#get` in `project`: asserts the new test sets
/// no var of its own, since `why` sets `userId`, and the project checks clean.
fn new_get_test_sets_no_var(project: &Project, why: &str) {
    let output = project.vc(&["new", "test", "users.crud#getsUser", "--from", "users#get"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let text = project.read("services/users/crud.test.yaml");
    assert!(text.contains("  - id: getsUser\n    name: Gets user\n    call: users#get\n"), "{text}");
    assert!(!text.contains("    vars:") && !text.contains("replace-me"), "{why} sets it:\n{text}");
    checks_clean(project);
}

#[test]
fn new_test_leaves_a_var_the_api_file_sets_at_its_root_or_in_a_profile() {
    for vars in ["vars: {userId: u_1}\n", "profiles:\n  - id: dev\n    vars: {userId: u_1}\n"] {
        let api = users_api(vars);
        new_get_test_sets_no_var(&Project::new(&[CONFIG, (api.0, &api.1)]), vars);
    }
}

#[test]
fn new_test_leaves_a_var_the_test_file_sets() {
    let api = users_api("");
    let tests =
        ("services/users/crud.test.yaml", "vars: {userId: u_1}\ntests:\n  - id: listsUsers\n    call: users#get\n");
    new_get_test_sets_no_var(&Project::new(&[CONFIG, (api.0, &api.1), tests]), "the test file");
}

#[test]
fn new_test_leaves_a_var_a_config_profile_sets() {
    let config = (CONFIG.0, CONFIG.1.replace("    default: true\n", "    default: true\n    vars: {userId: u_1}\n"));
    let api = users_api("");
    new_get_test_sets_no_var(&Project::new(&[(config.0, &config.1), (api.0, &api.1)]), "the dev profile");
}

#[test]
fn two_new_api_files_from_path_each_get_their_own_base_url_var() {
    let project = Project::new(&[CONFIG]);
    for (service, other) in [("refunds", "orders"), ("orders", "refunds")] {
        let path = format!("/{service}");
        let output = project.vc(&["new", "api", &format!("{service}#list"), "--path", &path]).output().unwrap();
        assert_eq!(output.status.code(), Some(0), "{}", said(&output));
        let note = format!("  set `vars.{service}Url` in the config to the service's URL; it is http://localhost\n");
        assert!(String::from_utf8_lossy(&output.stdout).ends_with(&note), "{}", said(&output));
        let text = project.read(&format!("services/{service}/{service}.api.yaml"));
        assert!(text.contains(&format!("\nbaseUrl: ${{var.{service}Url}}\napis:\n")), "{text}");
        assert!(!text.contains(&format!("{other}Url")), "never another service's var:\n{text}");
    }
    let config = project.read(CONFIG.0);
    assert!(
        config.contains("refundsUrl: http://localhost") && config.contains("ordersUrl: http://localhost"),
        "{config}"
    );
    checks_clean(&project);
}

#[test]
fn a_base_url_var_a_profile_sets_is_reused_with_no_note() {
    let config =
        (CONFIG.0, CONFIG.1.replace("    default: true\n", "    default: true\n    vars: {refundsUrl: 'http://r'}\n"));
    let project = Project::new(&[(config.0, &config.1)]);
    let output = project.vc(&["new", "api", "refunds#list", "--path", "/refunds"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    assert!(!said(&output).contains("set `vars."), "the profile sets it: {}", said(&output));
    assert!(project.read("services/refunds/refunds.api.yaml").contains("\nbaseUrl: ${var.refundsUrl}\n"));
    assert_eq!(project.read(CONFIG.0), config.1, "the config is left as it was");
    checks_clean(&project);
}

#[test]
fn a_url_api_never_reuses_another_services_localhost_var() {
    let project = Project::new(&[CONFIG]);
    project.vc(&["new", "api", "refunds#list", "--path", "/refunds"]).assert().code(0);
    let output = project.vc(&["new", "api", "orders#list", "--url", "http://localhost/orders"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let text = project.read("services/orders/orders.api.yaml");
    assert!(text.contains("\nbaseUrl: ${var.ordersUrl}\napis:\n"), "its own var, not refundsUrl:\n{text}");
    let config = project.read(CONFIG.0);
    assert!(
        config.contains("refundsUrl: http://localhost") && config.contains("ordersUrl: http://localhost"),
        "{config}"
    );
    checks_clean(&project);
}

#[test]
fn a_url_api_whose_var_a_profile_sets_is_refused_not_shadowed() {
    let dev = "    default: true\n    vars: {refundsUrl: 'http://r'}\n";
    let config = (CONFIG.0, CONFIG.1.replace("    default: true\n", dev));
    let project = Project::new(&[(config.0, &config.1)]);
    let args = ["new", "api", "refunds#list", "--url", "https://refunds.example/list"];
    let output = project.vc(&args).output().unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", said(&output));
    let text = said(&output);
    assert!(text.contains("the config already sets `refundsUrl`, to another URL or in a profile"), "{text}");
    assert!(
        text.contains("help: set `vars.refundsUrl` to https://refunds.example, or add the api by `--path`"),
        "{text}"
    );
    assert_eq!(project.read(CONFIG.0), config.1, "the config is left as it was");
    assert!(!project.path().join("services/refunds").exists(), "no api file is written");
}
