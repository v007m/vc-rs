use std::sync::Arc;

use serde::{Deserialize, Serialize};
use vc_model::{ContentHash, SourceFileId, SourcePath, StringTable};

use crate::stage::FileUnit;

/// The first bytes of every cache blob.
const MAGIC: &[u8; 4] = b"VCCC";

/// The layout version; a blob with any other version is ignored.
const VERSION: u32 = 4;

/// What the compiler keeps between runs so it can skip files that did not change (D132).
///
/// Opaque to its users: the engine only persists [`CompileCache::to_bytes`] and reads it back
/// with [`CompileCache::from_bytes`]. A blob from another version, or a damaged one, reads as an
/// empty cache, which only costs a full compile. The cache holds each file's parsed and
/// compiled unit and the names they use, so an unchanged file is not parsed again, in memory
/// (`vc watch`, the editors) and when read back from bytes. A unit with diagnostics is kept only
/// in memory: the bytes never hold diagnostics, so its file is compiled again after a read.
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
    /// What the per-file stages made of it; `None` when it must be compiled again.
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

    /// How many of the files it remembers the cache holds a compiled unit for, so the next
    /// compile does not parse them again.
    #[must_use]
    pub fn units(&self) -> usize {
        self.entries.iter().filter(|entry| entry.unit.is_some()).count()
    }

    /// The cache as bytes: a magic number and the layout version, the length of a JSON header
    /// and the header, then each stored unit's JSON, back to back.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let units: Vec<_> = self.entries.iter().map(unit_json).collect();
        let entries = self.entries.iter().zip(&units).map(|(entry, unit)| stored(entry, unit.as_deref())).collect();
        let body = Body { strings: self.strings.texts().map(str::to_owned).collect(), entries };
        let header = serde_json::to_vec(&body).unwrap_or_default();
        let mut out = MAGIC.to_vec();
        out.extend(VERSION.to_le_bytes());
        out.extend(u32::try_from(header.len()).unwrap_or(u32::MAX).to_le_bytes());
        out.extend(header);
        units.into_iter().flatten().for_each(|unit| out.extend(unit));
        out
    }

    /// The cache `bytes` hold, or an empty cache when they are from another version or damaged.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        decode(bytes).unwrap_or_default()
    }
}

/// The blob after its header.
#[derive(Serialize, Deserialize)]
struct Body {
    /// The string table, in symbol order.
    strings: Vec<String>,
    /// The files, in the cache's order.
    entries: Vec<Stored>,
}

/// One file in the blob.
#[derive(Serialize, Deserialize)]
struct Stored {
    path: String,
    hash: [u8; 32],
    file: u32,
    /// The length of the unit's JSON among the units after the header: each unit is read on its
    /// own, so one too deeply nested for the JSON reader to take back costs only its own file,
    /// not the whole cache. `None` for a unit with diagnostics.
    unit: Option<u32>,
}

fn stored(entry: &CacheEntry, unit: Option<&[u8]>) -> Stored {
    Stored {
        path: entry.path.as_str().to_owned(),
        hash: *entry.hash.as_bytes(),
        file: entry.file.index(),
        unit: unit.and_then(|unit| u32::try_from(unit.len()).ok()),
    }
}

/// The JSON of an entry's unit, when it is kept and has no diagnostics.
fn unit_json(entry: &CacheEntry) -> Option<Vec<u8>> {
    let unit = entry.unit.as_deref().filter(|unit| unit.diagnostics.is_empty())?;
    serde_json::to_vec(unit).ok().filter(|json| u32::try_from(json.len()).is_ok())
}

fn decode(bytes: &[u8]) -> Option<CompileCache> {
    let rest = bytes.strip_prefix(MAGIC.as_slice())?;
    let (version, rest) = rest.split_first_chunk::<4>()?;
    if u32::from_le_bytes(*version) != VERSION {
        return None;
    }
    let (length, rest) = rest.split_first_chunk::<4>()?;
    let (json, mut units) = rest.split_at_checked(usize::try_from(u32::from_le_bytes(*length)).ok()?)?;
    let body: Body = serde_json::from_slice(json).ok()?;
    let strings = table(&body.strings)?;
    let mut entries = Vec::with_capacity(body.entries.len());
    for stored in &body.entries {
        entries.push(entry(stored, &mut units)?);
    }
    units.is_empty().then(|| CompileCache::of(entries, strings))
}

/// The table `texts` came from: each text interned in order must get the next symbol.
fn table(texts: &[String]) -> Option<StringTable> {
    let mut table = StringTable::new();
    for (index, text) in texts.iter().enumerate() {
        table.intern(text).ok()?;
        if table.len() != index + 1 {
            return None;
        }
    }
    Some(table)
}

/// The entry `stored` describes, its unit read off the front of `units`.
fn entry(stored: &Stored, units: &mut &[u8]) -> Option<CacheEntry> {
    let path = SourcePath::new(&stored.path).ok()?;
    let length = stored.unit.map(usize::try_from).transpose().ok()?.unwrap_or(0);
    let (json, rest) = units.split_at_checked(length)?;
    *units = rest;
    let unit = stored.unit.and_then(|_| serde_json::from_slice::<FileUnit>(json).ok());
    let (hash, file) = (ContentHash::new(stored.hash), SourceFileId::new(stored.file));
    Some(CacheEntry { path, hash, file, unit: unit.map(Arc::new) })
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
