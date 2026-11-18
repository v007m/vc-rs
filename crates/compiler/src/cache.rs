use std::sync::Arc;

use serde::{Deserialize, Serialize};
use vc_model::{ContentHash, SourceFile, SourceFileId, SourceFiles, SourcePath, StringTable};

pub use memo::{LinkKey, Memo, Replay};

use crate::stage::FileUnit;

mod memo;

/// The first bytes of every cache blob.
const MAGIC: &[u8; 4] = b"VCCC";

/// The layout version; a blob with any other version is ignored.
const VERSION: u32 = 12;

/// The compiler release and build that wrote a blob; a blob from another one is ignored, so a
/// check added or fixed in a new release, or in a dev build of the same release, is not hidden by
/// units or a memo an older compiler kept. The build id is a hash of the sources that decide a
/// compile's output (`build.rs`, D333).
const BUILD: &str = concat!(env!("CARGO_PKG_VERSION"), "+", env!("VC_LINK_BUILD_ID"));

/// What the compiler keeps between runs so it can skip files that did not change (D132).
///
/// Opaque to its users: the engine only persists [`CompileCache::to_bytes`] and reads it back
/// with [`CompileCache::from_bytes`]. A blob from another version, or a damaged one, reads as an
/// empty cache, which only costs a full compile. The cache holds each file's parsed and
/// compiled unit and the names they use, so an unchanged file is not parsed again, in memory
/// (`vc watch`, the editors) and when read back from bytes. A unit with diagnostics is kept only
/// in memory: the bytes never hold diagnostics, so its file is compiled again after a read.
///
/// It also memoizes the last clean compile's output (D333): a compile of the same files, by
/// path, hash and number, in the same world gives it back without parsing or linking. Units
/// read back from bytes are decoded only when a compile needs them.
#[derive(Clone, Debug, Default)]
pub struct CompileCache {
    entries: Vec<CacheEntry>,
    strings: StringTable,
    memo: Option<Memo>,
    /// Whether the compile that made it found a unit or memo that did not read back, so the
    /// cache differs from the one it was given and is saved again.
    repaired: bool,
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
    /// What the per-file stages made of it; `None` when it must be compiled again or is only
    /// in `bytes`.
    pub unit: Option<Arc<FileUnit>>,
    /// The unit as a blob holds it, when it was read back from one.
    pub bytes: Option<Arc<[u8]>>,
}

impl CacheEntry {
    /// The file and whether its bytes would hold its unit: a unit with diagnostics is not stored.
    fn key(&self) -> (&SourcePath, &ContentHash, SourceFileId, bool) {
        let stored = self.bytes.is_some() || self.unit.as_deref().is_some_and(|unit| unit.diagnostics.is_empty());
        (&self.path, &self.hash, self.file, stored)
    }
}

/// What a cache holds for one file.
pub enum Kept {
    /// Its unit, and the bytes it was read from, if it was.
    Unit(Arc<FileUnit>, Option<Arc<[u8]>>),
    /// Nothing: the file is new, changed, or its unit had diagnostics.
    Missing,
    /// Bytes that do not read back as a unit.
    Damaged,
}

/// What a cache gives back for a compile's files and key.
pub enum Recall {
    /// The output the last clean compile of them made.
    Hit(Replay),
    /// Nothing: no memo, or one of other files or another key.
    Miss,
    /// A memo of them whose bytes do not read back.
    Damaged,
}

/// Caches are equal when they remember the same files, would store the same units and hold a
/// memo under the same key; the units, names and output are a memo of those. A compile that
/// found a unit or memo that did not read back makes a cache that differs from the one it was
/// given, so it is saved again and heals.
impl PartialEq for CompileCache {
    fn eq(&self, other: &Self) -> bool {
        let memo = |cache: &Self| cache.memo.as_ref().map(|memo| memo.key.clone());
        self.repaired == other.repaired
            && memo(self) == memo(other)
            && self.entries.iter().map(CacheEntry::key).eq(other.entries.iter().map(CacheEntry::key))
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
        Self { entries, strings, memo: None, repaired: false }
    }

