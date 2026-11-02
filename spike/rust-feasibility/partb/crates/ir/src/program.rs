//! The arena that owns every test and the builder that validates it.
use crate::values::{FileId, FileName, Role, Test, TestDraft, TestId};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IrError {
    #[error("test {test:?} refers to unknown setup {setup:?}")]
    UnknownSetup { test: TestId, setup: TestId },
    #[error("setup {setup:?} of test {test:?} has its own setup; setup chains are not supported")]
    NestedSetup { test: TestId, setup: TestId },
    #[error("test {0:?} refers to unknown file")]
    UnknownFile(TestId),
    #[error("too many nodes for a u32 id")]
    TooLarge,
}

/// Immutable compiled program; share it as `Arc<Program>`.
#[derive(Debug, PartialEq, Eq)]
pub struct Program {
    files: Vec<FileName>,
    tests: Vec<Test>,
}

impl Program {
    /// Ids are only minted by this program's builder, so lookups cannot miss.
    #[must_use]
    pub fn test(&self, id: TestId) -> &Test {
        &self.tests[id.index()]
    }

    #[must_use]
    pub fn file_name(&self, id: FileId) -> Option<&FileName> {
        self.files.get(id.index())
    }

    pub fn tests(&self) -> impl Iterator<Item = &Test> {
        self.tests.iter()
    }

    /// Tests that run on their own (not setup-only).
    pub fn runnable(&self) -> impl Iterator<Item = TestId> {
        self.tests.iter().filter(|t| t.spec.role == Role::Test).map(|t| t.id)
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.tests.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.tests.is_empty()
    }
}

#[derive(Debug, Default)]
pub struct ProgramBuilder {
    files: Vec<FileName>,
    drafts: Vec<TestDraft>,
}

impl ProgramBuilder {
    /// # Errors
    /// [`IrError::TooLarge`] past `u32::MAX` files.
    pub fn add_file(&mut self, name: FileName) -> Result<FileId, IrError> {
        let id = FileId::new(u32::try_from(self.files.len()).map_err(|_| IrError::TooLarge)?);
        self.files.push(name);
        Ok(id)
    }

    /// # Errors
    /// [`IrError::TooLarge`] past `u32::MAX` tests.
    pub fn add_test(&mut self, draft: TestDraft) -> Result<TestId, IrError> {
        let id = TestId::new(u32::try_from(self.drafts.len()).map_err(|_| IrError::TooLarge)?);
        self.drafts.push(draft);
        Ok(id)
    }

    /// Validate references and freeze the program.
    ///
    /// # Errors
    /// Unknown files, unknown setups, or setups that have setups.
    pub fn build(self) -> Result<Program, IrError> {
        let tests: Vec<Test> =
            self.drafts.into_iter().zip(0u32..).map(|(spec, i)| Test { id: TestId::new(i), spec }).collect();
        for test in &tests {
            validate(test, &tests, self.files.len())?;
        }
        Ok(Program { files: self.files, tests })
    }
}

fn validate(test: &Test, all: &[Test], file_count: usize) -> Result<(), IrError> {
    if test.spec.file.index() >= file_count {
        return Err(IrError::UnknownFile(test.id));
    }
    let Some(setup) = test.spec.setup else { return Ok(()) };
    let target = all.get(setup.test.index()).ok_or(IrError::UnknownSetup { test: test.id, setup: setup.test })?;
    if target.spec.setup.is_some() {
        return Err(IrError::NestedSetup { test: test.id, setup: setup.test });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::values::{Method, RequestSpec, Scope, SetupRef, Template, TestName};

    fn draft(file: FileId, setup: Option<SetupRef>) -> TestDraft {
        TestDraft {
            name: TestName::new("t"),
            file,
            role: crate::values::Role::Test,
            request: RequestSpec { method: Method::Get, path: Template::new("/x"), headers: vec![], body: None },
            setup,
            extract: vec![],
            assertions: vec![],
            before_request: None,
        }
    }

    #[test]
    fn rejects_nested_setup() {
        let mut builder = ProgramBuilder::default();
        let file = builder.add_file(FileName::new("a")).unwrap();
        let root = builder.add_test(draft(file, None)).unwrap();
        let setup = builder.add_test(draft(file, Some(SetupRef { test: root, scope: Scope::File }))).unwrap();
        let nested = builder.add_test(draft(file, Some(SetupRef { test: setup, scope: Scope::File }))).unwrap();
        assert_eq!(builder.build(), Err(IrError::NestedSetup { test: nested, setup }));
    }

    #[test]
    fn template_renders_known_and_keeps_unknown() {
        let mut vars = crate::Vars::default();
        vars.insert(crate::VarName::new("id"), "42");
        assert_eq!(Template::new("/s/{{id}}/{{other}}").render(&vars), "/s/42/{{other}}");
    }
}
