use std::collections::HashMap;
use std::sync::Arc;

use crate::ModelError;

/// An interned string: a 4-byte handle into a [`StringTable`].
///
/// Cheap to copy and compare. Two symbols from the same table are equal exactly when their
/// text is. They order by when their text was first interned, not alphabetically.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sym(u32);

/// The interned strings of one program.
///
/// The compiler interns into it while it builds the program. Once the program is frozen and
/// shared, only the `&self` lookups are reachable, so the table is read-only from then on.
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
    fn get_finds_without_interning() {
        let mut table = StringTable::new();
        assert!(table.is_empty());
        assert_eq!(table.get("staging"), None);
        let sym = table.intern("staging").unwrap();
        assert_eq!(table.get("staging"), Some(sym));
        assert_eq!(table.len(), 1);
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
