use std::fs;

use vc_contracts::engine::v1 as wire;

use super::{plugin_add, plugin_list, plugin_remove, test_alert};
use crate::Code;
use crate::engine_fixture::{SECRET, project};

const MANIFEST: &str = concat!(
    "id: report\nversion: 1.0.0\nkind: reporter\n",
    "entry: {command: python3, args: [report.py]}\n",
    "permissions: {secrets: [reportToken], network: [api.example.com]}\n",
);

fn add(source: &str, expected: Option<String>) -> wire::PluginAddRequest {
    wire::PluginAddRequest { source: source.into(), write: expected.is_some(), expected_digest: expected }
}

#[tokio::test]
async fn a_plugin_is_previewed_added_with_the_digest_listed_then_removed() {
    let (dir, api) = project();
    fs::create_dir_all(dir.path().join("plugin")).unwrap();
    fs::write(dir.path().join("plugin/vibranium-plugin.yaml"), MANIFEST).unwrap();
    let preview = plugin_add(api.clone(), add("path:plugin", None)).await.unwrap();
    let manifest = preview.manifest.clone().unwrap();
    assert_eq!((manifest.id.as_str(), manifest.kind.as_str()), ("report", "reporter"));
    assert_eq!(manifest.permissions.unwrap().network, ["api.example.com"]);
    assert_eq!(
        preview.files.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(),
        ["vibranium.config.yaml", "vibranium.lock"]
    );
    assert!(!dir.path().join("vibranium.lock").exists());

    let stale = plugin_add(api.clone(), add("path:plugin", Some("0".into()))).await.unwrap_err();
    assert_eq!(stale.code, Code::Aborted);
    plugin_add(api.clone(), add("path:plugin", Some(preview.digest))).await.unwrap();
    let listed = plugin_list(api.clone(), wire::PluginListRequest {}).await.unwrap();
    let plugin = &listed.plugins[0];
    assert_eq!((plugin.id.as_str(), plugin.source.as_str()), ("report", "path:plugin"));
    assert_eq!((plugin.state.as_str(), plugin.detail.as_str()), ("ready", "ready"));
    assert_eq!(plugin.granted.clone().unwrap().secrets, ["reportToken"]);

    let request = wire::PluginRemoveRequest { id: "report".into(), ..Default::default() };
    let removal = plugin_remove(api.clone(), request).await.unwrap();
    let request = wire::PluginRemoveRequest { id: "report".into(), write: true, expected_digest: Some(removal.digest) };
    plugin_remove(api.clone(), request).await.unwrap();
    assert_eq!(plugin_list(api, wire::PluginListRequest {}).await.unwrap().plugins, []);
    assert!(!dir.path().join(".vibranium/plugins/report").exists());
}

#[tokio::test]
async fn a_source_outside_the_project_or_not_a_registry_package_is_invalid_and_not_echoed() {
    let (_dir, api) = project();
    for source in ["path:../sekrit", "path:/sekrit", "npm:file:../sekrit", "npm:git+https://u:sekrit@example.com/r.git"]
    {
        let error = plugin_add(api.clone(), add(source, None)).await.unwrap_err();
        assert_eq!(error.code, Code::InvalidArgument, "{source}");
        assert!(!format!("{error:?}").contains("sekrit"), "{source}: {error:?}");
    }
    let request = wire::PluginRemoveRequest { id: "../sekrit".into(), ..Default::default() };
    let error = plugin_remove(api, request).await.unwrap_err();
    assert_eq!(error.code, Code::InvalidArgument);
    assert!(!format!("{error:?}").contains("sekrit"), "{error:?}");
}

#[tokio::test]
async fn a_test_alert_takes_the_run_checks_and_reports_each_channel_without_its_url() {
    let (dir, api) = project();
    let config = fs::read_to_string(dir.path().join("vibranium.config.yaml")).unwrap();
    let alerts = format!("alerts:\n  - notify:\n      - webhook: {{url: 'http://127.0.0.1:9/hook?key={SECRET}'}}\n");
    fs::write(dir.path().join("vibranium.config.yaml"), format!("{config}{alerts}")).unwrap();
    let request = wire::TestAlertRequest { sets: vec!["reports.dir=../out".into()], ..Default::default() };
    assert_eq!(test_alert(api.clone(), request).await.unwrap_err().code, Code::InvalidArgument);

    let sent = test_alert(api, wire::TestAlertRequest::default()).await.unwrap().sent;
    assert_eq!((sent.len(), sent[0].rule, sent[0].channel.as_str()), (1, 1, "webhook"));
    assert!(sent[0].error.is_some(), "nothing listens on port 9");
    assert!(!format!("{sent:?}").contains(SECRET), "{sent:?}");
}
