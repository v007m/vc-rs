//! [`ProgramBuilder`]: the compiler fills one table at a time, then `freeze()` hands out the
//! immutable [`Program`].

use std::sync::Arc;

use crate::{
    Action, ActionId, Api, ApiGroup, AuthDef, Config, Dataset, GeneratorDef, HookModules, ModelError, Profile, Program,
    RunBlocks, SourceFiles, StringTable, Sym, Test, TestFile,
};

/// Accumulates a [`Program`]; nothing can change it after [`ProgramBuilder::freeze`].
#[derive(Debug, Default)]
pub struct ProgramBuilder {
    program: Program,
}

impl ProgramBuilder {
    /// An empty program over the compile's strings and files.
    #[must_use]
    pub fn new(strings: StringTable, files: SourceFiles) -> Self {
        Self { program: Program::new(strings, files) }
    }

    /// The symbol for `text`, adding it to the program's strings.
    ///
    /// # Errors
    /// [`ModelError::StringTableFull`] when the table is full.
    pub fn intern(&mut self, text: &str) -> Result<Sym, ModelError> {
        self.program.strings_mut().intern(text)
    }

    /// Sets the config.
    pub fn config(&mut self, config: Config) {
        self.program.config = config;
    }

    /// Adds a profile, keeping the first one with its ID.
    pub fn profile(&mut self, profile: Profile) {
        self.program.profiles.entry(profile.id).or_insert(profile);
    }

    /// Adds an auth definition, keeping the first one with its ID.
    pub fn auth(&mut self, auth: AuthDef) {
        self.program.auth.entry(auth.id).or_insert(auth);
    }

    /// Adds an api file.
    pub fn api_group(&mut self, group: ApiGroup) {
        self.program.api_groups.entry(group.id).or_insert(group);
    }

    /// Adds an api.
    pub fn api(&mut self, api: Api) {
        self.program.apis.entry(api.id).or_insert(api);
    }

    /// Adds a test file.
    pub fn test_file(&mut self, file: TestFile) {
        self.program.test_files.entry(file.id).or_insert(file);
    }

    /// Adds a test.
    pub fn test(&mut self, test: Test) {
        self.program.tests.entry(test.id).or_insert(test);
    }

    /// The ID the next [`ProgramBuilder::action`] gets, for checks that name their action.
    ///
    /// # Errors
    /// [`ModelError::ActionTableFull`] when no further ID fits.
    pub fn next_action(&self) -> Result<ActionId, ModelError> {
        let len = self.program.actions.len();
        u32::try_from(len).map(ActionId::new).map_err(|_| ModelError::ActionTableFull(len))
    }

    /// Adds an action and gives its ID.
    ///
    /// # Errors
    /// [`ModelError::ActionTableFull`] when no further ID fits.
    pub fn action(&mut self, action: Action) -> Result<ActionId, ModelError> {
        let id = self.next_action()?;
        self.program.actions.push(action);
        Ok(id)
    }

    /// Adds a named generator, keeping the first one with its ID.
    pub fn generator(&mut self, generator: GeneratorDef) {
        self.program.generators.entry(generator.id).or_insert(generator);
    }

    /// Adds a dataset, keeping the first one with its ID.
    pub fn dataset(&mut self, dataset: Dataset) {
        self.program.datasets.entry(dataset.id).or_insert(dataset);
    }

    /// Sets the config's `beforeRun` and `afterRun` actions.
    pub fn run_blocks(&mut self, blocks: RunBlocks) {
        self.program.run_blocks = blocks;
    }

    /// Sets the hook files the program refers to (D172).
    pub fn hooks(&mut self, hooks: HookModules) {
        self.program.hooks = hooks;
    }

    /// The finished program, shared read-only from here on.
    #[must_use]
    pub fn freeze(self) -> Arc<Program> {
        Arc::new(self.program)
    }
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;

    use super::ProgramBuilder;
    use crate::{
        Action, ActionKind, ApiId, Expect, Owner, Pos, Profile, ProfileId, SaveSpec, SourceFileId, SourceFiles,
        SourceSpan, StringTable, VarDefs,
    };

    fn action(builder: &mut ProgramBuilder) -> Action {
        let api = ApiId::new(builder.intern("users#get").unwrap());
        Action {
            id: None,
            kind: ActionKind::Call(api),
            owner: Owner::Instance,
            vars: VarDefs::default(),
            auth: None,
            headers: IndexMap::new(),
            query: IndexMap::new(),
            body: None,
            expect: Expect::default(),
            save: SaveSpec::default(),
            poll: None,
            retry: None,
            keep: false,
            provides: vec![],
            cache: None,
            repeat: None,
            span: SourceSpan::new(SourceFileId::new(0), Pos::new(1, 1, 0), Pos::new(1, 1, 0)),
        }
    }

    #[test]
    fn numbers_actions_in_order_and_freezes_what_was_added() {
        let mut builder = ProgramBuilder::new(StringTable::new(), SourceFiles::new());
        let first = action(&mut builder);
        assert_eq!(builder.next_action().unwrap().index(), 0);
        assert_eq!(builder.action(first.clone()).unwrap().index(), 0);
        assert_eq!(builder.action(first).unwrap().index(), 1);
        let id = ProfileId::new(builder.intern("dev").unwrap());
        builder.profile(Profile { id, name: None, default: true, vars: IndexMap::new() });
        builder.profile(Profile { id, name: Some("again".into()), default: false, vars: IndexMap::new() });
        let program = builder.freeze();
        assert_eq!(program.actions.len(), 2);
        assert_eq!(program.profiles.len(), 1);
        assert!(program.profiles[&id].default);
        assert_eq!(program.str(id.sym()), Some("dev"));
        assert!(program.apis.is_empty() && program.test_files.is_empty());
    }
}
