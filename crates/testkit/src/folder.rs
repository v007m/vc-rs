use std::collections::BTreeMap;
use std::io;
use std::path::Path;

/// Every file under `root` as text, by its path from `root` with `/` between folders: a project
/// on disk (such as the docs site's example project) as the in-memory sources a test hands over.
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
}
