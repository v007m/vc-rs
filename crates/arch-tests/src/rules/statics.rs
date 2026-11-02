use syn::visit::{self, Visit};

use super::Finder;

/// No `static` items in library code: no globals, atomics or lazies; pass state in instead.
pub struct Statics;

syntax_rule!(Statics, "ARCH-STATIC");

impl Visit<'_> for Finder<Statics> {
    fn visit_item_static(&mut self, item: &syn::ItemStatic) {
        let message = format!("static `{}` is global state; pass it in through a constructor", item.ident);
        self.push(item.ident.span(), message);
        visit::visit_item_static(self, item);
    }
}

#[cfg(test)]
mod tests {
    use super::Statics;
    use crate::fixtures::check_lib;

    #[test]
    fn accepts_consts_and_test_statics() {
        assert_eq!(check_lib(&Statics, "const A: u8 = 1; #[cfg(test)] mod tests { static B: u8 = 0; }"), []);
    }

    #[test]
    fn rejects_statics_mutable_atomic_and_nested() {
        let src = "static A: AtomicU32 = AtomicU32::new(0);\nstatic mut B: u8 = 0;\nfn f() { static C: u8 = 0; }";
        let found: Vec<_> = check_lib(&Statics, src).into_iter().map(|v| v.location).collect();
        assert_eq!(found, ["crates/model/src/lib.rs:1", "crates/model/src/lib.rs:2", "crates/model/src/lib.rs:3"]);
    }
}
