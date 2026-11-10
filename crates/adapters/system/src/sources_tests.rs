use std::fs;
use std::path::Path;

use tempfile::TempDir;
use vc_interfaces::{Discovery, SourceError, SourceFilter, SourceProvider, SourceWriter};
use vc_model::SourcePath;

use super::FsSources;

fn project(files: &[(&str, &[u8])]) -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, bytes) in files {
        let file = dir.path().join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, bytes).unwrap();
    }
    dir
}

fn discover(root: &Path) -> Discovery {
    FsSources::new(root).unwrap().discover(&SourceFilter::default()).unwrap()
}

fn paths(found: &Discovery) -> Vec<&str> {
    found.files.iter().map(|file| file.path.as_str()).collect()
}

#[test]
fn meets_the_source_provider_contract() {
    let mut dirs = Vec::new();
    vc_testkit::source_provider_suite(&mut |files| {
        let dir = project(files);
        let sources = FsSources::new(dir.path()).unwrap();
        dirs.push(dir);
        Box::new(sources)
    });
    assert!(!dirs.is_empty(), "the suite built providers");
}

#[test]
fn respects_gitignore_outside_a_repository() {
    let dir = project(&[
        (".gitignore", b"generated/\n*.local.api.yaml\n"),
        ("generated/x.api.yaml", b""),
        ("users.local.api.yaml", b""),
        ("users.api.yaml", b""),
    ]);
    assert_eq!(paths(&discover(dir.path())), ["users.api.yaml"]);
}

#[test]
fn a_missing_root_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("missing");
    assert!(matches!(FsSources::new(&missing), Err(SourceError::Root { .. })));
    let gone = dir.path().join("gone");
    fs::create_dir(&gone).unwrap();
    let sources = FsSources::new(&gone).unwrap();
    fs::remove_dir(&gone).unwrap();
    assert!(matches!(sources.discover(&SourceFilter::default()), Err(SourceError::Root { .. })));
}

#[test]
fn a_folder_read_as_a_file_is_named_as_one() {
    let dir = project(&[("bruno/shop/carts.bru", b"")]);
    let found = FsSources::new(dir.path()).unwrap().read(&[SourcePath::new("bruno/shop").unwrap()]).unwrap();
    let reason = "it is a folder, not a file".to_owned();
    assert_eq!(found.problems, [SourceError::Read { path: "bruno/shop".to_owned(), reason }]);
}

#[test]
fn meets_the_source_writer_contract() {
    let mut dirs = Vec::new();
    vc_testkit::source_writer_suite(&mut |files| {
        let dir = project(files);
        let writer = FsSources::new(dir.path()).unwrap();
        let reader = FsSources::new(dir.path()).unwrap();
        dirs.push(dir);
        (Box::new(writer), Box::new(reader))
    });
    assert!(!dirs.is_empty(), "the suite built writers");
}

#[test]
fn a_write_the_system_refuses_names_its_file() {
    let dir = project(&[("taken", b"a file, not a folder")]);
    let sources = FsSources::new(dir.path()).unwrap();
    let refused = sources.write(&SourcePath::new("taken/x.api.yaml").unwrap(), "");
    assert!(matches!(refused, Err(SourceError::Write { path, .. }) if path == "taken/x.api.yaml"));
}

#[cfg(unix)]
mod unix {
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};

    use vc_interfaces::{SourceError, SourceWriter};
    use vc_model::SourcePath;

    use super::{FsSources, discover, paths, project};

    #[test]
    fn reads_a_link_to_a_file_inside_the_root() {
        let dir = project(&[("real/users.api.yaml", b"apis: {}\n")]);
        symlink(dir.path().join("real/users.api.yaml"), dir.path().join("linked.api.yaml")).unwrap();
        let found = discover(dir.path());
        assert_eq!(paths(&found), ["linked.api.yaml", "real/users.api.yaml"]);
        assert_eq!(found.files[0].text.as_str(), "apis: {}\n");
    }

    #[test]
    fn reports_a_link_to_a_file_outside_the_root_without_reading_it() {
        let outside = project(&[("secret.api.yaml", b"token: hunter2\n")]);
        let dir = project(&[("users.api.yaml", b"")]);
        symlink(outside.path().join("secret.api.yaml"), dir.path().join("escape.api.yaml")).unwrap();
        let found = discover(dir.path());
        assert_eq!(paths(&found), ["users.api.yaml"]);
        assert_eq!(found.problems, [SourceError::OutsideRoot { path: "escape.api.yaml".to_owned() }]);
    }

    #[test]
    fn does_not_follow_links_to_folders() {
        let outside = project(&[("services/x.api.yaml", b"")]);
        let dir = project(&[("users.api.yaml", b"")]);
        symlink(outside.path().join("services"), dir.path().join("services")).unwrap();
        let found = discover(dir.path());
        assert_eq!(paths(&found), ["users.api.yaml"]);
        assert_eq!(found.problems, []);
    }

    #[test]
    fn reports_names_a_source_path_refuses() {
        let dir = project(&[("k:v.api.yaml", b""), ("ok.api.yaml", b"")]);
        let found = discover(dir.path());
        assert_eq!(paths(&found), ["ok.api.yaml"]);
        assert_eq!(found.problems, [SourceError::InvalidPath { path: "k\u{fffd}v.api.yaml".to_owned() }]);
    }

    #[test]
    fn reports_a_file_it_cannot_read() {
        let dir = project(&[("locked.api.yaml", b""), ("ok.api.yaml", b"")]);
        let locked = dir.path().join("locked.api.yaml");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        let found = discover(dir.path());
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(paths(&found), ["ok.api.yaml"]);
        assert!(matches!(&found.problems[..], [SourceError::Read { path, .. }] if path == "locked.api.yaml"));
    }

    #[test]
    fn refuses_to_write_through_a_link_out_of_the_root() {
        let outside = project(&[("services/x.api.yaml", b"kept\n")]);
        let dir = project(&[]);
        symlink(outside.path().join("services"), dir.path().join("services")).unwrap();
        symlink(outside.path().join("services/x.api.yaml"), dir.path().join("x.api.yaml")).unwrap();
        let sources = FsSources::new(dir.path()).unwrap();
        for path in ["services/x.api.yaml", "services/new/y.api.yaml", "x.api.yaml"] {
            let refused = sources.write(&SourcePath::new(path).unwrap(), "changed\n");
            assert_eq!(refused, Err(SourceError::OutsideRoot { path: path.to_owned() }), "{path}");
        }
        assert_eq!(fs::read_to_string(outside.path().join("services/x.api.yaml")).unwrap(), "kept\n");
        assert!(!outside.path().join("services/new").exists());
        let removed = sources.remove(&SourcePath::new("services/x.api.yaml").unwrap());
        assert_eq!(removed, Err(SourceError::OutsideRoot { path: "services/x.api.yaml".to_owned() }));
        assert!(outside.path().join("services/x.api.yaml").exists());
    }

    #[test]
    fn a_root_reached_through_a_link_is_resolved() {
        let dir = project(&[("real/users.api.yaml", b"")]);
        symlink(dir.path().join("real"), dir.path().join("alias")).unwrap();
        let sources = FsSources::new(&dir.path().join("alias")).unwrap();
        assert_eq!(sources.root(), fs::canonicalize(dir.path().join("real")).unwrap());
        assert_eq!(paths(&discover(&dir.path().join("alias"))), ["users.api.yaml"]);
    }
}
