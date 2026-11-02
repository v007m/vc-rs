use syn::visit::{self, Visit};

use super::Finder;

const MAX_METHODS: usize = 7;

/// A trait has at most seven methods (D114); split a bigger one into role interfaces.
pub struct TraitSize;

syntax_rule!(TraitSize, "ARCH-TRAIT-SIZE");

impl Visit<'_> for Finder<TraitSize> {
    fn visit_item_trait(&mut self, item: &syn::ItemTrait) {
        let count = item.items.iter().filter(|i| matches!(i, syn::TraitItem::Fn(_))).count();
        if count > MAX_METHODS {
            let message = format!("trait `{}` has {count} methods (max {MAX_METHODS}); split it", item.ident);
            self.push(item.ident.span(), message);
        }
        visit::visit_item_trait(self, item);
    }
}

#[cfg(test)]
mod tests {
    use super::TraitSize;
    use crate::fixtures::check_lib;

    fn trait_with(methods: usize) -> String {
        let body = (0..methods).map(|i| format!("fn m{i}(&self);")).collect::<Vec<_>>().join(" ");
        format!("trait T {{ type A; const B: u8; {body} }}")
    }

    #[test]
    fn accepts_seven_methods() {
        assert_eq!(check_lib(&TraitSize, &trait_with(7)), []);
    }

    #[test]
    fn rejects_eight_methods() {
        assert_eq!(check_lib(&TraitSize, &trait_with(8)).len(), 1);
    }
}
