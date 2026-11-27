use std::fs;
use std::sync::Arc;

use tempfile::TempDir;
use vc_contracts::engine::v1 as wire;
use vc_engine::EngineBuilder;

use super::{answers, demo, init};
use crate::engine_fixture::serving;
use crate::project_rpcs::get_project;
use crate::{Code, EngineApi};

/// An empty folder and the API over an engine rooted at it, as `vc ui` starts with no project.
fn folder() -> (TempDir, Arc<EngineApi>) {
    let dir = tempfile::tempdir().unwrap();
    let engine = EngineBuilder::new(dir.path().to_path_buf()).build().unwrap();
    (dir, Arc::new(EngineApi::new(Arc::new(engine), Vec::new(), Vec::new())))
}

fn wizard(expected: Option<String>) -> wire::InitRequest {
    wire::InitRequest {
        name: Some("shop".into()),
        service: Some("users".into()),
        base_url: Some("https://api.example.com".into()),
        profile: Some("dev".into()),
        sample: false,
        write: expected.is_some(),
        expected_digest: expected,
    }
}

#[tokio::test]
async fn the_wizard_previews_then_writes_the_project_and_get_project_sees_it() {
    let (dir, api) = folder();
    assert!(!get_project(api.clone(), wire::GetProjectRequest::default()).await.unwrap().configured);
    let preview = init(api.clone(), wizard(None)).await.unwrap();
    let paths: Vec<&str> = preview.files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, [".env.schema", ".gitignore", "services/users/users.api.yaml", "vibranium.config.yaml"]);
    assert!(!dir.path().join("vibranium.config.yaml").exists() && !preview.root.is_empty());

    let stale = init(api.clone(), wizard(Some("0".into()))).await.unwrap_err();
    assert_eq!(stale.code, Code::Aborted);
    assert!(!dir.path().join("vibranium.config.yaml").exists());

    init(api.clone(), wizard(Some(preview.digest))).await.unwrap();
    let project = get_project(api.clone(), wire::GetProjectRequest::default()).await.unwrap();
    assert!(project.configured);
    assert_eq!((project.name.as_str(), project.profiles.as_slice()), ("shop", &["dev".to_owned()][..]));
    let again = init(api, wizard(None)).await.unwrap_err();
    assert_eq!(again.code, Code::InvalidArgument);
}

#[tokio::test]
async fn a_choice_that_does_not_fit_is_invalid_and_not_echoed() {
    let (dir, api) = folder();
    let requests = [
        wire::InitRequest { base_url: Some("https://me:sekrit@example.com".into()), ..wizard(None) },
        wire::InitRequest { service: Some("../sekrit".into()), ..wizard(None) },
        wire::InitRequest { name: Some("sekrit\r\n".into()), ..wizard(None) },
    ];
    for request in requests {
        let error = init(api.clone(), request).await.unwrap_err();
        assert_eq!(error.code, Code::InvalidArgument);
        assert!(!format!("{error:?}").contains("sekrit"), "{error:?}");
    }
    assert_eq!(
        fs::read_dir(dir.path()).unwrap().flatten().filter(|entry| entry.file_name() != ".vibranium").count(),
        0
    );
}

#[tokio::test]
async fn the_demo_writes_the_sample_project_once_and_says_how_to_start_its_service() {
    let (dir, api) = folder();
    let preview = demo(api.clone(), wire::DemoRequest::default()).await.unwrap();
    assert_eq!((preview.url.as_str(), preview.start.as_str()), ("http://localhost:7421", "vc demo start --detach"));
    let project = preview.project.unwrap();
    assert!(project.files.iter().any(|file| file.path == "services/sample/basics.test.yaml"));
    let write = wire::DemoRequest { write: true, expected_digest: Some(project.digest) };
    demo(api.clone(), write).await.unwrap();
    assert!(dir.path().join("services/sample/sample.api.yaml").is_file());
    let again = demo(api, wire::DemoRequest::default()).await.unwrap();
    assert_eq!(again.project.unwrap().files.len(), 0);
}

#[tokio::test]
async fn only_a_service_that_echoes_the_path_is_the_demo() {
    let port = |url: String| url.rsplit(':').next().unwrap().parse::<u16>().unwrap();
    assert!(answers(port(serving(200, br#"{"path":"/echo","pid":1}"#.to_vec()))).await);
    assert!(!answers(port(serving(200, b"{}".to_vec()))).await);
    assert!(!answers(port(serving(404, br#"{"path":"/echo"}"#.to_vec()))).await);
    assert!(!answers(9).await);
}
