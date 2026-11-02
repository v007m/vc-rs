use crate::{DepKind, Rule, Violation, Workspace};

const ID: &str = "ARCH-ANYHOW";

/// `anyhow` is a normal dependency of `vc-cli` only; library crates use their own `thiserror` enum.
pub struct Anyhow;

impl Rule for Anyhow {
    fn id(&self) -> &'static str {
        ID
    }

    fn check(&self, workspace: &Workspace) -> Vec<Violation> {
        let libraries = workspace.packages().iter().filter(|package| package.name != "vc-cli");
        libraries
            .filter(|package| package.dependencies.iter().any(|d| d.name == "anyhow" && d.kind == DepKind::Normal))
            .map(|package| {
                let message =
                    format!("{} depends on anyhow, which only vc-cli may use; add a thiserror enum", package.name);
                Violation::new(ID, &package.manifest, message)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::Anyhow;
    use crate::DepKind::{Dev, Normal};
    use crate::Rule;
    use crate::fixtures::{package, workspace};

    #[test]
    fn allows_cli_and_dev_use_and_rejects_libraries() {
        let ws = workspace(
            vec![
                package("vc-cli", &[("anyhow", Normal)]),
                package("vc-planner", &[("anyhow", Dev)]),
                package("vc-engine", &[("anyhow", Normal)]),
            ],
            vec![],
        );
        let found: Vec<_> = Anyhow.check(&ws).into_iter().map(|v| v.location).collect();
        assert_eq!(found, ["crates/engine/Cargo.toml"]);
    }
}
