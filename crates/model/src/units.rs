use serde::{Deserialize, Serialize};

/// A length of time written in a definition file (`250ms`, `2s`, `1m`).
///
/// Plain data: the template crate parses the text; this type only carries the value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Duration(std::time::Duration);

impl Duration {
    /// Wraps a standard duration.
    #[must_use]
    pub const fn new(value: std::time::Duration) -> Self {
        Self(value)
    }

    /// The standard duration.
    #[must_use]
    pub const fn get(self) -> std::time::Duration {
        self.0
    }
}

/// A number of bytes written in a definition file (`4kb`, `1mb`).
///
/// Plain data: the template crate parses the text; this type only carries the value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Size(u64);

impl Size {
    /// A size of `bytes` bytes.
    #[must_use]
    pub const fn from_bytes(bytes: u64) -> Self {
        Self(bytes)
    }

    /// The size in bytes.
    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{Duration, Size};

    #[test]
    fn units_carry_their_value() {
        let std = std::time::Duration::from_millis(250);
        assert_eq!(Duration::new(std).get(), std);
        assert_eq!(Size::from_bytes(4096).bytes(), 4096);
        assert!(Size::from_bytes(1) < Size::from_bytes(2));
    }
}
