use crate::{Rule, Violation, Workspace};

const ID: &str = "ARCH-FILE-SIZE";
const MAX_LINES: usize = 400;

/// A source file has at most 400 lines; test-only and `@generated` files are exempt (D114).
pub struct FileSize;

impl Rule for FileSize {
    fn id(&self) -> &'static str {
        ID
    }

    fn check(&self, workspace: &Workspace) -> Vec<Violation> {
        let sources = workspace.sources().iter().filter(|s| !s.origin.test_only && !s.generated);
        sources
            .filter(|source| source.lines > MAX_LINES)
            .map(|source| {
                let message = format!("{} lines (max {MAX_LINES}); split the file", source.lines);
                Violation::new(ID, &source.origin.path, message)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::FileSize;
    use crate::fixtures::{check_files, file, lib_file};

    fn lines(count: usize, header: &str) -> String {
        format!("{header}\n{}", "fn f() {}\n".repeat(count - 1))
    }

    #[test]
    fn accepts_short_generated_and_test_files() {
        let short = lib_file("vc-model", &lines(400, ""));
        let generated = file("vc-model", "src/gen.rs", false, &lines(500, "// @generated"));
        let test = file("vc-model", "tests/big.rs", true, &lines(500, ""));
        assert_eq!(check_files(&FileSize, vec![short, generated, test]), []);
    }

    #[test]
    fn rejects_a_long_library_file() {
        let found = check_files(&FileSize, vec![lib_file("vc-model", &lines(401, ""))]);
        assert_eq!(found[0].to_string(), "ARCH-FILE-SIZE crates/model/src/lib.rs: 401 lines (max 400); split the file");
    }
}
