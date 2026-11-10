use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use super::{confine, creatable, entry, outside, uncreatable};
use crate::confine::{LAUNCHER_ARG, Policy};

/// Whether the kernel enforces Landlock here; the tests that rely on it skip otherwise.
fn landlock() -> bool {
    std::fs::read_to_string("/sys/kernel/security/lsm")
        .is_ok_and(|lsms| lsms.split(',').any(|lsm| lsm.trim() == "landlock"))
}

/// What `script` did, run by `sh` in `root` from a thread confined to `policy`.
fn confined(policy: &Policy, root: &Path, script: &str) -> Output {
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                confine(policy).unwrap();
                Command::new("/bin/sh").args(["-c", script]).current_dir(root).output().unwrap()
            })
            .join()
            .unwrap()
    })
}

#[test]
fn the_launcher_runs_the_plugin_with_the_policy_as_json() {
    let policy = Policy::closed(Path::new("/p"), Path::new("/p/.vibranium/plugins/a"), Path::new("/tmp"));
    let plugin = (PathBuf::from("python3"), vec![OsString::from("main.py")]);
    let (program, args) = entry(&policy, Some(Path::new("/usr/bin/vc")), plugin.clone()).unwrap();
    assert_eq!(program, PathBuf::from("/usr/bin/vc"));
    assert_eq!(args[0], LAUNCHER_ARG);
    assert_eq!(serde_json::from_str::<Policy>(args[1].to_str().unwrap()).unwrap(), policy);
    assert_eq!(&args[2..], [OsString::from("python3"), OsString::from("main.py")]);
    assert_eq!(entry(&policy, None, plugin.clone()).unwrap(), plugin);
}

#[test]
fn outside_is_every_entry_beside_the_way_to_the_root() {
    let tree = tempfile::tempdir().unwrap();
    let root = tree.path().join("work/project");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(tree.path().join("work/other")).unwrap();
    std::fs::write(tree.path().join("notes.txt"), "").unwrap();
    let found = outside(&root);
    assert!(found.contains(&tree.path().join("work/other")), "{found:?}");
    assert!(found.contains(&tree.path().join("notes.txt")), "{found:?}");
    assert!(!found.iter().any(|path| root.starts_with(path)), "{found:?}");
}

#[test]
fn outside_leaves_out_an_entry_that_links_back_to_the_project() {
    let tree = tempfile::tempdir().unwrap();
    let tree = tree.path().canonicalize().unwrap();
    let root = tree.join("dev/project");
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(tree.join("dev/other")).unwrap();
    for (link, to) in [("dev/work", root.clone()), ("dev/code", root.join("src")), ("home", tree.join("dev"))] {
        std::os::unix::fs::symlink(to, tree.join(link)).unwrap();
    }
    let found = outside(&root);
    assert!(found.contains(&tree.join("dev/other")), "{found:?}");
    for link in ["dev/work", "dev/code", "home"] {
        assert!(!found.contains(&tree.join(link)), "{link} leads into the project: {found:?}");
    }
}

#[test]
fn a_confined_process_reads_and_writes_only_what_its_policy_opens() {
    if !landlock() {
        return eprintln!("skipped: the kernel does not enforce Landlock");
    }
    let tree = tempfile::tempdir().unwrap();
    let root = tree.path().join("project");
    let (plugin, data) = (root.join("plugin"), root.join("data"));
    for dir in [&plugin, &data] {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(root.join("secret.txt"), "s3cr3t").unwrap();
    std::fs::write(plugin.join("main.sh"), "echo plugin").unwrap();
    let policy = Policy::closed(&root, &plugin, &data);
    let script = "cat plugin/main.sh && echo ok > data/out.txt; cat secret.txt; echo no > leaked.txt";
    let ran = confined(&policy, &root, script);
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "plugin\n");
    assert_eq!(std::fs::read_to_string(data.join("out.txt")).unwrap(), "ok\n");
    assert!(!root.join("leaked.txt").exists());
}

#[test]
fn a_file_not_made_yet_is_made_in_its_nearest_folder_inside_the_project_but_the_root() {
    let tree = tempfile::tempdir().unwrap();
    let root = tree.path().join("project");
    std::fs::create_dir_all(root.join("reports")).unwrap();
    let missing = [root.join("reports/summary.json"), root.join("reports/2026/day.json"), root.join("out.json")];
    let missing: Vec<&PathBuf> = missing.iter().collect();
    assert_eq!(creatable(&root, &missing), [root.join("reports"), root.join("reports")]);
    assert_eq!(creatable(&root, &[&tree.path().join("elsewhere/x")]), Vec::<PathBuf>::new());
}

#[test]
fn a_file_not_made_yet_at_the_top_of_the_project_cannot_be_made() {
    let tree = tempfile::tempdir().unwrap();
    let root = tree.path().to_path_buf();
    std::fs::create_dir_all(root.join("reports")).unwrap();
    std::fs::write(root.join("kept.json"), "{}").unwrap();
    let write = ["out.json", "reports/summary.json", "kept.json"].map(|file| root.join(file)).to_vec();
    let policy = Policy { root: root.clone(), read: vec![], write, network: false };
    assert_eq!(uncreatable(&policy), [root.join("out.json")]);
}

#[test]
fn a_confined_process_makes_a_file_it_may_write_that_does_not_exist_yet() {
    if !landlock() {
        return eprintln!("skipped: the kernel does not enforce Landlock");
    }
    let tree = tempfile::tempdir().unwrap();
    let root = tree.path().join("project");
    std::fs::create_dir_all(root.join("reports")).unwrap();
    std::fs::write(root.join("reports/old.json"), "old").unwrap();
    let policy =
        Policy { root: root.clone(), read: vec![], write: vec![root.join("reports/summary.json")], network: false };
    let script = "echo ok > reports/summary.json; cat reports/old.json; rm reports/old.json; echo no > leaked.txt";
    let ran = confined(&policy, &root, script);
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "");
    assert_eq!(std::fs::read_to_string(root.join("reports/summary.json")).unwrap(), "ok\n");
    assert!(root.join("reports/old.json").exists());
    assert!(!root.join("leaked.txt").exists());
}
