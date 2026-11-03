use indexmap::IndexMap;

use crate::{
    Action, Api, ApiGroup, ApiId, AuthDef, AuthId, Config, Dataset, DatasetId, FileId, GeneratorDef, Lifecycle,
    LoadScenario, Profile, ProfileId, SourceFiles, StringTable, Sym, Test, TestFile, TestId,
};

/// Everything `vc check` produced, frozen and shared read-only as `Arc<Program>`.
///
/// The compiler builds it and hands it out once there are no errors; nothing changes it after
/// that. The linker fills one table per kind of definition; see the LLD, "The program".
#[derive(Clone, Debug, Default)]
pub struct Program {
    strings: StringTable,
    /// Path, text and hash of every source file, by [`crate::SourceFileId`].
    pub files: SourceFiles,
    /// Defaults, reports, history and the other config settings.
    pub config: Config,
    /// `profiles`, in config order.
    pub profiles: IndexMap<ProfileId, Profile>,
    /// `auth`, in config order.
    pub auth: IndexMap<AuthId, AuthDef>,
    /// One entry per api file.
    pub api_groups: IndexMap<FileId, ApiGroup>,
    /// Every api, by full ID.
    pub apis: IndexMap<ApiId, Api>,
    /// One entry per test file.
    pub test_files: IndexMap<FileId, TestFile>,
    /// Every test, by full ID.
    pub tests: IndexMap<TestId, Test>,
    /// Every action anywhere, by [`crate::ActionId`].
    pub actions: Vec<Action>,
    /// `beforeRun` and `afterRun`.
    pub run_blocks: Lifecycle,
    /// One entry per load file.
    pub loads: IndexMap<FileId, LoadScenario>,
    /// `datasets`.
    pub datasets: IndexMap<DatasetId, Dataset>,
    /// `generators`.
    pub generators: IndexMap<Sym, GeneratorDef>,
}

impl Program {
    /// A program over its interned strings and its source files, with empty tables.
    #[must_use]
    pub fn new(strings: StringTable, files: SourceFiles) -> Self {
        Self { strings, files, ..Self::default() }
    }

    /// The text behind `sym`, a symbol from this program's string table.
    #[must_use]
    pub fn str(&self, sym: Sym) -> Option<&str> {
        self.strings.resolve(sym)
    }
}

#[cfg(test)]
mod tests {
    use super::Program;
    use crate::{SourceFiles, StringTable};

    #[test]
    fn str_resolves_symbols_from_its_table() {
        let mut strings = StringTable::new();
        let sym = strings.intern("users#create").unwrap();
        let program = Program::new(strings, SourceFiles::new());
        assert_eq!(program.str(sym), Some("users#create"));
        assert!(program.files.is_empty() && program.apis.is_empty() && program.actions.is_empty());
    }
}
