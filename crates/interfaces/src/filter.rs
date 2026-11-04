use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use vc_model::{FileKind, SourcePath};

use crate::SourceError;

/// Left out unless the config says otherwise (`paths.exclude`, syntax reference).
const DEFAULT_EXCLUDE: [&str; 1] = ["**/node_modules"];

/// Vibranium's own state (cache, history, reports) and git's: never definition files.
const ALWAYS_EXCLUDED: [&str; 2] = [".vibranium", "**/.git"];

/// Which files discovery reads: the config's `paths.include` and `paths.exclude`.
///
/// Globs match the path relative to the project root, with `/` separators: `*` stays within
/// one folder and `**` crosses folders. An exclude glob that matches a folder leaves out
/// everything in it. With no include globs, every file is included. `.vibranium/` and `.git/`
/// folders are always left out; [`SourceFilter::default`] also leaves out `node_modules` ones.
#[derive(Clone, Debug)]
pub struct SourceFilter {
    include: Option<GlobSet>,
    exclude: GlobSet,
}

impl SourceFilter {
    /// A filter from the include and exclude globs as written.
    ///
    /// # Errors
    ///
    /// [`SourceError::InvalidGlob`] for the first glob that does not parse.
    pub fn new<S: AsRef<str>>(include: &[S], exclude: &[S]) -> Result<Self, SourceError> {
        let include = if include.is_empty() { None } else { Some(glob_set(include.iter().map(AsRef::as_ref))?) };
        let exclude = glob_set(exclude.iter().map(AsRef::as_ref).chain(ALWAYS_EXCLUDED))?;
        Ok(Self { include, exclude })
    }

    /// A filter from the config's globs: written `exclude` globs replace the default ones,
    /// and `None` keeps them.
    ///
    /// # Errors
    ///
    /// [`SourceError::InvalidGlob`] for the first glob that does not parse.
    pub fn written<S: AsRef<str>>(include: &[S], exclude: Option<&[S]>) -> Result<Self, SourceError> {
        let include: Vec<&str> = include.iter().map(AsRef::as_ref).collect();
        let exclude =
            exclude.map_or_else(|| DEFAULT_EXCLUDE.to_vec(), |globs| globs.iter().map(AsRef::as_ref).collect());
        Self::new(&include, &exclude)
    }

    /// Whether discovery reads the file at `path`. Include globs pick the other definition
    /// files; the root config is always read, since it says how to read the rest.
    #[must_use]
    pub fn allows(&self, path: &SourcePath) -> bool {
        let text = path.as_str();
        let included =
            |include: &GlobSet| include.is_match(text) || FileKind::from_path(path) == Some(FileKind::Config);
        self.include.as_ref().is_none_or(included) && !self.skips(text)
    }

    /// What discovery does with the file at `relative` (relative to the root, `/` or `\`
    /// separators): `None` to pass it by, because its name has no
    /// [`FileKind`] or the filter leaves it out; `Some(Ok(path))` to read
    /// it; `Some(Err(SourceError::InvalidPath))` to report it, when it looks like a definition
    /// file but its name is one a [`SourcePath`] refuses.
    #[must_use]
    pub fn select(&self, relative: &str) -> Option<Result<SourcePath, SourceError>> {
        let (path, valid) = match SourcePath::new(relative) {
            Ok(path) => (path, true),
            Err(_) => (SourcePath::lossy(relative).ok()?, false),
        };
        if FileKind::from_path(&path).is_none() || !self.allows(&path) {
            return None;
        }
        Some(if valid { Ok(path) } else { Err(SourceError::InvalidPath { path: path.to_string() }) })
    }

    /// The path of a file at `relative` that is not a definition file but that a `$file:` may
    /// point at; `None` for a definition file, a name a [`SourcePath`] refuses, or a file the
    /// exclude globs leave out. Include globs pick definition files, so they do not apply.
    #[must_use]
    pub fn asset(&self, relative: &str) -> Option<SourcePath> {
        let path = SourcePath::new(relative).ok()?;
        (FileKind::from_path(&path).is_none() && !self.skips(path.as_str())).then_some(path)
    }

    /// Whether `path` (a file or a folder, relative to the root) is excluded, by itself or
    /// through a folder it is in. A walker uses this to skip whole folders.
    #[must_use]
    pub fn skips(&self, path: &str) -> bool {
        let folders = path.match_indices('/').filter_map(|(at, _)| path.get(..at));
        folders.chain([path]).any(|part| self.exclude.is_match(part))
    }
}

impl Default for SourceFilter {
    fn default() -> Self {
        let include: [&str; 0] = [];
        Self::new(&include, &DEFAULT_EXCLUDE).unwrap_or_else(|_| Self { include: None, exclude: GlobSet::empty() })
    }
}

fn glob_set<'a>(globs: impl Iterator<Item = &'a str>) -> Result<GlobSet, SourceError> {
    let mut set = GlobSetBuilder::new();
    for glob in globs {
        let invalid =
            |error: globset::Error| SourceError::InvalidGlob { glob: glob.to_owned(), reason: error.to_string() };
        set.add(GlobBuilder::new(glob).literal_separator(true).build().map_err(invalid)?);
    }
    set.build().map_err(|error| SourceError::InvalidGlob { glob: String::new(), reason: error.to_string() })
}

