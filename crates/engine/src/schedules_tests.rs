use std::fs;
use std::path::Path;
use std::sync::Arc;

use vc_adapter_system::FsSources;
use vc_interfaces::{HistoryWriter, RunStart};
use vc_model::Date;
use vc_testkit::{FakeVcs, FixedClock, MemoryCompileCache, MemoryEnvironment, MemoryHistory};

use super::{NotifyChannel, ScheduleEdit, ScheduleSpec};
use crate::create::DOCS;
use crate::{Engine, EngineBuilder, EngineError, ScheduleRunner};

const PROFILES: &str = "syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n  - id: staging\n";
const SCHEDULES: &str = concat!(
    "schedules:\n",
    "  # nightly · schedule\n",
    "  - id: nightly\n",
    "    name: Nightly smoke\n",
    "    cron: 0 2 * * *\n",
    "    selector: [users.crud, '#listsUsers']\n",
    "    profile: staging\n",
    "    notify:\n",
    "      - slack: {channel: '#qa', webhook: '${secret.SLACK_HOOK}'}\n",
    "  - id: weekly\n",
    "    cron: 0 6 * * 1\n",
    "    runner: githubActions\n",
    "  - id: paused\n",
    "    cron: '*/5 * * * *'\n",
    "    enabled: false\n",
);

/// A project on disk with `config`, its engine recording runs in `history`; the clock reads 0.
fn project(config: &str) -> (tempfile::TempDir, Engine, Arc<MemoryHistory>) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("vibranium.config.yaml"), config).unwrap();
    let folder = Arc::new(FsSources::new(dir.path()).unwrap());
    let history = Arc::new(MemoryHistory::new());
    let engine = EngineBuilder::new(dir.path().to_path_buf())
        .vcs(Arc::new(FakeVcs::default()))
        .environment(Arc::new(MemoryEnvironment::new(&[])))
        .compile_cache(Arc::new(MemoryCompileCache::new()))
        .clock(Arc::new(FixedClock(Date::new(2026, 11, 27).unwrap())))
        .history(history.clone(), history.clone())
        .sources(folder.clone())
        .writer(folder)
        .build()
        .unwrap();
    (dir, engine, history)
}

fn config(dir: &Path) -> String {
    fs::read_to_string(dir.join("vibranium.config.yaml")).unwrap()
}

fn nightly() -> ScheduleSpec {
    ScheduleSpec {
        id: "nightly".into(),
        name: Some("Nightly smoke".into()),
        cron: "0 2 * * *".into(),
        selector: vec!["users.crud".into(), "#listsUsers".into()],
        profiles: vec!["staging".into()],
        notify: vec![NotifyChannel {
            kind: "slack".into(),
            settings: vec![("channel".into(), "#qa".into()), ("webhook".into(), "${secret.SLACK_HOOK}".into())],
        }],
        runner: ScheduleRunner::VcServer,
        enabled: true,
    }
}

fn reason(error: &EngineError) -> (String, Option<String>) {
    (error.to_string(), error.help())
}

const HOUR: u64 = 3_600_000;

#[test]
fn each_schedule_is_listed_as_written_with_when_the_server_next_starts_it() {
    let (_dir, engine, _) = project(&format!("{PROFILES}{SCHEDULES}"));
    let listed = engine.schedules().unwrap();
    let specs: Vec<&ScheduleSpec> = listed.iter().map(|listing| &listing.spec).collect();
    assert_eq!(specs[0], &nightly(), "the settings stay as written: a secret is only its reference");
    assert_eq!(
        (specs[1].id.as_str(), specs[1].runner, specs[1].enabled),
        ("weekly", ScheduleRunner::GithubActions, true)
    );
    assert_eq!((specs[2].id.as_str(), specs[2].enabled, specs[2].profiles.len()), ("paused", false, 0));
    let next: Vec<Option<u64>> = listed.iter().map(|listing| listing.next_ms).collect();
    assert_eq!(next, [Some(2 * HOUR), None, None], "only an enabled schedule `vc ui` runs has a next start");
    assert!(listed.iter().all(|listing| listing.last.is_none()));
}

