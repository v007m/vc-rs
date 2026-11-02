use crate::{Rule, Violation, Workspace, layers};

const ID: &str = "ARCH-UNKNOWN-CRATE";

/// Every workspace package has a row in the layer table.
pub struct UnknownCrate;

impl Rule for UnknownCrate {
    fn id(&self) -> &'static str {
        ID
    }

    fn check(&self, workspace: &Workspace) -> Vec<Violation> {
        let unknown = workspace.packages().iter().filter(|package| layers::allowed(&package.name).is_none());
        unknown
            .map(|package| {
                let message =
                    format!("{} is not in the layer table; add it to the LLD and to arch-tests", package.name);
                Violation::new(ID, &package.manifest, message)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::UnknownCrate;
    use crate::Rule;
    use crate::fixtures::{package, workspace};

    #[test]
    fn accepts_known_crates_and_rejects_others() {
        let ws =
            workspace(vec![package("vc-model", &[]), package("vc-mock-server", &[]), package("vc-extra", &[])], vec![]);
        let found = UnknownCrate.check(&ws);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].location, "crates/extra/Cargo.toml");
    }
}
