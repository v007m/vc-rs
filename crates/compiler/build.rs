//! Hashes the sources a compile's output depends on into `VC_LINK_BUILD_ID`, so a compile cache
//! that a build with other compiler, model, parser, template or diagnostics code, other
//! dependency versions (the workspace `Cargo.lock`) or other features wrote misses, even when the
//! release is the same (D333).

use std::error::Error;
use std::path::Path;
use std::{env, fs};

/// The crates, by their folder next to this one, whose code decides what a compile gives back.
const CRATES: [&str; 5] = ["compiler", "model", "parser", "template", "diagnostics"];

fn main() -> Result<(), Box<dyn Error>> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut hasher = blake3::Hasher::new();
    for name in CRATES {
        let src = crates.join(name).join("src");
        println!("cargo:rerun-if-changed={}", src.display());
        hasher.update(name.as_bytes());
        hash_tree(&src, &src, &mut hasher)?;
    }
    let lock = crates.join("..").join("Cargo.lock");
    println!("cargo:rerun-if-changed={}", lock.display());
    hash_bytes("Cargo.lock", &fs::read(lock)?, &mut hasher);
    let mut features: Vec<String> =
        env::vars().map(|(name, _)| name).filter(|name| name.starts_with("CARGO_FEATURE_")).collect();
    features.sort();
    hash_bytes("features", features.join("\n").as_bytes(), &mut hasher);
    println!("cargo:rustc-env=VC_LINK_BUILD_ID={}", hasher.finalize().to_hex());
    Ok(())
}

/// Adds every file under `dir` to `hasher` in path order: its path from `root`, its length and
/// its bytes.
fn hash_tree(root: &Path, dir: &Path, hasher: &mut blake3::Hasher) -> Result<(), Box<dyn Error>> {
    let mut entries = fs::read_dir(dir)?.map(|entry| entry.map(|entry| entry.path())).collect::<Result<Vec<_>, _>>()?;
    entries.sort();
    for path in entries {
        if path.is_dir() {
            hash_tree(root, &path, hasher)?;
            continue;
        }
        let relative = path.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
        hash_bytes(&relative, &fs::read(&path)?, hasher);
    }
    Ok(())
}

/// Adds `bytes` to `hasher` under `name`: the name, the length and the bytes.
fn hash_bytes(name: &str, bytes: &[u8], hasher: &mut blake3::Hasher) {
    hasher.update(name.as_bytes()).update(&(bytes.len() as u64).to_le_bytes()).update(bytes);
}