#[test]
fn a_schedule_with_a_wrong_cron_is_still_listed_without_a_next_start() {
    let (_dir, engine, _) = project(&format!("{PROFILES}schedules:\n  - id: broken\n    cron: nope\n"));
    let listed = engine.schedules().unwrap();
    assert_eq!((listed[0].spec.cron.as_str(), listed[0].next_ms), ("nope", None));
    assert_eq!(project(PROFILES).1.schedules().unwrap(), []);
}

#[test]
fn a_listing_shows_the_newest_run_that_schedule_started() {
    let (_dir, engine, history) = project(&format!("{PROFILES}{SCHEDULES}"));
    let started = |schedule: Option<&str>, started_ms| RunStart {
        kind: "run".into(),
        started_ms,
        schedule: schedule.map(str::to_owned),
        ..RunStart::default()
    };
    history.begin(&started(Some("nightly"), 10)).unwrap();
    history.begin(&started(Some("nightly"), 20)).unwrap();
    history.begin(&started(None, 30)).unwrap();
    let listed = engine.schedules().unwrap();
    let last = listed[0].last.as_ref().unwrap();
    assert_eq!((last.start.started_ms, last.start.schedule.as_deref()), (20, Some("nightly")));
    assert!(listed[1].last.is_none(), "a run by hand belongs to no schedule");
}

#[test]
fn a_new_schedule_is_added_under_its_comment_and_reads_back_the_same() {
    let (dir, engine, _) = project(PROFILES);
    let changes = engine.schedule_write(&ScheduleEdit::Put(nightly())).unwrap();
    assert_eq!(changes.titles, ["Add schedule nightly"]);
    engine.write(&changes).unwrap();
    let written = config(dir.path());
    let expected = concat!(
        "  - id: nightly\n",
        "    name: Nightly smoke\n",
        "    cron: 0 2 * * *\n",
        "    selector: [users.crud, \"#listsUsers\"]\n",
        "    profile: staging\n",
        "    notify:\n",
        "      - slack:\n",
        "          channel: \"#qa\"\n",
        "          webhook: ${secret.SLACK_HOOK}\n",
    );
    let comment = format!("schedules:\n  # nightly · schedule · keys: {DOCS}#schedules\n");
    assert_eq!(written, format!("{PROFILES}{comment}{expected}"));
    assert_eq!(engine.schedules().unwrap()[0].spec, nightly());
}

#[test]
fn a_schedule_put_again_is_changed_in_place_and_the_others_stay() {
    let (dir, engine, _) = project(&format!("{PROFILES}{SCHEDULES}"));
    let paused =
        ScheduleSpec { cron: "*/10 * * * *".into(), enabled: true, ..engine.schedules().unwrap()[2].spec.clone() };
    let changes = engine.schedule_write(&ScheduleEdit::Put(paused.clone())).unwrap();
    assert_eq!(changes.titles, ["Change schedule paused"]);
    engine.write(&changes).unwrap();
    let written = config(dir.path());
    assert!(
        written.starts_with(&format!("{PROFILES}{}", &SCHEDULES[..SCHEDULES.find("  - id: paused").unwrap()])),
        "{written}"
    );
    assert!(written.ends_with("  - id: paused\n    cron: '*/10 * * * *'\n    enabled: true\n"), "{written}");
    let listed = engine.schedules().unwrap();
    assert_eq!((listed.len(), &listed[2].spec), (3, &paused));
}

const KEPT: &str = concat!(
    "schedules:\n",
    "  # nightly · schedule\n",
    "  - id: nightly\n",
    "    name: Nightly smoke  # shown in the Runs space\n",
    "    cron: '0 2 * * *'\n",
    "    selector: [users.crud, '#listsUsers']\n",
    "    profile: staging\n",
    "    notify:\n",
    "      - webhook:\n",
    "          url: https://hooks.example/qa\n",
    "          headers:\n",
    "            X-Team: qa\n",
    "    owner: qa-team\n",
    "  - id: weekly\n",
    "    cron: '0 6 * * 1'  # Mondays\n",
    "    runner: gitlabCi\n",
);

