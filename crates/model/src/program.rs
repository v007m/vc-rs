use crate::{SourceFiles, StringTable, Sym};

/// Everything `vc check` produced, frozen and shared read-only as `Arc<Program>`.
///
/// The compiler builds it and hands it out once there are no errors; nothing changes it after
/// that. It grows a table per kind of definition (apis, tests, actions and the rest) as the
/// compiler learns to fill them; see the LLD, "The program".
#[derive(Clone, Debug, Default)]
pub struct Program {
    strings: StringTable,
    /// Path, text and hash of every source file, by [`crate::SourceFileId`].
    pub files: SourceFiles,
}

impl Program {
    /// A program over its interned strings and its source files.
    #[must_use]
    pub const fn new(strings: StringTable, files: SourceFiles) -> Self {
        Self { strings, files }
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
        assert!(program.files.is_empty());
    }
}
