use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::{V1Migration, migrate_v1};

macro_rules! fixture {
    ($($path:literal),* $(,)?) => {
        [$(($path, include_str!(concat!("../../tests/fixtures/v1/shop/", $path)))),*]
    };
}

fn shop() -> V1Migration {
    let files = fixture![
        "config.json",
        "Vibranium-Tests/payloads/orders/new_order.json",
        "Vibranium-Tests/scenarios/catalog/catalog_browse.json",
        "Vibranium-Tests/scenarios/orders/orders_checkout.json",
        "Vibranium-Tests/scenarios/users/users_crud.json",
        "Vibranium-Tests/schemas/users/user.json",
    ];
    let files: BTreeMap<String, String> =
        files.iter().map(|(path, text)| ((*path).to_owned(), (*text).to_owned())).collect();
    migrate_v1(&files).unwrap()
}

fn doc<'m>(migration: &'m V1Migration, path: &str) -> &'m Value {
    &migration.files.iter().find(|file| file.path == path).unwrap().doc
}

fn api<'d>(file: &'d Value, id: &str) -> &'d Value {
    file["apis"].as_array().unwrap().iter().find(|api| api["id"] == id).unwrap()
}

#[test]
fn the_config_comes_first_and_every_collection_is_an_api_file() {
    let migration = shop();
    let paths: Vec<_> = migration.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(
        paths[..4],
        [
            "vibranium.config.yaml",
            "services/catalog/catalog.api.yaml",
            "services/orders/orders.api.yaml",
            "services/users/users.api.yaml",
        ]
    );
    let config = doc(&migration, "vibranium.config.yaml");
    assert_eq!(config["project"], "shop");
    assert_eq!(config["vars"], json!({"apiUrl": "http://localhost:7421", "currency": "EUR"}));
    let generators: Vec<_> = config["generators"].as_array().unwrap().iter().map(|g| g["id"].clone()).collect();
    assert_eq!(generators, [json!("lorem20"), json!("lorem50"), json!("shortText")]);
    assert_eq!(migration.secrets.len(), 4);
    for file in &migration.files {
        let text = file.doc.to_string();
        assert!(!text.contains("fixture-admin-password") && !text.contains("fixture-partner-secret"), "{text}");
    }
}

#[test]
fn endpoints_become_apis_with_their_url_options_and_body() {
    let migration = shop();
    let users = doc(&migration, "services/users/users.api.yaml");
    let create = api(users, "createUser");
    assert_eq!(create["vars"]["role"], "${fake.helpers.arrayElement('ADMIN', 'MEMBER')}");
    assert_eq!(create["vars"]["userName"], "${fake.person.fullName}");
    assert_eq!(create["body"]["bio"], "${fake.lorem50}");
    let update = api(users, "updateUser");
    assert_eq!(
        (update["method"].as_str(), update["path"].as_str()),
        (Some("PUT"), Some("/api/v1/users/${var.userId}"))
    );
    assert_eq!((update["headers"].clone(), update["timeout"].clone()), (json!({"accept-language": "de"}), json!("5s")));
    assert_eq!(api(users, "deleteUser")["auth"], "shopAdmin");
    assert!(api(users, "listUsers").get("auth").is_none());
}

#[test]
fn query_vars_and_auth_carry_over() {
    let migration = shop();
    let catalog = doc(&migration, "services/catalog/catalog.api.yaml");
    assert_eq!(catalog["vars"], json!({"pageSize": 20, "searchTerm": "${fake.lorem.word}"}));
    let search = api(catalog, "searchProducts");
    assert_eq!(
        search["query"],
        json!({"q": "${var.searchTerm}", "category": "${var.categoryCode}", "limit": "${var.pageSize}"})
    );
    let orders = doc(&migration, "services/orders/orders.api.yaml");
    let place = api(orders, "placeOrder");
    assert_eq!(place["body"]["items"][1]["productId"], "${var.productIds[1]}");
    assert_eq!(place["body"]["reference"], "${fake.shortText}");
    assert_eq!(place["auth"], "shopPartner");
}

