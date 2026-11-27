use std::fs;
use std::path::{Path, PathBuf};

use crate::{Engine, EngineBuilder, EngineError};

/// A project on disk with no tests, and its engine.
fn project() -> (tempfile::TempDir, Engine) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("vibranium.config.yaml"), "syntaxVersion: 2\n").unwrap();
    let engine = EngineBuilder::for_project(dir.path()).unwrap().build().unwrap();
    (dir, engine)
}

/// The refusal's message, which names the code and not the path.
fn refused(engine: &Engine, out: &str) -> String {
    let Err(error @ EngineError::Reporter { .. }) = engine.client_report_folder(out) else { panic!("{out} is kept") };
    error.to_string()
}

#[test]
fn a_folder_inside_the_project_is_kept_as_it_is_named() {
    let (dir, engine) = project();
    fs::create_dir(dir.path().join("reports")).unwrap();
    for out in ["reports/ci", "./new", "reports"] {
        assert_eq!(engine.client_report_folder(out).unwrap(), PathBuf::from(out));
    }
}

#[test]
fn an_absolute_folder_is_refused_without_echoing_it() {
    let (dir, engine) = project();
    let inside = dir.path().join("reports");
    let message = refused(&engine, &inside.to_string_lossy());
    assert!(message.starts_with("V5132"), "{message}");
    assert!(!message.contains(&*dir.path().to_string_lossy()), "{message}");
    assert!(refused(&engine, "/tmp").starts_with("V5132"));
}

#[test]
fn a_folder_with_a_parent_part_is_refused_even_when_it_comes_back_inside() {
    let (dir, engine) = project();
    let name = dir.path().file_name().unwrap().to_string_lossy();
    for out in ["../elsewhere", "reports/../../elsewhere", &format!("../{name}/reports"), "reports/.."] {
        assert!(refused(&engine, out).starts_with("V5132"), "{out}");
    }
}

#[cfg(unix)]
#[test]
fn a_folder_that_leads_out_through_a_link_is_refused() {
    let (dir, engine) = project();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("linked")).unwrap();
    for out in ["linked", "linked/ci"] {
        assert!(refused(&engine, out).starts_with("V5132"), "{out}");
    }
    std::os::unix::fs::symlink(dir.path().join("reports"), dir.path().join("inner")).unwrap();
    fs::create_dir(dir.path().join("reports")).unwrap();
    assert_eq!(engine.client_report_folder("inner/ci").unwrap(), Path::new("inner/ci"));
}

/// The refusal of `sets`, which names the code and not the folder.
fn sets_refused(engine: &Engine, sets: &[&str]) -> String {
    let sets: Vec<String> = sets.iter().map(|&set| set.to_owned()).collect();
    let Err(error @ EngineError::Reporter { .. }) = engine.client_sets(&sets) else { panic!("{sets:?} are kept") };
    error.to_string()
}

#[test]
fn a_client_reports_dir_set_inside_the_project_is_kept_and_other_sets_are_not_read() {
    let (dir, engine) = project();
    fs::create_dir(dir.path().join("reports")).unwrap();
    let sets = ["reports.dir=reports/ci", " reports.dir = './new' ", "defaults.concurrency=4", "vars.dir=/tmp"];
    engine.client_sets(&sets.map(str::to_owned)).unwrap();
}

#[test]
fn a_client_set_with_no_equals_sign_gives_the_diagnostic_vc_run_gives() {
    let (_dir, engine) = project();
    let sets = ["defaults.concurrency=4".to_owned(), "baseUrl".to_owned()];
    let error = engine.client_sets(&sets).unwrap_err();
    assert!(matches!(&error, EngineError::Setting { origin, .. } if origin == "--set"), "{error:?}");
    let set = vc_adapter_system::SetFlags::new(sets.to_vec());
    let run = crate::settings::ordered(&[&set], &[]).unwrap_err();
    assert_eq!(error.to_string(), run.to_string());
    assert_eq!(error.to_string(), "--set: `baseUrl` is not key=value");
}

#[test]
fn a_client_reports_dir_set_that_is_absolute_or_climbs_is_refused_without_echoing_it() {
    let (dir, engine) = project();
    let inside = format!("reports.dir={}", dir.path().join("reports").display());
    let message = sets_refused(&engine, &["defaults.concurrency=2", &inside]);
    assert!(message.starts_with("V5132") && message.contains("`reports.dir`"), "{message}");
    assert!(!message.contains(&*dir.path().to_string_lossy()), "{message}");
    for set in ["reports.dir=/tmp", "reports.dir=../elsewhere", "reports.dir=\"reports/../..\"", "reports.dir=.."] {
        assert!(sets_refused(&engine, &[set]).starts_with("V5132"), "{set}");
    }
}

#[cfg(unix)]
#[test]
fn a_client_reports_dir_set_that_leads_out_through_a_link_is_refused() {
    let (dir, engine) = project();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join("linked")).unwrap();
    for set in ["reports.dir=linked", "reports.dir=linked/ci"] {
        assert!(sets_refused(&engine, &[set]).starts_with("V5132"), "{set}");
    }
}
