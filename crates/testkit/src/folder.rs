use std::collections::BTreeMap;
use std::io;
use std::path::Path;

/// Every file under `root` as text, by its path from `root` with `/` between folders: a project
/// on disk (such as the docs site's example project) as the in-memory sources a test hands over.
///
/// A `.vibranium/` folder is skipped: it holds what a `vc` run there cached, not sources.
///
/// # Errors
///
/// The first folder or file that cannot be read, or a file that is not UTF-8.
pub fn read_folder(root: &Path) -> io::Result<BTreeMap<String, String>> {
    let mut files = BTreeMap::new();
    collect(root, root, &mut files)?;
    Ok(files)
}

fn collect(root: &Path, dir: &Path, files: &mut BTreeMap<String, String>) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == ".vibranium") {
                continue;
            }
            collect(root, &path, files)?;
        } else {
            let relative = path.strip_prefix(root).map_err(io::Error::other)?;
            files.insert(relative.to_string_lossy().replace('\\', "/"), std::fs::read_to_string(&path)?);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::read_folder;

    #[test]
    fn reads_each_file_by_its_slash_path() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let files = read_folder(&root).unwrap();
        assert!(files["folder.rs"].contains("pub fn read_folder"));
        assert!(files.keys().all(|path| !path.contains('\\')));
        assert!(read_folder(&root.join("missing")).is_err());
    }

    #[test]
    fn skips_the_cache_a_vc_run_leaves() {
        let root = std::env::temp_dir().join(format!("vc-read-folder-{}", std::process::id()));
        let cache = root.join(".vibranium/cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("compile.bin"), [0xff, 0xfe, 0x00]).unwrap();
        std::fs::write(root.join("vibranium.config.yaml"), "project: shop\n").unwrap();
        let files = read_folder(&root).unwrap();
        assert_eq!(files.keys().collect::<Vec<_>>(), ["vibranium.config.yaml"]);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
