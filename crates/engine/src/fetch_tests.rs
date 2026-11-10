use std::sync::Arc;

use serde_json::json;
use vc_interfaces::{FetchError, SpecFetcher, TransportError};
use vc_testkit::{FakeTransport, MemorySources, status};

use super::ProjectFetcher;

fn fetcher(transport: FakeTransport) -> ProjectFetcher {
    let sources = MemorySources::new(&[("specs/users.yaml", b"openapi: 3.1.0\ninfo: {title: Users}\n")]);
    ProjectFetcher { sources: Arc::new(sources), transport: Some(Arc::new(transport)) }
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
