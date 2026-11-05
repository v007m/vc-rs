use std::collections::BTreeSet;
use std::sync::Arc;

use vc_diagnostics::Diagnostics;
use vc_model::{Date, Program, SourceFile, SourceFileId, SourceFiles, SourcePath, StringTable};

use crate::cache::CacheEntry;
use crate::link::DataWanted;
use crate::parts::Tools;
use crate::{CompileCache, link, stage};

/// What one compile needs: the project's files, the cache the previous compile returned, and
/// what the files cannot say themselves (today's date, the project's other files).
#[derive(Clone, Debug, Default)]
#[expect(clippy::exhaustive_structs, reason = "options struct filled with `..Default::default()` (D160)")]
pub struct CompileInput {
    /// Every definition file, in any order; the compiler numbers them in path order.
    pub files: Vec<SourceFile>,
    /// The cache from the previous compile, or [`CompileCache::new`] when there is none.
    pub previous: CompileCache,
    /// Today's date in UTC, for an expired `skip` (V3104); `None` leaves that lint out.
    pub today: Option<Date>,
    /// The project's other files, relative to the root, that a `$file:` may point at; the
    /// definition files count without being listed. `None` leaves `$file` paths unchecked (V2004).
    pub assets: Option<BTreeSet<SourcePath>>,
    /// The project root's `.env.schema`, which declares the secret names (D165); `None` leaves
    /// undeclared `${secret.X}` (V2106) and `${env.X}` of a secret (V2107) unchecked.
    pub secret_schema: Option<SourceFile>,
}

/// What one compile produced.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Compiled {
    /// The frozen program; `Some` only when no diagnostic is an error.
    pub program: Option<Arc<Program>>,
    /// Every error, warning and info, in the order found; sort before showing them.
    pub diagnostics: Diagnostics,
    /// The files the diagnostics' spans point into, in path order.
    pub files: SourceFiles,
    /// The cache to persist and pass to the next compile.
    pub cache: CompileCache,
    /// The data files the project names that were not in [`CompileInput::files`] (D173): every
    /// file under `datasets/` that [`CompileInput::assets`] lists, each `datasets.files` entry,
    /// and each literal `$file` an `examples` or `forEach` reads. The engine reads them (an
    /// outside one only while its canonical path stays under its root, else V1012) and compiles
    /// again with them; until then their rows are unchecked and read at run time.
    pub data_wanted: Vec<DataWanted>,
    /// The hook files the project's `function:` references reach, directly or through relative
    /// imports, that were not in [`CompileInput::files`] (D172). The engine reads them and
    /// compiles again with them, until none is wanted; until then their exports are unchecked
    /// and [`Program::hooks`] lacks them.
    pub hooks_wanted: Vec<SourcePath>,
}

/// Compiles `input` into a program, or into the diagnostics that stop one.
///
/// Pure: the same input gives the same output, and nothing outside it is read or written.
/// A file whose path, hash and number match the previous cache is not parsed again.
#[must_use]
pub fn compile(input: CompileInput) -> Compiled {
    let mut sources = input.files;
    sources.sort_by(|a, b| a.path.cmp(&b.path));
    sources.dedup_by(|later, first| later.path == first.path);
    let files = file_table(sources);
    let (entries, strings) = units(&files, &input.previous);
    let mut diagnostics = Diagnostics::new();
    for unit in entries.iter().filter_map(|entry| entry.unit.as_deref()) {
        diagnostics.extend(unit.diagnostics.iter().cloned());
    }
    let declared = input.secret_schema.map(|schema| vc_model::schema_names(&schema.text));
    let world = link::World {
        strings: &strings,
        today: input.today,
        assets: input.assets.as_ref(),
        secrets: declared.as_deref(),
    };
    let linked = link::link(&entries, &files, &world);
    diagnostics.extend(linked.diagnostics);
    let (program, data_wanted, hooks_wanted) = (linked.program, linked.data_wanted, linked.hooks_wanted);
    let cache = CompileCache::of(entries, strings);
    Compiled { program, diagnostics, files, cache, data_wanted, hooks_wanted }
}

/// Runs the per-file stages over every file the previous cache does not hold.
fn units(files: &SourceFiles, previous: &CompileCache) -> (Vec<CacheEntry>, StringTable) {
    let tools = Tools::new();
    let mut strings = previous.strings().clone();
    let mut entries = Vec::with_capacity(files.len());
    for (file, source) in numbered(files) {
        let kept = previous.unit(&source.path, &source.hash, file);
        let unit = kept.unwrap_or_else(|| Arc::new(stage::unit(file, source, (&tools, &mut strings))));
        entries.push(CacheEntry { path: source.path.clone(), hash: source.hash, file, unit: Some(unit) });
    }
    (entries, strings)
}

/// Each file with its number.
fn numbered(files: &SourceFiles) -> impl Iterator<Item = (SourceFileId, &SourceFile)> {
    (0..).map(SourceFileId::new).map_while(|id| Some((id, files.get(id)?)))
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
    use std::sync::Arc;

    use vc_diagnostics::Code;
    use vc_model::{ContentHash, SourceFile, SourceFileId, SourceFiles, SourcePath};

    use super::{CompileInput, compile};
    use crate::CompileCache;
    use crate::stage::FileUnit;

    fn kept(cache: &CompileCache, files: &SourceFiles, file: SourceFileId) -> Option<Arc<FileUnit>> {
        let source = files.get(file)?;
        cache.unit(&source.path, &source.hash, file)
    }

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

    #[test]
    fn holds_back_the_program_and_reports_each_file_on_errors() {
        let files = vec![file("x.api.yaml", "nme: x\n"), file("y.api.yaml", "apis: {}\n")];
        let compiled = compile(CompileInput { files, ..CompileInput::default() });
        assert!(compiled.program.is_none());
        let codes: Vec<_> = compiled.diagnostics.iter().map(|d| d.code).collect();
        assert_eq!(codes, [Code::V1004, Code::V2302]);
    }

    #[test]
    fn reuses_the_units_of_unchanged_files() {
        let first = compile(CompileInput { files: vec![file("x.api.yaml", "name: x\n")], ..CompileInput::default() });
        let unit = kept(&first.cache, &first.files, SourceFileId::new(0)).unwrap();
        let again = compile(CompileInput {
            files: vec![file("x.api.yaml", "name: x\n")],
            previous: first.cache,
            ..CompileInput::default()
        });
        assert!(Arc::ptr_eq(&unit, &kept(&again.cache, &again.files, SourceFileId::new(0)).unwrap()));
        let mut changed = file("x.api.yaml", "name: y\n");
        changed.hash = ContentHash::new([2; 32]);
        let third = compile(CompileInput { files: vec![changed], previous: again.cache, ..CompileInput::default() });
        assert!(!Arc::ptr_eq(&unit, &kept(&third.cache, &third.files, SourceFileId::new(0)).unwrap()));
    }
}
