use crate::{DepKind, Rule, Violation, Workspace};

const ID: &str = "ARCH-TESTKIT-DEV";

/// `vc-testkit` is only ever a dev-dependency (D112).
pub struct TestkitDev;

impl Rule for TestkitDev {
    fn id(&self) -> &'static str {
        ID
    }

    fn check(&self, workspace: &Workspace) -> Vec<Violation> {
        let uses =
            workspace.internal_deps().into_iter().filter(|(_, d)| d.name == "vc-testkit" && d.kind != DepKind::Dev);
        uses.map(|(package, dep)| {
            let message = format!(
                "{} uses vc-testkit as a {} dependency; it may only be a dev-dependency",
                package.name, dep.kind
            );
            Violation::new(ID, &package.manifest, message)
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::TestkitDev;
    use crate::DepKind::{Build, Dev, Normal};
    use crate::Rule;
    use crate::fixtures::{package, workspace};

    #[test]
    fn accepts_dev_use_and_rejects_normal_and_build_use() {
        let ws = workspace(
            vec![
                package("vc-testkit", &[]),
                package("vc-planner", &[("vc-testkit", Dev)]),
                package("vc-executor", &[("vc-testkit", Normal)]),
                package("vc-engine", &[("vc-testkit", Build)]),
            ],
            vec![],
        );
        let found: Vec<_> = TestkitDev.check(&ws).into_iter().map(|v| v.location).collect();
        assert_eq!(found, ["crates/executor/Cargo.toml", "crates/engine/Cargo.toml"]);
    }
}
