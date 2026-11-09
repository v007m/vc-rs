use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use notify::event::{AccessKind, CreateKind, ModifyKind};
use notify::{Event, EventKind};
use vc_interfaces::{FileWatcher, SourceFilter};
use vc_model::SourcePath;
use vc_testkit::file_watcher_suite;

use super::{NotifyWatcher, Saved, debounce};

const ROOT: &str = "/project";
const WAIT: Duration = Duration::from_secs(10);

fn saved() -> Saved {
    Saved { roots: vec![PathBuf::from(ROOT), PathBuf::from("/private/project")], filter: SourceFilter::default() }
}

fn modified(paths: &[&str]) -> Event {
    paths.iter().fold(Event::new(EventKind::Modify(ModifyKind::Any)), |event, path| event.add_path(path.into()))
}

fn texts(paths: &[SourcePath]) -> Vec<&str> {
    paths.iter().map(SourcePath::as_str).collect()
}

#[test]
fn reports_definition_files_and_the_root_env_files() {
    let event = modified(&[
        "/project/services/users/crud.test.yaml",
        "/private/project/services/users/users.api.yaml",
        "/project/vibranium.config.yaml",
        "/project/.env",
        "/project/.env.schema",
    ]);
    let found = saved().of(&event);
    let expected = [
        "services/users/crud.test.yaml",
        "services/users/users.api.yaml",
        "vibranium.config.yaml",
        ".env",
        ".env.schema",
    ];
    assert_eq!(texts(&found), expected);
}

#[test]
fn leaves_out_other_files_skipped_folders_and_paths_outside_the_root() {
    let event = modified(&[
        "/project/notes.md",
        "/project/payloads/user.json",
        "/project/services/.env",
        "/project/.vibranium/reports/x.test.yaml",
        "/project/.git/a.test.yaml",
        "/project/node_modules/b.test.yaml",
        "/elsewhere/c.test.yaml",
        "/project",
    ]);
    assert_eq!(saved().of(&event), Vec::<SourcePath>::new());
}

#[test]
fn reading_a_file_is_not_a_save() {
    let read = Event::new(EventKind::Access(AccessKind::Any)).add_path("/project/a.test.yaml".into());
    assert_eq!(saved().of(&read), Vec::<SourcePath>::new());
    let created = Event::new(EventKind::Create(CreateKind::File)).add_path("/project/a.test.yaml".into());
    assert_eq!(texts(&saved().of(&created)), ["a.test.yaml"]);
}

#[test]
fn a_burst_of_saves_arrives_as_one_batch_in_path_order() {
    let (events, raw) = mpsc::channel();
    let (send, batches) = mpsc::channel();
    let worker = std::thread::spawn(move || debounce(&raw, &send, &saved()));
    events.send(Ok(modified(&["/project/b.test.yaml"]))).unwrap();
    events.send(Ok(modified(&["/project/notes.md"]))).unwrap();
    events.send(Err(notify::Error::generic("lost an event"))).unwrap();
    events.send(Ok(modified(&["/project/a.test.yaml", "/project/b.test.yaml"]))).unwrap();
    assert_eq!(texts(&batches.recv_timeout(WAIT).unwrap()), ["a.test.yaml", "b.test.yaml"]);
    events.send(Ok(modified(&["/project/notes.md"]))).unwrap();
    events.send(Ok(modified(&["/project/c.test.yaml"]))).unwrap();
    assert_eq!(texts(&batches.recv_timeout(WAIT).unwrap()), ["c.test.yaml"]);
    drop(events);
    worker.join().unwrap();
    assert!(batches.recv().is_err(), "no batch after the events stop");
}

#[test]
fn stops_once_the_batches_are_not_wanted() {
    let (events, raw) = mpsc::channel();
    let (send, batches) = mpsc::channel();
    let worker = std::thread::spawn(move || debounce(&raw, &send, &saved()));
    drop(batches);
    events.send(Ok(modified(&["/project/a.test.yaml"]))).unwrap();
    worker.join().unwrap();
    assert!(events.send(Ok(modified(&["/project/a.test.yaml"]))).is_err(), "the watch ended");
}

fn write(root: &Path, path: &str, text: &str) {
    let file = root.join(path);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(file, text).unwrap();
}

#[test]
fn passes_the_file_watcher_suite() {
    let dir = tempfile::tempdir().unwrap();
    let watcher = NotifyWatcher::new(dir.path());
    file_watcher_suite(&watcher, &mut |path, text| write(dir.path(), path, text));
}

#[test]
fn says_why_a_folder_cannot_be_watched() {
    let dir = tempfile::tempdir().unwrap();
    let missing = NotifyWatcher::new(&dir.path().join("missing"));
    assert!(missing.watch(&SourceFilter::default()).is_err());
}
