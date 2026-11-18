//! A project folder read as source files, shared by the golden cases and the compile cache's
//! tests, which include this file with `#[path]`.

use std::error::Error;
use std::fs;
use std::path::Path;

use vc_model::{ContentHash, SourceFile, SourcePath};

/// Every file under `dir`, with its path relative to `root`.
pub fn read_tree(root: &Path, dir: &Path, files: &mut Vec<SourceFile>) -> Result<(), Box<dyn Error>> {
    let mut entries = fs::read_dir(dir)?.map(|entry| entry.map(|entry| entry.path())).collect::<Result<Vec<_>, _>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            read_tree(root, &path, files)?;
            continue;
        }
        let relative = path.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
        let text = fs::read_to_string(&path)?;
        files.push(SourceFile { path: SourcePath::new(&relative)?, text, hash: ContentHash::new([0; 32]) });
    }
    Ok(())
}
