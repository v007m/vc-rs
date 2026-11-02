use syn::visit::{self, Visit};

use super::{Finding, scan_library};
use crate::{Rule, Violation, Workspace};

const ID: &str = "ARCH-PARAMS";
const MAX_PARAMS: usize = 3;

/// A function or method takes at most three parameters, not counting `self` (D109).
pub struct Params;

impl Rule for Params {
    fn id(&self) -> &'static str {
        ID
    }

    fn check(&self, workspace: &Workspace) -> Vec<Violation> {
        scan_library(workspace, ID, |_, file| {
            let mut finder = Finder(Vec::new());
            finder.visit_file(file);
            finder.0
        })
    }
}

struct Finder(Vec<Finding>);

impl Visit<'_> for Finder {
    fn visit_signature(&mut self, sig: &syn::Signature) {
        let count = sig.inputs.iter().filter(|arg| matches!(arg, syn::FnArg::Typed(_))).count();
        if count > MAX_PARAMS {
            let message =
                format!("`{}` takes {count} parameters (max {MAX_PARAMS}); group them in a struct", sig.ident);
            self.0.push((sig.ident.span(), message));
        }
        visit::visit_signature(self, sig);
    }
}

#[cfg(test)]
mod tests {
    use super::Params;
    use crate::fixtures::check_lib;

    #[test]
    fn accepts_three_parameters_plus_self() {
        let src = "fn f(a: u8, b: u8, c: u8) {} impl S { fn g(&self, a: u8, b: u8, c: u8) {} }";
        assert_eq!(check_lib(&Params, src), []);
        assert_eq!(check_lib(&Params, "#[test] fn t(a: u8, b: u8, c: u8, d: u8) {}"), []);
    }

    #[test]
    fn rejects_four_parameters_in_functions_methods_and_traits() {
        let src = "fn f(a: u8, b: u8, c: u8, d: u8) {}
            impl S { fn g(self, a: u8, b: u8, c: u8, d: u8) {} }
            trait T { fn h(&self, a: u8, b: u8, c: u8, d: u8); }";
        assert_eq!(check_lib(&Params, src).len(), 3);
    }
}