    /// The cache, also holding `memo`.
    pub(crate) fn remembering(self, memo: Option<Memo>) -> Self {
        Self { memo, ..self }
    }

    /// The cache, marked when the compile that made it found a unit or memo that did not read back.
    pub(crate) fn repaired(self, repaired: bool) -> Self {
        Self { repaired, ..self }
    }

    /// What the cache holds for the file at `path` with `hash`, numbered `file`, its unit read
    /// back from its bytes when it has only those. The entries are in path order, as a compile
    /// makes them, so the search is a binary one.
    pub(crate) fn kept(&self, path: &SourcePath, hash: &ContentHash, file: SourceFileId) -> Kept {
        let at = self.entries.binary_search_by(|entry| entry.path.cmp(path)).ok();
        let found = at.and_then(|at| self.entries.get(at)).filter(|entry| entry.hash == *hash && entry.file == file);
        let Some(entry) = found else { return Kept::Missing };
        if let Some(unit) = &entry.unit {
            return Kept::Unit(Arc::clone(unit), entry.bytes.clone());
        }
        let Some(bytes) = &entry.bytes else { return Kept::Missing };
        read_all::<FileUnit>(bytes).map_or(Kept::Damaged, |unit| Kept::Unit(Arc::new(unit), Some(Arc::clone(bytes))))
    }

    /// The unit [`CompileCache::kept`] finds, if any.
    #[cfg(test)]
    pub(crate) fn unit(&self, path: &SourcePath, hash: &ContentHash, file: SourceFileId) -> Option<Arc<FileUnit>> {
        match self.kept(path, hash, file) {
            Kept::Unit(unit, _) => Some(unit),
            Kept::Missing | Kept::Damaged => None,
        }
    }

    /// The output memoized under `key` when the cache remembers exactly `files`, by path, hash
    /// and number.
    pub(crate) fn replay(&self, files: &SourceFiles, key: &LinkKey) -> Recall {
        let memo = self.memo.as_ref().filter(|memo| memo.key == *key && self.holds_exactly(files));
        let Some(memo) = memo else { return Recall::Miss };
        memo.replay(files).map_or(Recall::Damaged, Recall::Hit)
    }

