use std::collections::HashMap;
use std::sync::Arc;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::ModelError;

/// An interned string: a 4-byte handle into a [`StringTable`].
///
/// Cheap to copy and compare. Two symbols from the same table are equal exactly when their
/// text is. They order by when their text was first interned, not alphabetically.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Sym(u32);

/// The interned strings of one program.
///
/// The compiler interns into it while it builds the program. Once the program is frozen and
/// shared, only the `&self` lookups are reachable, so the table is read-only from then on. It
/// serializes as its texts in symbol order; reading back a list with a text twice fails.
#[derive(Debug, Clone, Default)]
pub struct StringTable {
    strings: Vec<Arc<str>>,
    index: HashMap<Arc<str>, Sym>,
}

impl StringTable {
    /// An empty table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The symbol for `text`, interning it first when the table has not seen it.
    ///
    /// # Errors
    ///
    /// [`ModelError::StringTableFull`] when the table holds as many strings as a [`Sym`] can address.
    pub fn intern(&mut self, text: &str) -> Result<Sym, ModelError> {
        if let Some(&sym) = self.index.get(text) {
            return Ok(sym);
        }
        let len = self.strings.len();
        let sym = u32::try_from(len).map(Sym).map_err(|_| ModelError::StringTableFull(len))?;
        let shared: Arc<str> = Arc::from(text);
        self.strings.push(Arc::clone(&shared));
        self.index.insert(shared, sym);
        Ok(sym)
    }

    /// The symbol for `text` if it has been interned; never adds it.
    #[must_use]
    pub fn get(&self, text: &str) -> Option<Sym> {
        self.index.get(text).copied()
    }

    /// The text behind `sym`, a symbol this table handed out.
    ///
    /// `None` when `sym` is past the end of the table. A symbol from another table is not
    /// detected: it may resolve to unrelated text.
    #[must_use]
    pub fn resolve(&self, sym: Sym) -> Option<&str> {
        let index = usize::try_from(sym.0).ok()?;
        self.strings.get(index).map(AsRef::as_ref)
    }

    /// Every string in the table, in symbol order: interning them in this order into an empty
    /// table gives each one the same symbol again.
    pub fn texts(&self) -> impl Iterator<Item = &str> {
        self.strings.iter().map(AsRef::as_ref)
    }

    /// How many distinct strings the table holds.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.strings.len()
    }

    /// Whether the table holds no strings.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

impl Serialize for StringTable {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.texts())
    }
}

impl<'de> Deserialize<'de> for StringTable {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let texts = Vec::<String>::deserialize(deserializer)?;
        let mut table = Self::new();
        for (index, text) in texts.iter().enumerate() {
            table.intern(text).map_err(D::Error::custom)?;
            if table.len() != index + 1 {
                return Err(D::Error::custom(format!("`{text}` is in the string table twice")));
            }
        }
        Ok(table)
    }
}

#[cfg(test)]
mod tests {
    use super::StringTable;

    #[test]
    fn interned_text_resolves_back() {
        let mut table = StringTable::new();
        let sym = table.intern("users.crud").unwrap();
        assert_eq!(table.resolve(sym), Some("users.crud"));
    }

    #[test]
    fn same_text_gives_the_same_symbol_once() {
        let mut table = StringTable::new();
        let first = table.intern("userId").unwrap();
        let other = table.intern("orderId").unwrap();
        assert_eq!(table.intern("userId").unwrap(), first);
        assert_ne!(first, other);
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn symbols_order_by_interning() {
        let mut table = StringTable::new();
        let b = table.intern("b").unwrap();
        let a = table.intern("a").unwrap();
        assert!(b < a);
    }

    #[test]
    fn texts_come_in_symbol_order() {
        let mut table = StringTable::new();
        for text in ["b", "a", "b"] {
            table.intern(text).unwrap();
        }
        assert_eq!(table.texts().collect::<Vec<_>>(), ["b", "a"]);
    }

    #[test]
    fn get_finds_without_interning() {
        let mut table = StringTable::new();
        assert!(table.is_empty());
        assert_eq!(table.get("staging"), None);
        let sym = table.intern("staging").unwrap();
        assert_eq!(table.get("staging"), Some(sym));
        assert_eq!(table.len(), 1);
    }

    #[test]
    fn reads_back_with_the_same_symbols_and_never_a_text_twice() {
        let mut table = StringTable::new();
        let syms = ["b", "a"].map(|text| table.intern(text).unwrap());
        let read: StringTable = serde_json::from_str(&serde_json::to_string(&table).unwrap()).unwrap();
        assert_eq!(syms.map(|sym| read.resolve(sym)), [Some("b"), Some("a")]);
        assert!(serde_json::from_str::<StringTable>(r#"["a","b","a"]"#).is_err());
    }

    #[test]
    fn out_of_range_symbol_does_not_resolve() {
        let mut big = StringTable::new();
        big.intern("a").unwrap();
        let foreign = big.intern("b").unwrap();
        let mut small = StringTable::new();
        small.intern("a").unwrap();
        assert_eq!(small.resolve(foreign), None);
    }
}