#[cfg(test)]
mod tests {
    use vc_model::SourcePath;

    use super::SourceFilter;
    use crate::SourceError;

    fn allows(filter: &SourceFilter, path: &str) -> bool {
        filter.allows(&SourcePath::new(path).unwrap())
    }

    #[test]
    fn default_leaves_out_node_modules_and_vibranium() {
        let filter = SourceFilter::default();
        assert!(allows(&filter, "services/users/users.api.yaml"));
        assert!(!allows(&filter, "node_modules/x/x.api.yaml"));
        assert!(!allows(&filter, "web/node_modules/x.api.yaml"));
        assert!(!allows(&filter, ".vibranium/cache/x.api.yaml"));
        assert!(!allows(&filter, ".git/x.api.yaml") && !allows(&filter, "sub/.git/x.api.yaml"));
        assert!(filter.skips("web/node_modules") && filter.skips(".vibranium"));
        assert!(!filter.skips("services"));
    }

    #[test]
    fn include_globs_pick_files_and_exclude_globs_drop_folders() {
        let filter = SourceFilter::new(&["services/**", "vibranium.config.yaml"], &["services/legacy"]).unwrap();
        assert!(allows(&filter, "vibranium.config.yaml"));
        assert!(allows(&filter, "services/users/users.api.yaml"));
        assert!(!allows(&filter, "services/legacy/old.api.yaml"));
        assert!(!allows(&filter, "other/x.api.yaml"));
        assert!(!allows(&filter, ".vibranium/x.api.yaml"), ".vibranium is always left out");
    }

    #[test]
    fn written_excludes_replace_the_default_ones() {
        let filter = SourceFilter::new(&[], &["legacy"]).unwrap();
        assert!(allows(&filter, "node_modules/x.api.yaml"));
        assert!(!allows(&filter, "legacy/x.api.yaml"));
    }

    #[test]
    fn include_globs_always_keep_the_root_config() {
        let filter = SourceFilter::new(&["services/**"], &[]).unwrap();
        assert!(allows(&filter, "vibranium.config.yaml"));
        assert!(!allows(&filter, "other/vibranium.config.yaml"), "only the root config is a config");
        let excluded = SourceFilter::new(&["services/**"], &["vibranium.config.yaml"]).unwrap();
        assert!(!allows(&excluded, "vibranium.config.yaml"), "an exclude still applies");
    }

    #[test]
    fn written_globs_keep_the_default_excludes_unless_they_write_their_own() {
        let kept = SourceFilter::written(&["services/**"], None).unwrap();
        assert!(allows(&kept, "services/a.api.yaml") && !allows(&kept, "services/node_modules/a.api.yaml"));
        assert!(!allows(&kept, "other/a.api.yaml"));
        let own = SourceFilter::written(&[], Some(&["legacy"])).unwrap();
        assert!(allows(&own, "services/node_modules/a.api.yaml") && !allows(&own, "legacy/a.api.yaml"));
        assert!(SourceFilter::written(&["[a"], None).is_err());
    }

    #[test]
    fn a_star_stays_within_one_folder() {
        let filter = SourceFilter::new(&["*.api.yaml"], &[]).unwrap();
        assert!(allows(&filter, "users.api.yaml"));
        assert!(!allows(&filter, "services/users.api.yaml"));
    }

    #[test]
    fn select_reads_definition_files_and_reports_bad_names() {
        let filter = SourceFilter::default();
        assert_eq!(
            filter.select(r"services\users.api.yaml"),
            Some(Ok(SourcePath::new("services/users.api.yaml").unwrap()))
        );
        assert_eq!(filter.select("schemas/user.json"), None);
        assert_eq!(filter.select("node_modules/x.api.yaml"), None);
        assert_eq!(filter.select("k:v.json"), None, "a bad name is only reported on a definition file");
        let bad = filter.select("k:v.api.yaml");
        assert_eq!(bad, Some(Err(SourceError::InvalidPath { path: "k\u{fffd}v.api.yaml".to_owned() })));
    }

    #[test]
    fn assets_are_the_other_files_the_excludes_keep() {
        let filter = SourceFilter::new(&["services/**"], &["legacy"]).unwrap();
        assert_eq!(filter.asset(r"openapi\users.yaml"), Some(SourcePath::new("openapi/users.yaml").unwrap()));
        assert_eq!(filter.asset("services/users/users.api.yaml"), None, "a definition file is not an asset");
        assert_eq!(filter.asset("legacy/user.json"), None);
        assert_eq!(filter.asset(".vibranium/cache/compile.bin"), None);
        assert_eq!(filter.asset("k:v.json"), None);
    }

    #[test]
    fn bad_globs_are_reported() {
        let error = SourceFilter::new(&["services/[a"], &[]).unwrap_err();
        assert!(matches!(error, SourceError::InvalidGlob { glob, .. } if glob == "services/[a"));
    }
}