/// `config` after one put of the listed schedule `at` as `change` makes it.
fn put_back(config_text: &str, at: usize, change: impl Fn(&mut ScheduleSpec)) -> String {
    let (dir, engine, _) = project(&format!("{PROFILES}{config_text}"));
    let mut spec = engine.schedules().unwrap()[at].spec.clone();
    change(&mut spec);
    let changes = engine.schedule_write(&ScheduleEdit::Put(spec)).unwrap();
    engine.write(&changes).unwrap();
    config(dir.path()).strip_prefix(PROFILES).unwrap().to_owned()
}

#[test]
fn a_toggle_changes_only_the_enabled_line_and_keeps_comments_keys_and_a_mapped_setting() {
    let off = put_back(KEPT, 0, |spec| spec.enabled = false);
    assert_eq!(off, KEPT.replace("    owner: qa-team\n", "    owner: qa-team\n    enabled: false\n"));
    let on = put_back(&off, 0, |spec| spec.enabled = true);
    assert_eq!(on, KEPT.replace("    owner: qa-team\n", "    owner: qa-team\n    enabled: true\n"));
}

#[test]
fn a_changed_cron_is_set_in_place_and_an_unknown_runner_stays() {
    let changed = put_back(KEPT, 1, |spec| spec.cron = "0 7 * * 1".into());
    assert_eq!(changed, KEPT.replace("'0 6 * * 1'  # Mondays", "'0 7 * * 1'  # Mondays"));
}

#[test]
fn a_changed_setting_is_set_in_place_and_a_new_channel_goes_last() {
    let changed = put_back(KEPT, 0, |spec| {
        spec.notify[0].settings = vec![("url".into(), "https://hooks.example/ops".into())];
        spec.notify.push(NotifyChannel { kind: "slack".into(), settings: vec![("channel".into(), "#qa".into())] });
        spec.selector = vec!["users.crud".into()];
        spec.name = None;
    });
    let expected = KEPT
        .replace("hooks.example/qa", "hooks.example/ops")
        .replace("            X-Team: qa\n", "            X-Team: qa\n      - slack:\n          channel: \"#qa\"\n")
        .replace("    name: Nightly smoke  # shown in the Runs space\n", "")
        .replace("[users.crud, '#listsUsers']", "users.crud");
    assert_eq!(changed, expected);
}

#[test]
fn a_channel_added_first_or_removed_moves_only_its_own_lines() {
    let two = put_back(KEPT, 0, |spec| spec.notify.insert(0, NotifyChannel { kind: "slack".into(), settings: vec![] }));
    assert_eq!(two, KEPT.replace("    notify:\n", "    notify:\n      - slack: {}\n"));
    let back = put_back(&two, 0, |spec| {
        spec.notify.remove(0);
    });
    assert_eq!(back, KEPT);
}

#[test]
fn a_flow_channel_and_lists_are_changed_in_place() {
    let changed = put_back(SCHEDULES, 0, |spec| {
        spec.notify[0].settings[0].1 = "#ops".into();
        spec.notify[0].settings.push(("mention".into(), "here".into()));
        spec.selector.push("orders".into());
        spec.profiles = vec!["staging".into(), "dev".into()];
    });
    let expected = SCHEDULES
        .replace("'#qa', webhook: '${secret.SLACK_HOOK}'}", "'#ops', webhook: '${secret.SLACK_HOOK}', mention: here}")
        .replace("'#listsUsers']", "\"#listsUsers\", orders]")
        .replace("    profile: staging\n", "    profile: [staging, dev]\n");
    assert_eq!(changed, expected);
}

