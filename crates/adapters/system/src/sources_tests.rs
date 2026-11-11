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
fn ignores_what_the_walk_leaves_out_whether_or_not_it_is_there() {
    let dir = project(&[
        (".gitignore", b"generated/\n*.local.api.yaml\n"),
        ("generated/x.api.yaml", b""),
        ("users.local.api.yaml", b""),
        ("users.api.yaml", b""),
        ("services/.ignore", b"!keep.local.api.yaml\n"),
    ]);
    let sources = FsSources::new(dir.path()).unwrap();
    let ignores = |path: &str| sources.ignores(&SourcePath::new(path).unwrap());
    let left_out = ["generated/x.api.yaml", "users.local.api.yaml", "generated/new/y.api.yaml", "new.local.api.yaml"];
    for path in left_out.into_iter().chain(["services/new/z.local.api.yaml"]) {
        assert!(ignores(path), "{path} is left out");
    }
    for path in ["users.api.yaml", "services/new/z.api.yaml", "services/keep.local.api.yaml", "fresh.test.yaml"] {
        assert!(!ignores(path), "{path} is discovered");
    }
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

#[test]
fn a_refused_save_into_a_new_folder_leaves_no_folder() {
    let dir = project(&[]);
    let sources = FsSources::new(dir.path()).unwrap();
    let was = Some(vc_interfaces::content_hash(b"tests: []\n"));
    let refused = sources.write_unchanged(&SourcePath::new("services/new/new.test.yaml").unwrap(), "x\n", was);
    let changed = SourceError::Changed { path: "services/new/new.test.yaml".to_owned() };
    assert_eq!(refused, Err(changed));
    assert!(!dir.path().join("services").exists(), "no folder is created for a refused save");
}

#[cfg(unix)]
mod unix {
    use std::fs;
    use std::os::unix::fs::{PermissionsExt, symlink};

    use vc_interfaces::{SourceError, SourceProvider, SourceWriter};
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
    fn ignores_every_path_under_a_link_to_a_folder_there_or_not_yet() {
        let dir = project(&[(".gitignore", b"generated/\n"), ("generated/x.api.yaml", b""), ("real/y.api.yaml", b"")]);
        symlink(dir.path().join("generated"), dir.path().join("vendor")).unwrap();
        symlink(dir.path().join("real"), dir.path().join("mirror")).unwrap();
        let sources = FsSources::new(dir.path()).unwrap();
        let ignores = |path: &str| sources.ignores(&SourcePath::new(path).unwrap());
        for path in ["vendor/new.test.yaml", "vendor/x.api.yaml", "vendor/a/b/new.test.yaml", "mirror/y.api.yaml"] {
            assert!(ignores(path), "{path} is under a link discovery does not follow");
        }
        assert!(!ignores("real/new.test.yaml"));
    }

    #[test]
    fn reads_a_data_file_outside_the_root_only_while_it_stays_under_its_allowed_folder() {
        let space = project(&[
            ("project/vibranium.config.yaml", b""),
            ("shared/users.csv", b"id\n1\n"),
            ("shared/.private/keys.csv", b"key\n"),
            ("other/x.csv", b"x\n"),
        ]);
        symlink(space.path().join("other/x.csv"), space.path().join("shared/escape.csv")).unwrap();
        let sources = FsSources::new(&space.path().join("project")).unwrap();
        let data = |path: &str| SourcePath::data(path).unwrap();
        let names = ["../shared/users.csv", "../shared/.private/keys.csv", "../shared/escape.csv", "../other/x.csv"];
        let mut wanted: Vec<(SourcePath, SourcePath)> =
            names.iter().map(|path| (data(path), data("../shared"))).collect();
        wanted.push((data("../shared/none.csv"), data("../shared")));
        wanted.push((data(names[0]), data("../shared")));
        let found = sources.read_outside(&wanted).unwrap();
        assert_eq!(paths(&found), ["../shared/users.csv"], "a pair asked for twice is read once");
        assert_eq!(found.files[0].text.as_str(), "id\n1\n");
        let outside = |path: &str| SourceError::OutsideRoot { path: path.to_owned() };
        let missing = SourceError::Missing { path: "../shared/none.csv".to_owned() };
        let expected = [outside("../other/x.csv"), outside(names[1]), outside(names[2]), missing];
        assert_eq!(found.problems, expected);
        let plain = sources.read(&[data(names[0])]).unwrap();
        assert_eq!(plain.problems, [outside(names[0])], "a plain read never leaves the root");
    }

    #[test]
    fn an_outside_read_never_lands_inside_the_project_even_through_an_overlapping_root() {
        let space =
            project(&[("project/.env", b"KEY=k\n"), ("project/.vibranium/history.db", b"x"), ("shared/a.csv", b"")]);
        symlink(space.path().join("project/.env"), space.path().join("shared/env.csv")).unwrap();
        let sources = FsSources::new(&space.path().join("project")).unwrap();
        let data = |path: &str| SourcePath::data(path).unwrap();
        let pairs = [
            ("../shared/env.csv", "../shared"),
            ("../project/.env", "../project"),
            ("../project/.vibranium/history.db", "../project/.vibranium"),
        ];
        let wanted: Vec<_> = pairs.iter().map(|(path, root)| (data(path), data(root))).collect();
        let found = sources.read_outside(&wanted).unwrap();
        assert!(found.files.is_empty(), "{:?}", paths(&found));
        let refused = pairs.map(|(path, _)| SourceError::OutsideRoot { path: path.to_owned() });
        assert_eq!(found.problems.len(), refused.len());
        assert!(refused.iter().all(|problem| found.problems.contains(problem)), "{:?}", found.problems);
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
            let unchanged = sources.write_unchanged(&SourcePath::new(path).unwrap(), "changed\n", None);
            assert_eq!(unchanged, Err(SourceError::OutsideRoot { path: path.to_owned() }), "{path}");
        }
        assert_eq!(fs::read_to_string(outside.path().join("services/x.api.yaml")).unwrap(), "kept\n");
        assert!(!outside.path().join("services/new").exists());
        let removed = sources.remove(&SourcePath::new("services/x.api.yaml").unwrap());
        assert_eq!(removed, Err(SourceError::OutsideRoot { path: "services/x.api.yaml".to_owned() }));
        assert!(outside.path().join("services/x.api.yaml").exists());
    }

    #[test]
    fn a_link_into_a_hidden_file_or_folder_is_neither_read_nor_written() {
        let dir = project(&[(".env", b"TOKEN=kept\n"), (".vibranium/x.api.yaml", b"kept\n"), (".env.schema", b"A=\n")]);
        symlink(dir.path().join(".env"), dir.path().join("env.api.yaml")).unwrap();
        symlink(dir.path().join(".vibranium"), dir.path().join("cache")).unwrap();
        let sources = FsSources::new(dir.path()).unwrap();
        let read = |path: &str| sources.read(&[SourcePath::new(path).unwrap()]).unwrap();
        let outside = |path: &str| SourceError::OutsideRoot { path: path.to_owned() };
        assert_eq!(read("env.api.yaml").problems, [outside("env.api.yaml")]);
        assert_eq!(read("cache/x.api.yaml").problems, [outside("cache/x.api.yaml")]);
        assert_eq!(read(".env.schema").files.len(), 1, "a hidden path named as such is still read");
        for path in ["env.api.yaml", "cache/x.api.yaml", "cache/new.api.yaml"] {
            assert_eq!(sources.write(&SourcePath::new(path).unwrap(), "changed\n"), Err(outside(path)), "{path}");
            let unchanged = sources.write_unchanged(&SourcePath::new(path).unwrap(), "changed\n", None);
            assert_eq!(unchanged, Err(outside(path)), "{path}");
        }
        assert_eq!(fs::read_to_string(dir.path().join(".env")).unwrap(), "TOKEN=kept\n");
        assert_eq!(fs::read_to_string(dir.path().join(".vibranium/x.api.yaml")).unwrap(), "kept\n");
        assert!(!dir.path().join(".vibranium/new.api.yaml").exists());
    }

    #[test]
    fn a_hidden_name_linked_to_a_dotenv_file_is_neither_read_nor_written() {
        let dir = project(&[(".env", b"TOKEN=kept\n")]);
        symlink(dir.path().join(".env"), dir.path().join(".env.schema")).unwrap();
        let sources = FsSources::new(dir.path()).unwrap();
        let path = SourcePath::new(".env.schema").unwrap();
        let outside = || SourceError::OutsideRoot { path: ".env.schema".to_owned() };
        assert_eq!(sources.read(std::slice::from_ref(&path)).unwrap().problems, [outside()]);
        assert_eq!(sources.write(&path, "A=\n"), Err(outside()));
        assert_eq!(sources.write_unchanged(&path, "A=\n", None), Err(outside()));
        assert_eq!(fs::read_to_string(dir.path().join(".env")).unwrap(), "TOKEN=kept\n");
    }

    #[test]
    fn a_hidden_name_linked_to_a_secrets_schema_in_a_hidden_folder_is_read_and_written() {
        let dir = project(&[(".config/env.schema", b"A=\n")]);
        symlink(dir.path().join(".config/env.schema"), dir.path().join(".env.schema")).unwrap();
        let sources = FsSources::new(dir.path()).unwrap();
        let schema = SourcePath::new(".env.schema").unwrap();
        let read = sources.read(std::slice::from_ref(&schema)).unwrap();
        assert_eq!((read.files[0].text.as_str(), read.problems.len()), ("A=\n", 0));
        assert_eq!(sources.write(&schema, "A=\nB=\n"), Ok(()));
        assert_eq!(fs::read_to_string(dir.path().join(".config/env.schema")).unwrap(), "A=\nB=\n");
    }

    #[test]
    fn a_hidden_name_linked_to_any_other_hidden_file_is_neither_read_nor_written() {
        let kept: [(&str, &[u8]); 5] = [
            (".git/config", b"kept\n"),
            (".git/env.schema", b"kept\n"),
            (".vibranium/cache.db", b"kept\n"),
            (".npmrc", b"kept\n"),
            (".env", b"kept\n"),
        ];
        for (target, _) in kept {
            let dir = project(&kept);
            symlink(dir.path().join(target), dir.path().join(".env.schema")).unwrap();
            let sources = FsSources::new(dir.path()).unwrap();
            let path = SourcePath::new(".env.schema").unwrap();
            let outside = || SourceError::OutsideRoot { path: ".env.schema".to_owned() };
            assert_eq!(sources.read(std::slice::from_ref(&path)).unwrap().problems, [outside()], "{target}");
            assert_eq!(sources.write(&path, "A=\n"), Err(outside()), "{target}");
            assert_eq!(sources.write_unchanged(&path, "A=\n", None), Err(outside()), "{target}");
            assert_eq!(fs::read_to_string(dir.path().join(target)).unwrap(), "kept\n", "{target}");
        }
    }

    #[test]
    fn an_unchanged_write_goes_through_a_link_inside_the_root_and_keeps_the_mode() {
        let dir = project(&[("real/users.api.yaml", b"apis: []\n")]);
        let real = dir.path().join("real/users.api.yaml");
        fs::set_permissions(&real, fs::Permissions::from_mode(0o640)).unwrap();
        symlink(&real, dir.path().join("linked.api.yaml")).unwrap();
        let sources = FsSources::new(dir.path()).unwrap();
        let was = Some(vc_interfaces::content_hash(b"apis: []\n"));
        assert_eq!(sources.write_unchanged(&SourcePath::new("linked.api.yaml").unwrap(), "apis: {}\n", was), Ok(()));
        assert_eq!(fs::read_to_string(&real).unwrap(), "apis: {}\n");
        assert!(fs::symlink_metadata(dir.path().join("linked.api.yaml")).unwrap().file_type().is_symlink());
        assert_eq!(fs::metadata(&real).unwrap().permissions().mode() & 0o777, 0o640);
        fs::set_permissions(&real, fs::Permissions::from_mode(0o440)).unwrap();
        let was = Some(vc_interfaces::content_hash(b"apis: {}\n"));
        let read_only = sources.write_unchanged(&SourcePath::new("real/users.api.yaml").unwrap(), "x\n", was);
        assert!(matches!(read_only, Err(SourceError::Write { .. })), "{read_only:?}");
        fs::set_permissions(&real, fs::Permissions::from_mode(0o640)).unwrap();
        let stale = sources.write_unchanged(&SourcePath::new("real/users.api.yaml").unwrap(), "x\n", None);
        assert!(matches!(stale, Err(SourceError::Changed { .. })), "{stale:?}");
        let left: Vec<_> =
            fs::read_dir(dir.path().join("real")).unwrap().map(|entry| entry.unwrap().file_name()).collect();
        assert_eq!(left, ["users.api.yaml"], "no staged file is left behind");
    }

    #[test]
    fn a_root_reached_through_a_link_is_resolved() {
        let dir = project(&[("real/users.api.yaml", b"")]);
        symlink(dir.path().join("real"), dir.path().join("alias")).unwrap();
        let sources = FsSources::new(&dir.path().join("alias")).unwrap();
        assert_eq!(sources.root(), fs::canonicalize(dir.path().join("real")).unwrap());
        assert_eq!(paths(&discover(&dir.path().join("alias"))), ["users.api.yaml"]);
    }

    #[test]
    fn a_file_s_real_path_follows_its_link_and_is_none_when_nothing_is_there() {
        let dir = project(&[("specs/users.yaml", b"openapi: 3.1.0\n")]);
        fs::create_dir_all(dir.path().join("openapi")).unwrap();
        symlink(dir.path().join("specs/users.yaml"), dir.path().join("openapi/users.yaml")).unwrap();
        let sources = FsSources::new(dir.path()).unwrap();
        let real = |path: &str| sources.real(&SourcePath::new(path).unwrap()).map(|real| real.to_string());
        assert_eq!(real("openapi/users.yaml").as_deref(), Some("specs/users.yaml"));
        assert_eq!(real("specs/users.yaml").as_deref(), Some("specs/users.yaml"));
        assert_eq!(real("openapi/orders.yaml"), None);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn a_file_s_real_path_is_in_the_case_the_disk_stores() {
    let dir = project(&[("openapi/users.yaml", b"openapi: 3.1.0\n")]);
    let sources = FsSources::new(dir.path()).unwrap();
    let real = sources.real(&SourcePath::new("openapi/Users.yaml").unwrap()).map(|real| real.to_string());
    assert_eq!(real.as_deref(), Some("openapi/users.yaml"));
}
