//! The connections a factory's transports hold open (D675), for the load run's Hosts panel:
//! each connection the connector opens holds one [`Open`] until hyper drops it.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// How many connections are open, shared by the transports one factory builds. A count, not a
/// lock: each connection adds one when it opens and takes it back when it closes.
#[derive(Clone, Default)]
pub struct Connections {
    open: Arc<AtomicU64>,
}

impl Connections {
    /// A count of none.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The connections open now.
    #[must_use]
    pub fn count(&self) -> u64 {
        self.open.load(Ordering::Relaxed)
    }

    /// Counts one more connection until the [`Open`] it gives is dropped.
    pub(crate) fn open(&self) -> Open {
        self.open.fetch_add(1, Ordering::Relaxed);
        Open { open: Arc::clone(&self.open) }
    }
}

impl fmt::Debug for Connections {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Connections").field(&self.count()).finish()
    }
}

/// Two counts are equal when they are the same count.
impl PartialEq for Connections {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.open, &other.open)
    }
}

impl Eq for Connections {}

/// One open connection, counted until it is dropped.
#[derive(Debug)]
pub struct Open {
    open: Arc<AtomicU64>,
}

impl Drop for Open {
    fn drop(&mut self) {
        self.open.fetch_sub(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::Connections;

    #[test]
    fn a_connection_counts_until_it_is_dropped() {
        let connections = Connections::new();
        let shared = connections.clone();
        let (first, second) = (connections.open(), shared.open());
        assert_eq!(connections.count(), 2, "clones share the count");
        drop(first);
        assert_eq!(connections.count(), 1);
        drop(second);
        assert_eq!(connections.count(), 0);
        assert_ne!(connections, Connections::new(), "a new count is another count");
    }
}
