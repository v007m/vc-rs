use crate::{ModelError, Pos, SourceFile, SourceFileId};

/// The source files of one check or run, each at the [`SourceFileId`] it was given.
///
/// Also turns byte offsets into lines and columns, and hands out a line's text, which is what
/// renderers need to show an excerpt.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceFiles {
    entries: Vec<Entry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    file: SourceFile,
    /// The byte offset where each line starts; the first is always 0.
    line_starts: Vec<usize>,
}

impl SourceFiles {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `file` and returns its ID; IDs count up from 0 in the order files are added.
    ///
    /// # Errors
    ///
    /// [`ModelError::FileTableFull`] when the table holds as many files as a [`SourceFileId`]
    /// can address.
    pub fn push(&mut self, file: SourceFile) -> Result<SourceFileId, ModelError> {
        let len = self.entries.len();
        let id = u32::try_from(len).map(SourceFileId::new).map_err(|_| ModelError::FileTableFull(len))?;
        let newlines = file.text.match_indices('\n').map(|(at, _)| at + 1);
        let line_starts = std::iter::once(0).chain(newlines).collect();
        self.entries.push(Entry { file, line_starts });
        Ok(id)
    }

    /// The file with this ID, or `None` when the ID is not from this table.
    #[must_use]
    pub fn get(&self, id: SourceFileId) -> Option<&SourceFile> {
        self.entry(id).map(|entry| &entry.file)
    }

    /// The line and column of byte `offset` in file `id`.
    ///
    /// `None` when the file is unknown, or the offset is past the end of the text or inside a
    /// multi-byte character. The end of the text is a valid offset.
    #[must_use]
    pub fn position(&self, id: SourceFileId, offset: usize) -> Option<Pos> {
        let entry = self.entry(id)?;
        let index = entry.line_starts.partition_point(|&start| start <= offset).checked_sub(1)?;
        let start = *entry.line_starts.get(index)?;
        let column = entry.file.text.get(start..offset)?.chars().count() + 1;
        Some(Pos::new(u32::try_from(index + 1).ok()?, u32::try_from(column).ok()?, offset))
    }

    /// The text of 1-based `line` in file `id`, without its line ending.
    #[must_use]
    pub fn line(&self, id: SourceFileId, line: u32) -> Option<&str> {
        let entry = self.entry(id)?;
        let index = usize::try_from(line).ok()?.checked_sub(1)?;
        let start = *entry.line_starts.get(index)?;
        let end = entry.line_starts.get(index + 1).map_or(entry.file.text.len(), |&next| next - 1);
        let text = entry.file.text.get(start..end)?;
        Some(text.strip_suffix('\r').unwrap_or(text))
    }

    /// How many files there are.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether there are none.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn entry(&self, id: SourceFileId) -> Option<&Entry> {
        self.entries.get(usize::try_from(id.index()).ok()?)
    }
}

#[cfg(test)]
mod tests {
    use super::SourceFiles;
    use crate::{ContentHash, Pos, SourceFile, SourceFileId, SourcePath};

    fn files(text: &str) -> (SourceFiles, SourceFileId) {
        let mut files = SourceFiles::new();
        let path = SourcePath::new("users.api.yaml").unwrap();
        let id = files.push(SourceFile { path, text: text.to_owned(), hash: ContentHash::new([0; 32]) }).unwrap();
        (files, id)
    }

    #[test]
    fn ids_count_up_and_find_their_file() {
        let (mut table, first) = files("a");
        let second = table.push(table.get(first).unwrap().clone()).unwrap();
        assert_eq!((first.index(), second.index(), table.len()), (0, 1, 2));
        assert!(!table.is_empty() && SourceFiles::new().is_empty());
        assert_eq!(table.get(SourceFileId::new(2)), None);
    }

    #[test]
    fn position_counts_lines_and_character_columns() {
        let (table, id) = files("id: users\nname: Zoë Ü\r\nx");
        assert_eq!(table.position(id, 0), Some(Pos::START));
        assert_eq!(table.position(id, 4), Some(Pos::new(1, 5, 4)));
        assert_eq!(table.position(id, 10), Some(Pos::new(2, 1, 10)));
        assert_eq!(table.position(id, 20), Some(Pos::new(2, 10, 20)));
        assert_eq!(table.position(id, 23), Some(Pos::new(2, 12, 23)));
        assert_eq!(table.position(id, 25), Some(Pos::new(3, 1, 25)));
        assert_eq!(table.position(id, 26), Some(Pos::new(3, 2, 26)));
    }

    #[test]
    fn position_rejects_offsets_off_the_text() {
        let (table, id) = files("Zoë");
        assert_eq!(table.position(id, 3), None);
        assert_eq!(table.position(id, 5), None);
        assert_eq!(table.position(SourceFileId::new(9), 0), None);
    }

    #[test]
    fn line_returns_text_without_its_ending() {
        let (table, id) = files("one\r\ntwo\n\nlast");
        let lines: Vec<_> = (0..=5).map(|n| table.line(id, n)).collect();
        assert_eq!(lines, [None, Some("one"), Some("two"), Some(""), Some("last"), None]);
        let (trailing, id) = files("one\n");
        assert_eq!((trailing.line(id, 2), trailing.line(id, 3)), (Some(""), None));
    }
}