#[test]
fn a_removed_schedule_leaves_the_others_and_the_last_one_takes_the_list_with_it() {
    let (dir, engine, _) = project(&format!("{PROFILES}{SCHEDULES}"));
    for id in ["nightly", "weekly"] {
        let changes = engine.schedule_write(&ScheduleEdit::Remove(id.into())).unwrap();
        assert_eq!(changes.titles, [format!("Remove schedule {id}")]);
        engine.write(&changes).unwrap();
    }
    let written = config(dir.path());
    assert!(
        written.ends_with("schedules:\n  - id: paused\n    cron: '*/5 * * * *'\n    enabled: false\n"),
        "{written}"
    );
    assert!(!written.contains("# nightly"), "its comment goes with it");
    engine.write(&engine.schedule_write(&ScheduleEdit::Remove("paused".into())).unwrap()).unwrap();
    assert_eq!(config(dir.path()), PROFILES);
}

#[test]
fn removing_a_schedule_that_is_not_there_names_the_close_ones() {
    let (_dir, engine, _) = project(&format!("{PROFILES}{SCHEDULES}"));
    let error = engine.schedule_write(&ScheduleEdit::Remove("nightlly".into())).unwrap_err();
    assert_eq!(
        reason(&error),
        ("there is no such schedule in the config".to_owned(), Some("did you mean `nightly`?".to_owned()))
    );
}

#[test]
fn the_compile_guard_refuses_a_wrong_cron_or_an_unknown_profile() {
    let (dir, engine, _) = project(PROFILES);
    let cron = ScheduleSpec { cron: "0 25 * * *".into(), ..nightly() };
    let (refused, _) = reason(&engine.schedule_write(&ScheduleEdit::Put(cron)).unwrap_err());
    assert!(refused.contains("error[V2317] at vibranium.config.yaml"), "{refused}");
    let profile = ScheduleSpec { profiles: vec!["prod".into()], ..nightly() };
    let (refused, _) = reason(&engine.schedule_write(&ScheduleEdit::Put(profile)).unwrap_err());
    assert!(refused.contains("error[V2002]"), "{refused}");
    assert_eq!(config(dir.path()), PROFILES, "nothing was written");
}

#[test]
fn an_id_kind_or_setting_key_that_is_not_a_word_is_refused_without_echoing_it() {
    let (_dir, engine, _) = project(PROFILES);
    let id = ScheduleSpec { id: "x: [evil]".into(), ..nightly() };
    let kind = ScheduleSpec { notify: vec![NotifyChannel { kind: "a: b".into(), settings: vec![] }], ..nightly() };
    let key = ScheduleSpec {
        notify: vec![NotifyChannel { kind: "slack".into(), settings: vec![("x:\n  y".into(), "z".into())] }],
        ..nightly()
    };
    for spec in [id, kind, key] {
        let (refused, help) = reason(&engine.schedule_write(&ScheduleEdit::Put(spec)).unwrap_err());
        assert!(!refused.contains("evil") && !refused.contains("a: b") && !refused.contains("x:"), "{refused}");
        assert!(help.is_some());
    }
}

#[test]
fn a_text_yaml_would_read_otherwise_is_quoted() {
    let (dir, engine, _) = project(PROFILES);
    let spec = ScheduleSpec {
        id: "odd".into(),
        name: Some("true".into()),
        cron: "*/15 * * * *".into(),
        selector: vec!["#a, b".into()],
        notify: vec![NotifyChannel { kind: "webhook".into(), settings: vec![] }],
        enabled: false,
        ..ScheduleSpec::default()
    };
    engine.write(&engine.schedule_write(&ScheduleEdit::Put(spec.clone())).unwrap()).unwrap();
    let written = config(dir.path());
    assert!(
        written.ends_with(concat!(
            "  - id: odd\n    name: \"true\"\n    cron: \"*/15 * * * *\"\n    selector: \"#a, b\"\n",
            "    notify:\n      - webhook: {}\n    enabled: false\n"
        )),
        "{written}"
    );
    assert_eq!(engine.schedules().unwrap()[0].spec, spec);
}

