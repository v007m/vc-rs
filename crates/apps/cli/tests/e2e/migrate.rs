//! `vc migrate v1 <path>` turns the v1 shop fixture into a v2 project that checks clean, keeps
//! the credentials out of the files it writes, and lists every `# TODO(vc migrate)` it left in
//! its report (D200).

use std::fs;
use std::path::{Path, PathBuf};

use crate::{Project, said};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../adapters/importers/tests/fixtures/v1/shop");
const TODO: &str = "# TODO(vc migrate): ";

#[test]
fn the_help_and_the_refusal_say_where_the_v1_folder_goes() {
    let project = Project::new(&[]);
    let help = said(&project.vc(&["migrate", "v1", "--help"]).output().unwrap());
    assert!(
        help.contains("which must be a folder without a project") && !help.contains("inside the project."),
        "{help}"
    );
    let outside = project.vc(&["migrate", "v1", ".."]).output().unwrap();
    assert_eq!(outside.status.code(), Some(2), "{}", said(&outside));
    let said = said(&outside);
    assert!(said.contains("where the v2 project is written") && !said.contains("the project /"), "{said}");
    assert!(said.contains("help: move the v1 folder into it"), "{said}");
}

#[test]
fn a_migrated_v1_project_checks_clean_and_reports_every_todo() {
    let project = Project::new(&[]);
    copy(Path::new(FIXTURE), &project.path().join("shop-v1"));
    let output = project.vc(&["migrate", "v1", "shop-v1"]).output().unwrap();
    assert_eq!(output.status.code(), Some(0), "{}", said(&output));
    let check = project.vc(&["check"]).output().unwrap();
    assert_eq!(check.status.code(), Some(0), "{}", said(&check));
    let report = said(&output);
    let written = yaml_files(project.path());
    let todos: Vec<String> = written
        .iter()
        .flat_map(|file| fs::read_to_string(file).unwrap().lines().map(str::to_owned).collect::<Vec<_>>())
        .filter_map(|line| line.trim_start().strip_prefix(TODO).map(str::to_owned))
        .collect();
    assert!(todos.iter().any(|todo| todo.contains("before-endpoint")), "{todos:?}");
    for todo in &todos {
        assert!(report.contains(todo.as_str()), "the report leaves out `{todo}`:\n{report}");
    }
    let schema = fs::read_to_string(project.path().join(".env.schema")).unwrap();
    assert!(schema.lines().any(|line| line == "SHOP_ADMIN_PASSWORD="), "{schema}");
    for file in &written {
        let text = fs::read_to_string(file).unwrap();
        assert!(!text.contains("fixture-admin-password") && !text.contains("fixture-partner-secret"), "{text}");
    }
    let again = project.vc(&["migrate", "v1", "shop-v1"]).output().unwrap();
    assert_eq!(again.status.code(), Some(2), "{}", said(&again));
}

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let path = entry.unwrap().path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() { copy(&path, &target) } else { fs::copy(&path, &target).map(drop).unwrap() }
    }
}

fn yaml_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(yaml_files(&path));
        } else if path.extension().is_some_and(|ext| ext == "yaml") {
            found.push(path);
        }
    }
    found
}
