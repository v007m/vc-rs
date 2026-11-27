use std::fs;

use vc_contracts::engine::v1 as wire;

use super::{export_schedule, list_schedules, write_schedule};
use crate::Code;
use crate::engine_fixture::{CONFIG, SECRET, project};

const CONFIG_FILE: &str = "vibranium.config.yaml";

fn nightly() -> wire::Schedule {
    wire::Schedule {
        id: "nightly".into(),
        name: Some("Nightly smoke".into()),
        cron: "0 2 * * *".into(),
        selector: vec!["users.crud".into()],
        profiles: vec!["dev".into()],
        notify: vec![wire::NotifyChannel {
            kind: "slack".into(),
            settings: vec![wire::NotifySetting { key: "webhook".into(), value: "${secret.SHOP_TOKEN}".into() }],
        }],
        runner: String::new(),
        enabled: true,
    }
}

fn putting(schedule: wire::Schedule) -> wire::WriteScheduleRequest {
    wire::WriteScheduleRequest { schedule: Some(schedule), ..wire::WriteScheduleRequest::default() }
}

#[tokio::test]
async fn a_schedule_is_previewed_then_written_then_listed_with_when_it_next_fires() {
    let (dir, api) = project();
    let on_disk = || fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap();
    let preview = write_schedule(api.clone(), putting(nightly())).await.unwrap();
    assert_eq!(
        (preview.files[0].path.as_str(), preview.titles.as_slice()),
        (CONFIG_FILE, ["Add schedule nightly".to_owned()].as_slice())
    );
    assert_eq!(on_disk(), CONFIG, "a preview writes nothing");
    let writing =
        wire::WriteScheduleRequest { write: true, expected_digest: Some(preview.digest), ..putting(nightly()) };
    write_schedule(api.clone(), writing).await.unwrap();
    assert!(on_disk().contains("    cron: 0 2 * * *\n"), "{}", on_disk());
    let listed = list_schedules(api, wire::ListSchedulesRequest::default()).await.unwrap();
    let [listing] = listed.schedules.as_slice() else { panic!("{listed:?}") };
    let written = wire::Schedule { runner: "vcServer".into(), ..nightly() };
    assert_eq!(listing.schedule.as_ref(), Some(&written), "the settings' secret references come back as written");
    assert!(listing.next_ms.is_some_and(|next| next % 60_000 == 0), "{listing:?}");
    assert_eq!(listing.last, None);
    assert!(!format!("{listed:?}").contains(SECRET));
}

#[tokio::test]
async fn a_write_without_its_previews_digest_or_after_the_config_changed_is_aborted() {
    let (dir, api) = project();
    let unpreviewed = wire::WriteScheduleRequest { write: true, ..putting(nightly()) };
    assert_eq!(write_schedule(api.clone(), unpreviewed).await.unwrap_err().code, Code::Aborted);
    let preview = write_schedule(api.clone(), putting(nightly())).await.unwrap();
    fs::write(dir.path().join(CONFIG_FILE), format!("{CONFIG}# edited\n")).unwrap();
    let stale = wire::WriteScheduleRequest { write: true, expected_digest: Some(preview.digest), ..putting(nightly()) };
    assert_eq!(write_schedule(api, stale).await.unwrap_err().code, Code::Aborted);
    assert_eq!(fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap(), format!("{CONFIG}# edited\n"));
}

#[tokio::test]
async fn a_bad_schedule_or_request_is_an_invalid_argument_and_writes_nothing() {
    let (dir, api) = project();
    let refusals = [
        putting(wire::Schedule { cron: "0 25 * * *".into(), ..nightly() }),
        putting(wire::Schedule { runner: "cronJob".into(), ..nightly() }),
        wire::WriteScheduleRequest::default(),
        wire::WriteScheduleRequest { remove: Some("nightly".into()), ..putting(nightly()) },
        wire::WriteScheduleRequest { remove: Some("nightly".into()), ..wire::WriteScheduleRequest::default() },
    ];
    for request in refusals {
        let refused = write_schedule(api.clone(), request.clone()).await.unwrap_err();
        assert_eq!(refused.code, Code::InvalidArgument, "{request:?}: {refused:?}");
    }
    assert_eq!(fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap(), CONFIG);
}

#[tokio::test]
async fn a_schedule_is_removed_through_the_same_preview_and_digest() {
    let (dir, api) = project();
    let preview = write_schedule(api.clone(), putting(nightly())).await.unwrap();
    let writing =
        wire::WriteScheduleRequest { write: true, expected_digest: Some(preview.digest), ..putting(nightly()) };
    write_schedule(api.clone(), writing).await.unwrap();
    let removing =
        wire::WriteScheduleRequest { remove: Some("nightly".into()), ..wire::WriteScheduleRequest::default() };
    let preview = write_schedule(api.clone(), removing.clone()).await.unwrap();
    assert_eq!(preview.files[0].after, CONFIG);
    let writing = wire::WriteScheduleRequest { write: true, expected_digest: Some(preview.digest), ..removing };
    write_schedule(api, writing).await.unwrap();
    assert_eq!(fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap(), CONFIG);
}

#[tokio::test]
async fn an_export_previews_the_workflow_at_the_engines_path_then_writes_it_once() {
    let (dir, api) = project();
    let preview = write_schedule(api.clone(), putting(nightly())).await.unwrap();
    let writing =
        wire::WriteScheduleRequest { write: true, expected_digest: Some(preview.digest), ..putting(nightly()) };
    write_schedule(api.clone(), writing).await.unwrap();
    let request = wire::ExportScheduleRequest { id: "nightly".into(), ..wire::ExportScheduleRequest::default() };
    let preview = export_schedule(api.clone(), request.clone()).await.unwrap();
    let path = ".github/workflows/vibranium-nightly.yml";
    assert_eq!((preview.path.as_str(), preview.files[0].path.as_str()), (path, path));
    assert!(preview.files[0].after.contains("    - cron: 0 2 * * *\n"), "{preview:?}");
    assert!(!dir.path().join(path).exists(), "a preview writes nothing");
    let writing = wire::ExportScheduleRequest { write: true, expected_digest: Some(preview.digest), ..request.clone() };
    export_schedule(api.clone(), writing).await.unwrap();
    assert_eq!(fs::read_to_string(dir.path().join(path)).unwrap(), preview.files[0].after);
    assert_eq!(
        export_schedule(api.clone(), request).await.unwrap_err().code,
        Code::InvalidArgument,
        "it is there already"
    );
    let unknown = wire::ExportScheduleRequest { id: "../nightly".into(), ..wire::ExportScheduleRequest::default() };
    let refused = export_schedule(api, unknown).await.unwrap_err();
    assert_eq!(refused.code, Code::InvalidArgument);
    assert!(!refused.message.contains(".."), "a client's id is not echoed: {refused:?}");
}
