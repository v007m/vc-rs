//! A path the config, a flag or the engine writes with `/` (`.vibranium/reports`), joined to
//! the project root with the platform's separators, so Windows prints
//! `C:\shop\.vibranium\reports\1` and never `C:\shop\.vibranium/reports\1`.

use std::path::{Component, Path, PathBuf};

/// `relative` under `root`, rebuilt from its parts; an absolute one replaces `root`, as
/// [`Path::join`] does.
pub fn under(root: &Path, relative: impl AsRef<Path>) -> PathBuf {
    root.join(relative.as_ref().components().collect::<PathBuf>())
}

/// `path` relative to `root`, rebuilt from its parts as [`under`] does; none when it is not
/// under `root` or a part of the rest is not a plain name. The check is lexical: a `..` part
/// (from a `reports.dir` such as `../out`) leaves the root.
pub fn plain(root: &Path, path: &Path) -> Option<PathBuf> {
    let relative = path.strip_prefix(root).ok()?;
    let plain = relative.components().all(|part| matches!(part, Component::Normal(_)));
    plain.then(|| relative.components().collect())
}

#[cfg(test)]
mod tests {
    use std::path::{Component, Path};

    use super::{plain, under};

    #[test]
    fn a_configured_relative_path_joins_the_root_part_by_part() {
        let root = Path::new("shop");
        let joined = under(root, "a/b");
        let parts: Vec<Component<'_>> = joined.components().collect();
        let expected =
            [Component::Normal("shop".as_ref()), Component::Normal("a".as_ref()), Component::Normal("b".as_ref())];
        assert_eq!(parts, expected);
        let native = format!("shop{0}a{0}b", std::path::MAIN_SEPARATOR);
        assert_eq!(joined.to_str(), Some(native.as_str()), "every separator is the platform's");
        assert_eq!(under(root, ".vibranium/reports/"), root.join(".vibranium").join("reports"));
    }

    #[test]
    fn an_absolute_path_replaces_the_root() {
        let absolute = std::env::temp_dir().join("reports");
        assert_eq!(under(Path::new("shop"), &absolute), absolute);
    }

    #[test]
    fn a_path_is_plain_only_under_the_root_with_plain_names() {
        let root = Path::new("/p");
        assert_eq!(plain(root, Path::new("/p/.vibranium/reports/3")), Some(Path::new(".vibranium/reports/3").into()));
        assert_eq!(plain(root, Path::new("/elsewhere/r")), None);
        assert_eq!(plain(root, &root.join("../out")), None, "`..` leaves the root");
    }
}
