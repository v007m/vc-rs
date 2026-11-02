use std::sync::mpsc::{self, Sender};

use vc_interfaces::{CompileCacheError, CompileCacheStore};

/// A compile cache held in memory by a small thread that owns the blob, so the store is
/// shared without a lock. Clones talk to the same thread, so a test can keep one to look at
/// what the engine saved.
#[derive(Clone, Debug)]
pub struct MemoryCompileCache {
    requests: Sender<Request>,
}

#[derive(Debug)]
enum Request {
    Load(Sender<Option<Vec<u8>>>),
    Save(Vec<u8>, Sender<()>),
}

impl MemoryCompileCache {
    /// An empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::holding(None)
    }

    /// A store that already holds `blob`.
    #[must_use]
    pub fn holding(blob: Option<Vec<u8>>) -> Self {
        let (requests, inbox) = mpsc::channel();
        std::thread::spawn(move || {
            let mut blob = blob;
            for request in inbox {
                match request {
                    Request::Load(reply) => drop(reply.send(blob.clone())),
                    Request::Save(bytes, reply) => {
                        blob = Some(bytes);
                        reply.send(()).ok();
                    }
                }
            }
        });
        Self { requests }
    }
}

impl Default for MemoryCompileCache {
    fn default() -> Self {
        Self::new()
    }
}

impl CompileCacheStore for MemoryCompileCache {
    fn load(&self) -> Result<Option<Vec<u8>>, CompileCacheError> {
        let (reply, answer) = mpsc::channel();
        let answered = self.requests.send(Request::Load(reply)).ok().and_then(|()| answer.recv().ok());
        answered.ok_or_else(|| CompileCacheError::Read { location: LOCATION.to_owned(), reason: STOPPED.to_owned() })
    }

    fn save(&self, bytes: &[u8]) -> Result<(), CompileCacheError> {
        let (reply, answer) = mpsc::channel();
        let answered = self.requests.send(Request::Save(bytes.to_vec(), reply)).ok().and_then(|()| answer.recv().ok());
        answered.ok_or_else(|| CompileCacheError::Write { location: LOCATION.to_owned(), reason: STOPPED.to_owned() })
    }
}

/// How the store names itself in errors.
const LOCATION: &str = "memory";

/// Why a store answers nothing: its thread is gone, which only a panic in it can cause.
const STOPPED: &str = "the store's thread stopped";

#[cfg(test)]
mod tests {
    use vc_interfaces::CompileCacheStore;

    use super::MemoryCompileCache;
    use crate::compile_cache_store_suite;

    #[test]
    fn passes_the_compile_cache_store_suite() {
        compile_cache_store_suite(&mut || Box::new(MemoryCompileCache::new()));
    }

    #[test]
    fn clones_share_one_blob() {
        let store = MemoryCompileCache::holding(Some(b"old".to_vec()));
        let other = store.clone();
        assert_eq!(other.load().unwrap(), Some(b"old".to_vec()));
        store.save(b"new").unwrap();
        assert_eq!(other.load().unwrap(), Some(b"new".to_vec()));
    }
}