#[test]
fn the_due_schedules_are_the_enabled_ones_the_server_runs_in_that_minute() {
    let (_dir, engine, _) = project(&format!("{PROFILES}{SCHEDULES}"));
    let due = engine.due_schedules(2 * HOUR).unwrap();
    let [due] = due.as_slice() else { panic!("{due:?}") };
    assert_eq!((due.id.as_str(), due.profiles.as_slice()), ("nightly", ["staging".to_owned()].as_slice()));
    assert_eq!(due.select.ids, ["users.crud", "#listsUsers"]);
    assert_eq!(due.select.paths, Vec::<String>::new());
    assert_eq!(engine.due_schedules(2 * HOUR + 60_000).unwrap(), [], "the next minute is not 02:00");
    assert_eq!(engine.due_schedules(5 * 60_000).unwrap(), [], "`paused` is disabled");
}

#[test]
fn a_selector_word_with_a_slash_is_a_project_path() {
    let config = format!(
        "{PROFILES}schedules:\n  - id: all\n    cron: '* * * * *'\n    selector: [services/users/, users.crud]\n"
    );
    let (_dir, engine, _) = project(&config);
    let due = engine.due_schedules(0).unwrap();
    assert_eq!(
        (due[0].select.paths.as_slice(), due[0].select.ids.as_slice()),
        (["services/users/".to_owned()].as_slice(), ["users.crud".to_owned()].as_slice())
    );
}

#[test]
fn a_project_with_errors_has_no_due_schedules() {
    let (_dir, engine, _) = project(&format!("{PROFILES}schedules:\n  - id: broken\n    cron: nope\n"));
    assert!(engine.due_schedules(0).is_err());
}

#[test]
fn an_export_is_a_new_workflow_at_the_path_the_engine_fixes() {
    let (dir, engine, _) = project(&format!("{PROFILES}{SCHEDULES}"));
    let export = engine.schedule_export("nightly").unwrap();
    assert_eq!(export.path, ".github/workflows/vibranium-nightly.yml");
    let [file] = export.changes.files.as_slice() else { panic!("{:?}", export.changes) };
    assert_eq!((file.path.as_str(), file.before.as_deref()), (export.path.as_str(), None));
    let workflow = &file.after;
    assert_nightly_workflow(workflow);
    engine.write(&export.changes).unwrap();
    assert_eq!(fs::read_to_string(dir.path().join(&export.path)).unwrap(), *workflow);
    let (refused, help) = reason(&engine.schedule_export("nightly").unwrap_err());
    assert_eq!(refused, "`.github/workflows/vibranium-nightly.yml` is there already");
    assert!(help.unwrap().contains("edit that workflow"));
}

/// The workflow exported for `nightly`: its cron, its read-only token, this `vc` and its run.
fn assert_nightly_workflow(workflow: &str) {
    assert!(workflow.starts_with("# The Vibranium schedule `nightly` (Nightly smoke), exported"), "{workflow}");
    assert!(workflow.contains("on:\n  schedule:\n    - cron: 0 2 * * *\n  workflow_dispatch:\n"), "{workflow}");
    assert!(workflow.contains("permissions:\n  contents: read\n"), "{workflow}");
    let install = format!("--tag v{} vc-cli --locked\n", env!("CARGO_PKG_VERSION"));
    assert!(workflow.contains(&install), "{workflow}");
    assert!(workflow.contains("        run: vc run users.crud '#listsUsers' --profile staging\n"), "{workflow}");
}

#[test]
fn exporting_a_schedule_that_is_not_there_names_the_close_ones() {
    let (_dir, engine, _) = project(&format!("{PROFILES}{SCHEDULES}"));
    let (refused, help) = reason(&engine.schedule_export("weakly").unwrap_err());
    assert_eq!(
        (refused.as_str(), help.as_deref()),
        ("there is no such schedule in the config", Some("did you mean `weekly`?"))
    );
}
