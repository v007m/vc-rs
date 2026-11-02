use vc_model::{ContentHash, SourceFile, SourcePath};

/// The first bytes of every cache blob.
const MAGIC: &[u8; 4] = b"VCCC";

/// The layout version; a blob with any other version is ignored.
const VERSION: u32 = 1;

/// What the compiler keeps between runs so it can skip files that did not change (D132).
///
/// Opaque to its users: the engine only persists [`CompileCache::to_bytes`] and reads it back
/// with [`CompileCache::from_bytes`]. A blob from another version, or a damaged one, reads as an
/// empty cache, which only costs a full compile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CompileCache {
    entries: Vec<(SourcePath, ContentHash)>,
}

impl CompileCache {
    /// An empty cache: the next compile starts from scratch.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A cache remembering each file's path and hash.
    pub(crate) fn of_files(files: &[SourceFile]) -> Self {
        Self { entries: files.iter().map(|file| (file.path.clone(), file.hash)).collect() }
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
        for (path, hash) in &self.entries {
            let text = path.as_str().as_bytes();
            out.extend(u32::try_from(text.len()).unwrap_or(u32::MAX).to_le_bytes());
            out.extend(text);
            out.extend(hash.as_bytes());
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
        entries.push((path, hash));
    }
    Some(CompileCache { entries })
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
    use vc_model::{ContentHash, SourceFile, SourcePath};

    use super::{CompileCache, MAGIC};

    fn cache() -> CompileCache {
        let file = |path: &str, byte| SourceFile {
            path: SourcePath::new(path).unwrap(),
            text: String::new(),
            hash: ContentHash::new([byte; 32]),
        };
        CompileCache::of_files(&[file("a.api.yaml", 1), file("services/b.test.yaml", 2)])
    }

    #[test]
    fn bytes_round_trip() {
        let cache = cache();
        assert_eq!(CompileCache::from_bytes(&cache.to_bytes()), cache);
        assert_eq!(CompileCache::from_bytes(&CompileCache::new().to_bytes()), CompileCache::new());
    }

    #[test]
    fn another_version_reads_as_empty() {
        let mut bytes = cache().to_bytes();
        bytes[MAGIC.len()] = 2;
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