#[test]
fn every_dataset_is_noted() {
    let migration = shop();
    let datasets: Vec<_> = migration.notes.iter().filter(|note| note.message.contains("{dataset.")).collect();
    assert!(datasets.iter().any(|note| note.message.contains("{dataset.pokemon}")), "{datasets:?}");
    assert!(datasets.iter().any(|note| note.message.contains("{dataset.harrypotter}")), "{datasets:?}");
}

fn test<'d>(file: &'d Value, id: &str) -> &'d Value {
    file["tests"].as_array().unwrap().iter().find(|test| test["id"] == id).unwrap()
}

#[test]
fn every_scenario_is_a_test_file_whose_dependencies_are_calls_made_first() {
    let migration = shop();
    let crud = doc(&migration, "services/users/crud.test.yaml");
    assert_eq!(crud["tags"], json!(["crud", "smoke"]));
    let update = test(crud, "updatesUser");
    let saved = json!({"userId": "${response.body.id}", "oldName": "${response.body.name}"});
    assert_eq!(update["before"], json!([{"call": "users#createUser", "save": saved}]));
    assert_eq!(update["expect"]["body"]["match"], json!({"name": {"ne": "${var.oldName}"}}));
    assert_eq!(test(crud, "createsUser")["repeat"], 3);
    let checkout = doc(&migration, "services/orders/checkout.test.yaml");
    assert_eq!(checkout["vars"], json!({"tenant": "acme"}));
    assert_eq!(checkout["beforeAll"][0]["save"], json!({"categoryCode": "${response.body[*].code | random}"}));
    let cancel = test(checkout, "cancelsOrder");
    assert_eq!(cancel["vars"], json!({"categoryCode": "${var.listCategories.categoryCode}"}));
    let calls: Vec<_> = cancel["before"].as_array().unwrap().iter().map(|call| call["call"].clone()).collect();
    assert_eq!(calls, [json!("users#createUser"), json!("catalog#searchProducts"), json!("orders#placeOrder")]);
    assert_eq!(cancel["before"][2]["vars"], json!({"quantity": 1}));
}

#[test]
fn repeat_until_is_a_poll_and_ignore_a_skip() {
    let migration = shop();
    let checkout = doc(&migration, "services/orders/checkout.test.yaml");
    let poll = &test(checkout, "waitsForConfirmation")["poll"];
    assert_eq!(
        poll,
        &json!({"until": {"status": 200, "body": {"match": {"status": "CONFIRMED"}}}, "every": "1s", "timeout": "30s"})
    );
    let browse = doc(&migration, "services/catalog/browse.test.yaml");
    assert_eq!(test(browse, "legaciesSearch")["skip"], "ignored in v1");
}

#[test]
fn what_has_no_v2_form_is_a_todo_or_a_note() {
    let migration = shop();
    let todos = |path: &str| doc_todos(&migration, path);
    let checkout = todos("services/orders/checkout.test.yaml");
    assert!(checkout.iter().any(|(at, todo)| at == "cancelsOrder" && todo.starts_with("scripts.before-endpoint: ")));
    let crud = todos("services/users/crud.test.yaml");
    assert!(crud.iter().any(|(at, todo)| at.is_empty() && todo.starts_with("scripts.before-scenario: ")));
    let orders = doc(&migration, "services/orders/orders.api.yaml");
    assert_eq!(api(orders, "cancelOrder")["vars"], json!({"reason": ""}));
    assert!(
        todos("services/orders/orders.api.yaml")
            .iter()
            .any(|(at, todo)| at == "cancelOrder" && todo.starts_with("reason "))
    );
    let notes: Vec<_> = migration.notes.iter().map(|note| note.message.as_str()).collect();
    assert!(
        notes.iter().any(|note| note.starts_with("delay ")) && notes.iter().any(|note| note.starts_with("async ")),
        "{notes:?}"
    );
}

fn doc_todos(migration: &V1Migration, path: &str) -> Vec<(String, String)> {
    let file = migration.files.iter().find(|file| file.path == path).unwrap();
    file.todos.iter().map(|todo| (todo.at.clone(), todo.message.clone())).collect()
}
