use syn::visit::Visit;

use super::{Finding, scan_library};
use crate::{Rule, Violation, Workspace};

const ID: &str = "ARCH-LOCK";

/// The only files allowed to hold a lock (D113).
const ALLOWED_FILES: &[&str] = &["token_cache.rs", "redactor.rs"];

/// `Mutex`, `RwLock` and `arc-swap` appear only in the token cache and the redactor.
pub struct Locks;

impl Rule for Locks {
    fn id(&self) -> &'static str {
        ID
    }

    fn check(&self, workspace: &Workspace) -> Vec<Violation> {
        scan_library(workspace, ID, |source, file| {
            if ALLOWED_FILES.contains(&source.file_name()) {
                return Vec::new();
            }
            let mut finder = Finder(Vec::new());
            finder.visit_file(file);
            finder.0
        })
    }
}

struct Finder(Vec<Finding>);

impl Visit<'_> for Finder {
    fn visit_ident(&mut self, ident: &proc_macro2::Ident) {
        let name = ident.to_string();
        if matches!(name.as_str(), "Mutex" | "RwLock" | "arc_swap") || name.starts_with("ArcSwap") {
            let message =
                format!("`{name}` is a lock; locks are allowed only in token_cache.rs and redactor.rs (D113)");
            self.0.push((ident.span(), message));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Locks;
    use crate::fixtures::{check_files, check_lib, file};

    #[test]
    fn accepts_locks_in_allowed_files_and_tests() {
        let allowed = file("vc-model", "src/redactor.rs", false, "struct R(arc_swap::ArcSwap<u8>);");
        let test = file("vc-model", "tests/t.rs", true, "use std::sync::Mutex;");
        assert_eq!(check_files(&Locks, vec![allowed, test]), []);
        assert_eq!(check_lib(&Locks, "fn mutex_count() {}"), []);
    }

    #[test]
    fn rejects_locks_elsewhere() {
        let src = "use std::sync::Mutex;\nstruct S(tokio::sync::RwLock<u8>);\nstruct T(arc_swap::ArcSwapOption<u8>);";
        assert_eq!(check_lib(&Locks, src).len(), 4);
    }
}
