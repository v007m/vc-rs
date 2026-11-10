use super::{collection, reads};
use crate::bruno::collection::{Collection, Item};

fn read(files: &[(&str, &str)]) -> Result<Collection, String> {
    let files: Vec<_> = files.iter().map(|(path, text)| ((*path).to_owned(), (*text).to_owned())).collect();
    collection(&files)
}

fn names(items: &[Item]) -> Vec<(String, String)> {
    items.iter().map(|item| (item.name.clone(), item.kind.clone())).collect()
}

const CONFIG: (&str, &str) = ("bruno.json", r#"{"name": "Shop", "ignore": ["drafts", "node_modules"]}"#);

#[test]
fn only_bruno_json_and_bru_files_are_read() {
    assert!(reads("bruno.json") && reads("Carts/get-cart.bru"));
    assert!(!reads("Carts/bruno.json.bak") && !reads("receipt.pdf") && !reads(".env"));
}

#[test]
fn a_folder_is_named_by_its_folder_bru_else_by_itself_and_a_request_by_its_meta_else_its_file() {
    let files = [
        CONFIG,
        ("ping.bru", "get {\n  url: http://x/ping\n}\n"),
        ("Carts/folder.bru", "meta {\n  name: Shopping carts\n  seq: 2\n}\n"),
        ("Carts/get.bru", "meta {\n  name: Get cart\n  seq: 1\n}\nget {\n  url: http://x/carts\n}\n"),
        ("Users/list.bru", "meta {\n  type: graphql\n}\npost {\n  url: http://x/graphql\n}\n"),
        ("drafts/later.bru", "get {\n  url: http://x/later\n}\n"),
        ("node_modules/a/b.bru", "get {\n  url: http://x/b\n}\n"),
    ];
    let found = read(&files).ok();
    let top = found.as_ref().map(|collection| names(&collection.items)).unwrap_or_default();
    let expected = [("ping", "http"), ("Shopping carts", "folder"), ("Users", "folder")];
    assert_eq!(top, expected.map(|(name, kind)| (name.to_owned(), kind.to_owned())));
    let items = found.as_ref().map(|collection| collection.items.as_slice()).unwrap_or_default();
    assert_eq!(items.get(1).and_then(|carts| carts.seq), Some(2.0));
    let users = items.get(2).map(|users| names(&users.items)).unwrap_or_default();
    assert_eq!(users, [("list".to_owned(), "graphql".to_owned())]);
}

#[test]
fn a_request_reads_its_method_block_params_body_and_auth() {
    let pay = "post {\n  url: http://x/pay/:id?a=b\n  body: json\n  auth: apikey\n}\n\
        params:query {\n  a: b\n  ~c: d\n}\nparams:path {\n  id: 7\n}\n\
        headers {\n  ~X-Off: 1\n}\nbody:json {\n  {\"n\": 1}\n}\n\
        auth:apikey {\n  key: api_key\n  value: {{payKey}}\n  placement: queryparams\n}\n";
    let found = read(&[CONFIG, ("pay.bru", pay)]).ok();
    let request = found.as_ref().and_then(|collection| collection.items.first()?.request.as_ref());
    let Some(request) = request else { panic!("pay.bru is a request") };
    assert_eq!((request.method.as_str(), request.url.as_str()), ("POST", "http://x/pay/:id?a=b"));
    let params: Vec<_> = request.params.iter().map(|p| (p.name.as_str(), p.kind.as_str(), p.enabled)).collect();
    assert_eq!(params, [("a", "query", true), ("c", "query", false), ("id", "path", true)]);
    assert_eq!(request.headers.first().map(|header| header.enabled), Some(false));
    let body = request.body.as_ref().map(|body| (body.mode.as_str(), body.json.as_deref()));
    assert_eq!(body, Some(("json", Some("{\"n\": 1}"))));
    let key = request.auth.as_ref().and_then(|auth| auth.apikey.as_ref());
    assert_eq!(key.map(|key| (key.key.as_str(), key.placement.as_str())), Some(("api_key", "queryparams")));
}

#[test]
fn an_environment_is_named_after_its_file_and_names_its_secrets_without_their_values() {
    let local = "vars {\n  baseUrl: http://x\n}\nvars:secret [\n  apiToken,\n  ~payKey\n]\n";
    let found = read(&[CONFIG, ("environments/Local.bru", local), ("environments/old/Prod.bru", local)]).ok();
    let environments = found.as_ref().map(|collection| collection.environments.as_slice()).unwrap_or_default();
    assert_eq!(environments.iter().map(|environment| environment.name.as_str()).collect::<Vec<_>>(), ["Local"]);
    let variables = environments.first().map(|environment| environment.variables.as_slice()).unwrap_or_default();
    let variables: Vec<_> =
        variables.iter().map(|v| (v.name.as_str(), v.value.as_deref(), v.enabled, v.secret)).collect();
    let expected =
        [("baseUrl", Some("http://x"), true, false), ("apiToken", None, true, true), ("payKey", None, false, true)];
    assert_eq!(variables, expected);
    assert!(found.is_some_and(|collection| collection.items.is_empty()), "an environment is not a request");
}

#[test]
fn a_folder_without_a_readable_bruno_json_or_with_a_broken_file_is_refused_without_quoting_it() {
    let refused = |files: &[(&str, &str)]| read(files).err().unwrap_or_default();
    let ping = ("ping.bru", "get {\n  url: http://x/ping\n}\n");
    assert_eq!(refused(&[ping]), "it is not a Bruno collection: the folder has no bruno.json");
    assert_eq!(refused(&[("bruno.json", "{hunter2"), ping]), "its bruno.json is not JSON");
    let broken = ("Carts/get.bru", "headers {\n  X-Token: hunter2\n");
    assert_eq!(refused(&[CONFIG, broken]), "Carts/get.bru: its headers block has no closing }");
    assert_eq!(refused(&[CONFIG, ("drafts/broken.bru", broken.1)]), "", "an ignored file is not parsed");
}

#[test]
fn a_bru_extension_in_any_case_is_left_out_of_an_environment_or_request_name() {
    let prod = ("environments/Prod.BRU", "vars {\n  a: b\n}\n");
    let found = read(&[CONFIG, prod, ("Get.Bru", "get {\n  url: http://x\n}\n")]).ok();
    let environments = found.as_ref().map(|collection| collection.environments.as_slice()).unwrap_or_default();
    assert_eq!(environments.iter().map(|environment| environment.name.as_str()).collect::<Vec<_>>(), ["Prod"]);
    let items = found.as_ref().map(|collection| names(&collection.items)).unwrap_or_default();
    assert_eq!(items, [("Get".to_owned(), "http".to_owned())]);
}
