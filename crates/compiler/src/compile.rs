use std::sync::Arc;

use vc_diagnostics::Diagnostics;
use vc_model::{Program, SourceFile, SourceFiles, StringTable};

use crate::CompileCache;

/// What one compile needs: the project's files and the cache the previous compile returned.
#[derive(Clone, Debug, Default)]
pub struct CompileInput {
    /// Every definition file, in any order; the compiler numbers them in path order.
    pub files: Vec<SourceFile>,
    /// The cache from the previous compile, or [`CompileCache::new`] when there is none.
    pub previous: CompileCache,
}

/// What one compile produced.
#[derive(Clone, Debug)]
pub struct Compiled {
    /// The frozen program; `Some` only when no diagnostic is an error.
    pub program: Option<Arc<Program>>,
    /// Every error, warning and info, in the order found; sort before showing them.
    pub diagnostics: Diagnostics,
    /// The files the diagnostics' spans point into, in path order.
    pub files: SourceFiles,
    /// The cache to persist and pass to the next compile.
    pub cache: CompileCache,
}

/// Compiles `input` into a program, or into the diagnostics that stop one.
///
/// Pure: the same input gives the same output, and nothing outside it is read or written.
#[must_use]
pub fn compile(input: CompileInput) -> Compiled {
    let mut sources = input.files;
    sources.sort_by(|a, b| a.path.cmp(&b.path));
    sources.dedup_by(|later, first| later.path == first.path);
    let cache = CompileCache::of_files(&sources);
    let files = file_table(sources);
    let diagnostics = Diagnostics::new();
    let program = (!diagnostics.has_errors()).then(|| Arc::new(Program::new(StringTable::new(), files.clone())));
    Compiled { program, diagnostics, files, cache }
}

/// The files numbered in the order given. A table cannot overflow in practice (it holds
/// `u32::MAX` files), so files past that limit are left out rather than failing the compile.
fn file_table(sources: Vec<SourceFile>) -> SourceFiles {
    let mut files = SourceFiles::new();
    for file in sources {
        if files.push(file).is_err() {
            break;
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use vc_model::{ContentHash, SourceFile, SourceFileId, SourcePath};

    use super::{CompileInput, compile};

    fn file(path: &str, text: &str) -> SourceFile {
        SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([1; 32]) }
    }

    #[test]
    fn numbers_files_in_path_order_and_drops_repeated_paths() {
        let files = vec![file("b.api.yaml", "b"), file("a.test.yaml", "a"), file("b.api.yaml", "again")];
        let compiled = compile(CompileInput { files, ..CompileInput::default() });
        let table: Vec<_> = (0..3).filter_map(|i| compiled.files.get(SourceFileId::new(i))).collect();
        assert_eq!(table.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["a.test.yaml", "b.api.yaml"]);
        assert_eq!(table[1].text, "b");
    }

    #[test]
    fn hands_out_a_program_holding_the_files_when_there_are_no_errors() {
        let compiled = compile(CompileInput { files: vec![file("x.api.yaml", "")], ..CompileInput::default() });
        let program = compiled.program.unwrap();
        assert_eq!(program.files, compiled.files);
        assert!(compiled.diagnostics.is_empty());
        assert_eq!(compiled.cache.len(), 1);
    }
}