    /// Whether the cache remembers `files` and no others, each at its path, hash and number.
    fn holds_exactly(&self, files: &SourceFiles) -> bool {
        let numbered = (0..).map(SourceFileId::new).map_while(|id| Some((id, files.get(id)?)));
        let mut pairs = numbered.zip(&self.entries);
        let same = |((id, source), entry): ((SourceFileId, &SourceFile), &CacheEntry)| {
            entry.path == source.path && entry.hash == source.hash && entry.file == id
        };
        files.len() == self.entries.len() && pairs.all(same)
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
    /// compile does not parse them again; a unit read back from bytes counts before it is read.
    #[must_use]
    pub fn units(&self) -> usize {
        self.entries.iter().filter(|entry| entry.unit.is_some() || entry.bytes.is_some()).count()
    }

    /// The cache as bytes: a magic number and the layout version, the length of a header and the
    /// header, then each stored unit, back to back, then the memo's output; all `postcard` (D155).
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let units: Vec<_> = self.entries.iter().map(unit_bytes).collect();
        let entries = self.entries.iter().zip(&units).map(|(entry, unit)| stored(entry, unit.as_deref())).collect();
        let output = self.memo.as_ref().and_then(|memo| memo.to_bytes().map(|bytes| (memo.key.clone(), bytes)));
        let output = output.and_then(|(key, bytes)| Some(((key, u32::try_from(bytes.len()).ok()?), bytes)));
        let (link, output) = output.unzip();
        let strings = self.strings.texts().map(str::to_owned).collect();
        let header = encode(&Body { build: BUILD.to_owned(), strings, entries, link }).unwrap_or_default();
        let mut out = MAGIC.to_vec();
        out.extend(VERSION.to_le_bytes());
        out.extend(u32::try_from(header.len()).unwrap_or(u32::MAX).to_le_bytes());
        out.extend(header);
        units.into_iter().flatten().for_each(|unit| out.extend(unit));
        out.extend(output.unwrap_or_default());
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
    /// The compiler release and build that wrote the blob.
    build: String,
    /// The string table, in symbol order.
    strings: Vec<String>,
    /// The files, in the cache's order.
    entries: Vec<Stored>,
    /// The memo's key and the length of its output, which follows the units.
    link: Option<(LinkKey, u32)>,
}

/// One file in the blob.
#[derive(Serialize, Deserialize)]
struct Stored {
    path: String,
    hash: [u8; 32],
    file: u32,
    /// The length of the unit's bytes among the units after the header: each unit is read on its
    /// own, so one that does not read back costs only its own file, not the whole cache. `None`
    /// for a unit with diagnostics.
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

/// The bytes of an entry's unit, when it is kept and has no diagnostics: the ones it was read
/// from, when it was.
fn unit_bytes(entry: &CacheEntry) -> Option<Vec<u8>> {
    if let Some(bytes) = &entry.bytes {
        return Some(bytes.to_vec());
    }
    let unit = entry.unit.as_deref().filter(|unit| unit.diagnostics.is_empty())?;
    encode(unit).filter(|bytes| u32::try_from(bytes.len()).is_ok())
}

fn decode(bytes: &[u8]) -> Option<CompileCache> {
    let rest = bytes.strip_prefix(MAGIC.as_slice())?;
    let (version, rest) = rest.split_first_chunk::<4>()?;
    if u32::from_le_bytes(*version) != VERSION {
        return None;
    }
    let (length, rest) = rest.split_first_chunk::<4>()?;
    let (header, mut units) = rest.split_at_checked(usize::try_from(u32::from_le_bytes(*length)).ok()?)?;
    let body: Body = read_all(header).filter(|body: &Body| body.build == BUILD)?;
    let strings = table(&body.strings)?;
    let mut entries = Vec::with_capacity(body.entries.len());
    for stored in &body.entries {
        entries.push(entry(stored, &mut units)?);
    }
    let length = body.link.as_ref().map_or(Some(0), |(_, length)| usize::try_from(*length).ok())?;
    let memo = body.link.map(|(key, _)| Memo::read(key, units));
    (units.len() == length).then(|| CompileCache::of(entries, strings).remembering(memo))
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

/// The entry `stored` describes, its unit's bytes taken off the front of `units`; they are
/// read only when a compile needs the unit.
fn entry(stored: &Stored, units: &mut &[u8]) -> Option<CacheEntry> {
    let path = SourcePath::data(&stored.path).ok()?;
    let length = stored.unit.map(usize::try_from).transpose().ok()?.unwrap_or(0);
    let (bytes, rest) = units.split_at_checked(length)?;
    *units = rest;
    let (hash, file) = (ContentHash::new(stored.hash), SourceFileId::new(stored.file));
    Some(CacheEntry { path, hash, file, unit: None, bytes: stored.unit.map(|_| Arc::from(bytes)) })
}

/// The value `bytes` hold, when they hold exactly one.
/// `value` as `postcard`, or `None` when it cannot be written.
fn encode<T: Serialize + ?Sized>(value: &T) -> Option<Vec<u8>> {
    postcard::serialize_with_flavor(value, Bytes(Vec::new())).ok()
}

/// A `postcard` output that copies a short write (a varint, a tag) byte by byte: through
/// `memcpy` such a write costs more than the bytes themselves, and most writes are short.
struct Bytes(Vec<u8>);

impl postcard::ser_flavors::Flavor for Bytes {
    type Output = Vec<u8>;

    fn try_extend(&mut self, data: &[u8]) -> postcard::Result<()> {
        if data.len() > 8 {
            self.0.extend_from_slice(data);
        } else {
            for &byte in data {
                self.0.push(byte);
            }
        }
        Ok(())
    }

    fn try_push(&mut self, data: u8) -> postcard::Result<()> {
        self.0.push(data);
        Ok(())
    }

    fn finalize(self) -> postcard::Result<Vec<u8>> {
        Ok(self.0)
    }
}

fn read_all<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Option<T> {
    match postcard::take_from_bytes(bytes) {
        Ok((value, [])) => Some(value),
        _ => None,
    }
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
