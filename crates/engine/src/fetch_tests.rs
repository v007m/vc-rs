use std::sync::Arc;

use serde_json::json;
use vc_interfaces::{FetchError, SpecFetcher, TransportError};
use vc_testkit::{FakeTransport, MemorySources, status};

use super::ProjectFetcher;
use crate::spec_headers::SpecHeaders;

fn fetcher(transport: FakeTransport) -> ProjectFetcher {
    let sources = MemorySources::new(&[("specs/users.yaml", b"openapi: 3.1.0\ninfo: {title: Users}\n")]);
    ProjectFetcher { sources: Arc::new(sources), transport: Some(Arc::new(transport)), headers: SpecHeaders::default() }
}

#[tokio::test]
async fn a_path_is_read_from_the_project_and_a_url_through_the_transport() -> Result<(), FetchError> {
    let served = FakeTransport::new(|request, _| {
        let mut response = status(200);
        response.body = format!("{{\"url\": \"{}\"}}", request.uri).into();
        Ok(response)
    });
    let fetcher = fetcher(served);
    let bytes = fetcher.fetch("specs/users.yaml").await?;
    assert_eq!(
        fetcher.decode("specs/users.yaml", &bytes)?,
        json!({ "openapi": "3.1.0", "info": { "title": "Users" } })
    );
    let bytes = fetcher.fetch("https://specs.test/users.json").await?;
    assert_eq!(fetcher.decode("users.json", &bytes)?, json!({ "url": "https://specs.test/users.json" }));
    Ok(())
}

#[tokio::test]
async fn what_cannot_be_fetched_says_why() {
    let fetcher = fetcher(FakeTransport::scripted(vec![Ok(status(404))]));
    let missing = fetcher.fetch("https://specs.test/gone.json").await;
    assert!(matches!(&missing, Err(FetchError::Failed { reason, .. }) if reason.contains("404")), "{missing:?}");
    let outside = fetcher.fetch("../backend/openapi.json").await;
    for hidden in [".env", "specs/.secret.yaml", ".vibranium/history.db"] {
        let refused = fetcher.fetch(hidden).await;
        assert!(matches!(refused, Err(FetchError::NotAllowed { .. })), "{hidden}: {refused:?}");
    }
    assert!(matches!(outside, Err(FetchError::NotAllowed { .. } | FetchError::Failed { .. })), "{outside:?}");
    let down = ProjectFetcher {
        transport: Some(Arc::new(FakeTransport::scripted(vec![Err(TransportError::InvalidRequest {
            reason: "down".to_owned(),
        })]))),
        ..fetcher
    };
    let failed = down.fetch("https://specs.test/a.json").await;
    assert!(matches!(&failed, Err(FetchError::Failed { reason, .. }) if reason.contains("down")), "{failed:?}");
}

#[test]
fn text_that_is_neither_yaml_nor_json_is_undecodable() {
    let fetcher = fetcher(FakeTransport::echo());
    for bytes in [&b"{\"a\": 1"[..], b"a: [1", b"\xff\xfe", b""] {
        let decoded = fetcher.decode("spec", bytes);
        assert!(
            matches!(&decoded, Err(FetchError::Undecodable { location, .. }) if location == "spec"),
            "{bytes:?}: {decoded:?}"
        );
    }
}

#[tokio::test]
async fn a_folder_lists_its_files_but_no_hidden_or_ignored_one() -> Result<(), FetchError> {
    let files: [(&str, &[u8]); 7] = [
        ("bruno/shop/bruno.json", b"{}"),
        ("bruno/shop/Carts/create-cart.bru", b"meta {\n}\n"),
        ("bruno/shop/.env", b"API_TOKEN=x"),
        ("bruno/shop/node_modules/a/index.js", b""),
        ("bruno/shop-two/bruno.json", b"{}"),
        ("bruno/shop/users.api.yaml", b"apis: []\n"),
        ("specs/users.yaml", b"openapi: 3.1.0\n"),
    ];
    let fetcher = ProjectFetcher {
        sources: Arc::new(MemorySources::new(&files)),
        transport: None,
        headers: SpecHeaders::default(),
    };
    let listed = fetcher.list("bruno/shop").await?;
    assert_eq!(listed, ["Carts/create-cart.bru", "bruno.json", "users.api.yaml"]);
    for nothing in ["specs/users.yaml", "bruno/gone", "bruno/shop/.env", "https://specs.test/shop"] {
        assert_eq!(fetcher.list(nothing).await?, Vec::<String>::new(), "{nothing}");
    }
    Ok(())
}
