//! Hook files as the hook runner gets them: TypeScript or JavaScript with the types stripped by
//! the compiler (D172).

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// One hook file as JavaScript, with the TypeScript line each JavaScript line came from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[expect(
    clippy::exhaustive_structs,
    reason = "vc-model data type, shared across the workspace; every user is in this repo (D160)"
)]
pub struct HookModule {
    /// The JavaScript, an ES module.
    pub js: String,
    /// For JavaScript line `n` (from 1), entry `n - 1` holds its TypeScript line (from 1), or 0
    /// when it has none.
    pub lines: Vec<u32>,
}

impl HookModule {
    /// The TypeScript line that JavaScript line `js_line` (from 1) came from.
    #[must_use]
    pub fn ts_line(&self, js_line: u32) -> Option<u32> {
        let index = usize::try_from(js_line.checked_sub(1)?).ok()?;
        self.lines.get(index).copied().filter(|line| *line > 0)
    }
}

/// Every hook file a program uses, by project-relative path. Cloning shares the files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HookModules(Arc<BTreeMap<String, HookModule>>);

impl HookModules {
    /// The modules given, by path.
    #[must_use]
    pub fn new(modules: BTreeMap<String, HookModule>) -> Self {
        Self(Arc::new(modules))
    }

    /// The module at `path`.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&HookModule> {
        self.0.get(path)
    }

    /// Every path and its module, by path.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &HookModule)> {
        self.0.iter().map(|(path, module)| (path.as_str(), module))
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// The extensions an import may leave out, tried in this order (D172).
const EXTENSIONS: [&str; 4] = [".ts", ".mts", ".js", ".mjs"];

/// The hook files an import of the project path `path` may mean, in the order they are tried.
///
/// They are the path itself, the path with an extension added, or, for a `.js` or `.mjs` path,
/// the TypeScript file it is compiled from (D172).
#[must_use]
pub fn import_paths(path: &str) -> Vec<String> {
    let mut found = vec![path.to_owned()];
    found.extend(EXTENSIONS.iter().map(|extension| format!("{path}{extension}")));
    for (javascript, typescript) in [(".js", ".ts"), (".mjs", ".mts")] {
        if let Some(stem) = path.strip_suffix(javascript) {
            found.push(format!("{stem}{typescript}"));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{HookModule, HookModules, import_paths};

    #[test]
    fn maps_a_javascript_line_to_its_typescript_line() {
        let module = HookModule { js: "a\nb\nc\n".into(), lines: vec![2, 0, 5] };
        assert_eq!([0, 1, 2, 3, 4].map(|line| module.ts_line(line)), [None, Some(2), None, Some(5), None]);
    }

    #[test]
    fn finds_modules_by_path_and_shares_them_when_cloned() {
        let modules = HookModules::new(BTreeMap::from([("hooks/a.ts".to_owned(), HookModule::default())]));
        let shared = modules.clone();
        assert!(shared.get("hooks/a.ts").is_some() && shared.get("hooks/b.ts").is_none());
        assert_eq!(shared.iter().map(|(path, _)| path).collect::<Vec<_>>(), ["hooks/a.ts"]);
        assert!(!modules.is_empty() && HookModules::default().is_empty());
    }

    #[test]
    fn an_import_may_leave_out_its_extension_or_name_the_compiled_file() {
        assert_eq!(import_paths("a/b")[..3], ["a/b", "a/b.ts", "a/b.mts"]);
        assert!(import_paths("a/b.js").contains(&"a/b.ts".to_owned()));
        assert!(import_paths("a/b.mjs").contains(&"a/b.mts".to_owned()));
    }
}
