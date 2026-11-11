use std::ffi::OsString;
use std::path::PathBuf;

use vc_interfaces::PluginPermissions;

use super::{DEVICES, LAUNCHER_ARG, Policy, launch, plain, real, without_verbatim};

fn permissions(files: &[&str], network: &[&str]) -> PluginPermissions {
    PluginPermissions {
        files: files.iter().map(|&file| file.to_owned()).collect(),
        network: network.iter().map(|&host| host.to_owned()).collect(),
        ..PluginPermissions::default()
    }
}

fn args(list: &[&str]) -> impl Iterator<Item = OsString> {
    list.iter().map(OsString::from).collect::<Vec<_>>().into_iter()
}

#[test]
fn a_plugin_reads_its_folder_and_writes_its_data_its_files_the_devices_and_temp() {
    let root = std::path::absolute("/no-such-vc-project").unwrap();
    let dir = root.join(".vibranium/plugins/acme");
    let data = root.join(".vibranium/plugin-data/acme");
    let policy = Policy::new(&root, &dir, &permissions(&["reports/", "out.json"], &[])).unwrap().writing(Some(&data));
    let temp = real(&std::env::temp_dir());
    let mut write: Vec<PathBuf> = DEVICES.iter().map(PathBuf::from).collect();
    write.extend([temp, root.join("reports"), root.join("out.json"), data]);
    assert_eq!(policy, Policy { root: root.clone(), read: vec![dir], write, network: false });
    let networked = Policy::new(&root, &root, &permissions(&[], &["api.acme.dev"])).unwrap();
    assert!(networked.network);
}

#[test]
fn a_project_inside_the_temporary_folder_does_not_open_it() {
    let home = tempfile::tempdir().unwrap();
    let policy = Policy::new(home.path(), home.path(), &permissions(&[], &[])).unwrap();
    let devices: Vec<PathBuf> = DEVICES.iter().map(PathBuf::from).collect();
    assert_eq!(policy.write, devices);
}

#[test]
fn a_file_permission_outside_the_project_is_refused() {
    let home = tempfile::tempdir().unwrap();
    for file in ["../elsewhere", "/etc/passwd", "", "./reports", "reports/../../x"] {
        let refused = Policy::new(home.path(), home.path(), &permissions(&[file], &[])).unwrap_err();
        assert_eq!(refused, format!("its file permission `{file}` is not a path inside the project"));
    }
}

#[cfg(unix)]
#[test]
fn a_file_permission_through_a_link_out_of_the_project_is_refused() {
    let (home, away) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::os::unix::fs::symlink(away.path(), home.path().join("reports")).unwrap();
    let refused = Policy::new(home.path(), home.path(), &permissions(&["reports"], &[])).unwrap_err();
    assert_eq!(refused, "its file permission `reports` is not a path inside the project");
}

#[cfg(unix)]
#[test]
fn a_file_not_made_yet_is_checked_where_its_nearest_existing_folder_really_is() {
    let (home, away) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let root = home.path().canonicalize().unwrap();
    std::os::unix::fs::symlink(away.path(), root.join("out")).unwrap();
    std::os::unix::fs::symlink(away.path().join("gone"), root.join("dangling.json")).unwrap();
    for file in ["out/new_key", "out/keys/new_key", "dangling.json"] {
        let refused = Policy::new(&root, &root, &permissions(&[file], &[])).unwrap_err();
        assert_eq!(refused, format!("its file permission `{file}` is not a path inside the project"));
    }
    let made = Policy::new(&root, &root, &permissions(&["reports/summary.json"], &[])).unwrap();
    assert_eq!(made.write.last(), Some(&root.join("reports/summary.json")));
}

#[test]
fn only_a_drive_path_loses_its_verbatim_prefix() {
    assert_eq!(without_verbatim(PathBuf::from(r"\\?\C:\x")), PathBuf::from(r"C:\x"));
    for kept in [r"\\?\UNC\srv\s", r"\\?\C", r"C:\x", "/srv/project"] {
        assert_eq!(without_verbatim(PathBuf::from(kept)), PathBuf::from(kept));
    }
}

#[test]
fn only_windows_writes_a_resolved_path_apart() {
    let verbatim = PathBuf::from(r"\\?\C:\x");
    let expected = if cfg!(windows) { PathBuf::from(r"C:\x") } else { verbatim.clone() };
    assert_eq!(plain(verbatim), expected);
}

#[test]
fn only_the_launcher_argument_makes_vc_the_launcher() {
    assert_eq!(launch(args(&["vc", "run"])), None);
    assert_eq!(launch(args(&["vc"])), None);
    let missing = launch(args(&["vc", LAUNCHER_ARG, "{}"])).unwrap();
    assert_eq!(missing, "the plugin launcher takes a policy and a program");
    let invalid = launch(args(&["vc", LAUNCHER_ARG, "{}", "python3"])).unwrap();
    assert!(invalid.starts_with("the plugin launcher's policy is not valid: "), "{invalid}");
}

#[cfg(not(target_os = "macos"))]
#[tokio::test]
async fn without_a_launcher_or_a_confining_os_no_plugin_can_be_confined() {
    let why = super::ready(None).unwrap_err();
    assert_ne!(why, "", "the refusal says why");
    assert_eq!(super::Confiner::new(None).ready().await, Err(why), "the confiner asks the OS the same");
}
