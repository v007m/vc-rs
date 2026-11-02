use serde::Serialize;

use crate::Sym;

/// Declares an ID newtype over an interned string.
macro_rules! sym_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(Sym);

        impl $name {
            /// Wraps an interned string as this kind of ID.
            #[must_use]
            pub const fn new(sym: Sym) -> Self {
                Self(sym)
            }

            /// The interned text of this ID.
            #[must_use]
            pub const fn sym(self) -> Sym {
                self.0
            }
        }
    };
}

/// Declares an ID newtype over an index into one of the program's tables.
macro_rules! index_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
        #[serde(transparent)]
        pub struct $name(u32);

        impl $name {
            /// Wraps an index into the table this ID points into.
            #[must_use]
            pub const fn new(index: u32) -> Self {
                Self(index)
            }

            /// The index this ID points at.
            #[must_use]
            pub const fn index(self) -> u32 {
                self.0
            }
        }
    };
}

sym_id! {
    /// A file's ID, from its `id:` key or derived from its path (D139): `users.crud`.
    FileId
}

sym_id! {
    /// An api's full ID, `<fileId>#<id>` (D138): `users#create`.
    ApiId
}

sym_id! {
    /// A test's full ID, `<fileId>#<id>` (D138): `users.crud#createsMember`.
    TestId
}

sym_id! {
    /// An auth definition's ID: `shopOauth`.
    AuthId
}

sym_id! {
    /// A profile's name: `staging`.
    ProfileId
}

sym_id! {
    /// A var's name: `userId`.
    VarName
}

sym_id! {
    /// A dataset's ID: `users.seed`.
    DatasetId
}

index_id! {
    /// An action, as an index into the program's action table.
    ActionId
}

index_id! {
    /// A source file, as an index into the program's file table.
    SourceFileId
}

#[cfg(test)]
mod tests {
    use super::{ActionId, ApiId, SourceFileId, TestId};
    use crate::StringTable;

    #[test]
    fn ids_keep_their_symbol() {
        let mut table = StringTable::new();
        let sym = table.intern("users.crud#createsMember").unwrap();
        let test = TestId::new(sym);
        assert_eq!(test.sym(), sym);
        assert_eq!(table.resolve(test.sym()), Some("users.crud#createsMember"));
    }

    #[test]
    fn ids_of_different_kinds_can_share_text() {
        let mut table = StringTable::new();
        let sym = table.intern("users#create").unwrap();
        assert_eq!(ApiId::new(sym).sym(), TestId::new(sym).sym());
    }

    #[test]
    fn index_ids_keep_their_index_and_order() {
        assert_eq!(ActionId::new(7).index(), 7);
        assert!(SourceFileId::new(1) < SourceFileId::new(2));
    }
}
