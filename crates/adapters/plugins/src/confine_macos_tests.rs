use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{SANDBOX_EXEC, entry, profile};
use crate::confine::Policy;

fn policy(root: &Path, read: &Path, write: &Path) -> Policy {
    Policy {
        root: root.to_path_buf(),
        read: vec![read.to_path_buf()],
        write: vec![write.to_path_buf()],
        network: false,
    }
}

/// Whether `sandbox-exec` can apply a profile here: not inside another sandbox, which refuses
/// it. The tests that rely on it skip otherwise.
fn seatbelt() -> bool {
    let tried = Command::new(SANDBOX_EXEC).args(["-p", "(version 1)(allow default)", "/usr/bin/true"]).status();
    tried.is_ok_and(|status| status.success())
}

#[test]
fn the_profile_closes_the_project_then_opens_the_policy_paths() {
    let mut policy = policy(Path::new("/p"), Path::new("/p/plugin"), Path::new("/p/da\"ta"));
    assert_eq!(
        profile(&policy).unwrap(),
        [
            "(version 1)",
            "(allow default)",
            "(deny file-write*)",
            "(allow file-write* (subpath \"/p/da\\\"ta\"))",
            "(deny file-read-data (subpath \"/p\"))",
            "(allow file-read-data (subpath \"/p/plugin\") (subpath \"/p/da\\\"ta\"))",
            "(deny network*)",
        ]
        .join("\n")
    );
    policy.network = true;
    assert!(!profile(&policy).unwrap().contains("network"));
}

#[test]
fn the_plugin_runs_under_sandbox_exec() {
    let policy = policy(Path::new("/p"), Path::new("/p/plugin"), Path::new("/tmp"));
    let (program, args) = entry(&policy, None, (PathBuf::from("python3"), vec![OsString::from("main.py")])).unwrap();
    assert_eq!(program, PathBuf::from(SANDBOX_EXEC));
    assert_eq!(args[0], "-p");
    assert_eq!(args[1], OsString::from(profile(&policy).unwrap()));
    assert_eq!(&args[2..], [OsString::from("python3"), OsString::from("main.py")]);
}

#[test]
fn a_confined_process_reads_and_writes_only_what_its_policy_opens() {
    if !seatbelt() {
        return eprintln!("skipped: sandbox-exec cannot apply a profile here");
    }
    let tree = tempfile::tempdir().unwrap();
    let root = tree.path().canonicalize().unwrap().join("project");
    let (plugin, data) = (root.join("plugin"), root.join("data"));
    for dir in [&plugin, &data] {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(root.join("secret.txt"), "s3cr3t").unwrap();
    std::fs::write(plugin.join("main.sh"), "echo plugin").unwrap();
    let script = "cat plugin/main.sh && echo ok > data/out.txt; cat secret.txt; echo no > leaked.txt";
    let shell = (PathBuf::from("/bin/sh"), vec![OsString::from("-c"), OsString::from(script)]);
    let (program, args) = entry(&policy(&root, &plugin, &data), None, shell).unwrap();
    let ran = Command::new(program).args(args).current_dir(&root).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&ran.stdout), "plugin\n");
    assert_eq!(std::fs::read_to_string(data.join("out.txt")).unwrap(), "ok\n");
    assert!(!root.join("leaked.txt").exists());
}
