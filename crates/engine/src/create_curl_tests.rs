use vc_testkit::FakeVcs;

use crate::sample::engine;
use crate::{Added, EngineError};

const RENAME: &str = "curl -X PATCH 'http://x/users/u_7f3k?notify=true' \\\n  -H 'authorization: Bearer eyJhbGciOi' \
                      -H 'content-type: application/json' -b 'sid=t0k3n' \\\n  --data-raw '{\"name\":\"Ada\",\"password\":\"hunter2\"}'";

async fn added(id: &str, curl: &str) -> Result<Added, EngineError> {
    engine(FakeVcs::default()).new_api_from_curl(id, curl).await
}

fn written(added: &Added) -> String {
    added.changes.files.iter().map(|file| file.after.as_str()).collect()
}

#[tokio::test]
async fn a_curl_command_is_appended_to_the_file_whose_base_url_var_holds_its_origin() {
    let added = added("users#rename", RENAME).await.unwrap();
    let paths: Vec<_> = added.changes.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!((added.id.as_str(), paths), ("users#rename", vec!["services/users/users.api.yaml"]));
    let api = "  - id: rename\n    name: Rename\n    method: PATCH\n    path: /users/${var.userId}\n    vars:\n      \
               name: Ada\n      password: \"\"\n      userId: u_7f3k\n    query:\n      notify: \"true\"\n    \
               headers:\n      content-type: application/json\n    body:\n      name: ${var.name}\n      \
               password: ${var.password}\n";
    assert!(written(&added).contains(api), "{}", written(&added));
}

#[tokio::test]
async fn no_credential_is_written_and_each_is_named_in_a_note() {
    let added = added("users#rename", RENAME).await.unwrap();
    let written = written(&added);
    assert!(["eyJhbGciOi", "t0k3n", "hunter2"].iter().all(|value| !written.contains(value)), "{written}");
    let notes = added.changes.titles.get(1..).unwrap_or_default().join("\n");
    assert!(["-b:", "header authorization:", "body field password:"].iter().all(|named| notes.contains(named)));
    assert!(!notes.contains("eyJhbGciOi") && !notes.contains("t0k3n") && !notes.contains("hunter2"), "{notes}");
}

#[tokio::test]
async fn a_new_origin_gets_a_config_var_and_a_new_api_file() {
    let added = added("shop#placesOrder", "curl https://api.shop.test/orders -d '{\"sku\":\"a1\"}'").await.unwrap();
    let paths: Vec<_> = added.changes.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, ["services/shop/shop.api.yaml", "vibranium.config.yaml"]);
    let written = written(&added);
    assert!(written.contains("baseUrl: ${var.shopUrl}\n") && written.contains("shopUrl: https://api.shop.test"));
    assert!(written.contains("    method: POST\n    path: /orders\n"), "{written}");
    assert!(!written.contains("source:"), "{written}");
}

#[tokio::test]
async fn what_is_not_one_curl_command_is_refused_with_help() {
    let help = |result: Result<Added, EngineError>| result.err().and_then(|error| error.help());
    let two = added("users#rename", "curl http://x/a\ncurl http://x/b").await;
    assert_eq!(help(two).as_deref(), Some("keep one command, or write them to a file for `vc import curl <file>`"));
    let none = added("users#rename", "wget http://x/a").await;
    assert_eq!(help(none).as_deref(), Some("give one command as copied, starting `curl`, with its URL"));
    let other = added("users#rename", "curl https://elsewhere.test/a").await.err().map(|error| error.to_string());
    assert!(other.is_some_and(|error| error.contains("takes its base URL from ${var.baseUrl}")));
    assert!(added("users#Rename", RENAME).await.is_err() && added("users#list", RENAME).await.is_err());
}
