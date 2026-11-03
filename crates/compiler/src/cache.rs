use std::sync::Arc;

use vc_model::{ContentHash, SourceFileId, SourcePath, StringTable};

use crate::stage::FileUnit;

/// The first bytes of every cache blob.
const MAGIC: &[u8; 4] = b"VCCC";

/// The layout version; a blob with any other version is ignored.
const VERSION: u32 = 2;

/// What the compiler keeps between runs so it can skip files that did not change (D132).
///
/// Opaque to its users: the engine only persists [`CompileCache::to_bytes`] and reads it back
/// with [`CompileCache::from_bytes`]. A blob from another version, or a damaged one, reads as an
/// empty cache, which only costs a full compile. In memory (`vc watch`, the editors) the cache
/// also holds each file's parsed and compiled unit and the names they use, so an unchanged file
/// is not parsed again; the bytes hold only paths, hashes and file numbers.
#[derive(Clone, Debug, Default)]
pub struct CompileCache {
    entries: Vec<CacheEntry>,
    strings: StringTable,
}

/// One file the cache remembers.
#[derive(Clone, Debug)]
pub struct CacheEntry {
    /// Where the file is.
    pub path: SourcePath,
    /// Its content hash.
    pub hash: ContentHash,
    /// Its number in the file table, which every span in its unit points to.
    pub file: SourceFileId,
    /// What the per-file stages made of it; `None` once read back from bytes.
    pub unit: Option<Arc<FileUnit>>,
}

impl CacheEntry {
    const fn key(&self) -> (&SourcePath, &ContentHash, SourceFileId) {
        (&self.path, &self.hash, self.file)
    }
}

/// Caches are equal when they remember the same files; units and names are a memo of those.
impl PartialEq for CompileCache {
    fn eq(&self, other: &Self) -> bool {
        self.entries.iter().map(CacheEntry::key).eq(other.entries.iter().map(CacheEntry::key))
    }
}

impl Eq for CompileCache {}

impl CompileCache {
    /// An empty cache: the next compile starts from scratch.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A cache of `entries`, whose units intern their names in `strings`.
    pub(crate) const fn of(entries: Vec<CacheEntry>, strings: StringTable) -> Self {
        Self { entries, strings }
    }

    /// The unit kept for the file at `path` with `hash`, numbered `file`.
    pub(crate) fn unit(&self, path: &SourcePath, hash: &ContentHash, file: SourceFileId) -> Option<Arc<FileUnit>> {
        let entry = self.entries.iter().find(|entry| entry.key() == (path, hash, file))?;
        entry.unit.clone()
    }

    /// The names the kept units use; a compile that reuses them interns on top of these.
    pub(crate) const fn strings(&self) -> &StringTable {
        &self.strings
    }

    /// How many files the cache remembers.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache remembers nothing.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The cache as bytes, starting with a magic number and the layout version.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend(VERSION.to_le_bytes());
        for entry in &self.entries {
            let text = entry.path.as_str().as_bytes();
            out.extend(u32::try_from(text.len()).unwrap_or(u32::MAX).to_le_bytes());
            out.extend(text);
            out.extend(entry.hash.as_bytes());
            out.extend(entry.file.index().to_le_bytes());
        }
        out
    }

    /// The cache `bytes` hold, or an empty cache when they are from another version or damaged.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        decode(bytes).unwrap_or_default()
    }
}

fn decode(bytes: &[u8]) -> Option<CompileCache> {
    let mut reader = Reader(bytes);
    if reader.take(MAGIC.len())? != MAGIC || reader.u32()? != VERSION {
        return None;
    }
    let mut entries = Vec::new();
    while !reader.0.is_empty() {
        let len = usize::try_from(reader.u32()?).ok()?;
        let path = SourcePath::new(std::str::from_utf8(reader.take(len)?).ok()?).ok()?;
        let hash = ContentHash::new(reader.take(32)?.try_into().ok()?);
        let file = SourceFileId::new(reader.u32()?);
        entries.push(CacheEntry { path, hash, file, unit: None });
    }
    Some(CompileCache::of(entries, StringTable::new()))
}

/// Reads a blob front to back; every read fails cleanly past the end.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let (head, rest) = self.0.split_at_checked(len)?;
        self.0 = rest;
        Some(head)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use indexmap::IndexMap;
    use vc_diagnostics::Diagnostics;
    use vc_model::{ContentHash, SourceFileId, SourcePath, StringTable};

    use super::{CacheEntry, CompileCache, MAGIC};
    use crate::stage::FileUnit;

    fn entry(path: &str, byte: u8, file: u32) -> CacheEntry {
        let unit = FileUnit { kind: None, root: None, parts: IndexMap::new(), diagnostics: Diagnostics::new() };
        let path = SourcePath::new(path).unwrap();
        CacheEntry {
            path,
            hash: ContentHash::new([byte; 32]),
            file: SourceFileId::new(file),
            unit: Some(Arc::new(unit)),
        }
    }

    fn cache() -> CompileCache {
        CompileCache::of(vec![entry("a.api.yaml", 1, 0), entry("services/b.test.yaml", 2, 1)], StringTable::new())
    }

    #[test]
    fn bytes_round_trip_without_the_units() {
        let cache = cache();
        let read = CompileCache::from_bytes(&cache.to_bytes());
        assert_eq!(read, cache);
        let (path, hash) = (SourcePath::new("services/b.test.yaml").unwrap(), ContentHash::new([2; 32]));
        assert!(cache.unit(&path, &hash, SourceFileId::new(1)).is_some());
        assert!(read.unit(&path, &hash, SourceFileId::new(1)).is_none());
        assert_eq!(CompileCache::from_bytes(&CompileCache::new().to_bytes()), CompileCache::new());
    }

    #[test]
    fn finds_a_unit_only_for_the_same_path_hash_and_number() {
        let cache = cache();
        let path = SourcePath::new("a.api.yaml").unwrap();
        assert!(cache.unit(&path, &ContentHash::new([1; 32]), SourceFileId::new(0)).is_some());
        assert!(cache.unit(&path, &ContentHash::new([9; 32]), SourceFileId::new(0)).is_none());
        assert!(cache.unit(&path, &ContentHash::new([1; 32]), SourceFileId::new(1)).is_none());
        assert_ne!(cache, CompileCache::new());
    }

    #[test]
    fn another_version_reads_as_empty() {
        let mut bytes = cache().to_bytes();
        bytes[MAGIC.len()] = 1;
        assert!(CompileCache::from_bytes(&bytes).is_empty());
    }

    #[test]
    fn damaged_bytes_read_as_empty() {
        let bytes = cache().to_bytes();
        for cut in [0, 3, 8, 11, bytes.len() - 1] {
            assert!(CompileCache::from_bytes(&bytes[..cut]).is_empty(), "cut at {cut}");
        }
        let mut bad_path = bytes;
        bad_path[12] = b'/';
        assert!(CompileCache::from_bytes(&bad_path).is_empty());
        assert!(CompileCache::from_bytes(b"not a cache at all").is_empty());
    }
}
