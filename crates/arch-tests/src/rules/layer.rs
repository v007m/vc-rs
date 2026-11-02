use crate::{DepKind, Rule, Violation, Workspace, layers};

const ID: &str = "ARCH-LAYER";

/// Every normal or build dependency between workspace crates is allowed by the layer table.
pub struct Layer;

impl Rule for Layer {
    fn id(&self) -> &'static str {
        ID
    }

    fn check(&self, workspace: &Workspace) -> Vec<Violation> {
        let edges = workspace.internal_deps().into_iter().filter(|(_, dep)| dep.kind != DepKind::Dev);
        edges
            .filter(|(package, dep)| layers::allowed(&package.name).is_some_and(|ok| !ok.contains(&dep.name.as_str())))
            .map(|(package, dep)| {
                Violation::new(ID, &package.manifest, format!("{} must not depend on {}", package.name, dep.name))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::Layer;
    use crate::DepKind::{Build, Dev, Normal};
    use crate::Rule;
    use crate::fixtures::{package, workspace};

    #[test]
    fn allows_edges_in_the_table_and_dev_edges() {
        let ws = workspace(
            vec![
                package("vc-model", &[]),
                package("vc-interfaces", &[]),
                package("vc-compiler", &[("vc-model", Normal), ("vc-interfaces", Dev)]),
            ],
            vec![],
        );
        assert_eq!(Layer.check(&ws), []);
    }

    #[test]
    fn rejects_edges_the_table_forbids() {
        let ws = workspace(
            vec![
                package("vc-interfaces", &[]),
                package("vc-adapter-system", &[]),
                package("vc-compiler", &[("vc-interfaces", Normal)]),
                package("vc-adapter-http", &[("vc-adapter-system", Build)]),
            ],
            vec![],
        );
        let found: Vec<String> = Layer.check(&ws).iter().map(ToString::to_string).collect();
        assert_eq!(
            found,
            [
                "ARCH-LAYER crates/compiler/Cargo.toml: vc-compiler must not depend on vc-interfaces",
                "ARCH-LAYER crates/adapter-http/Cargo.toml: vc-adapter-http must not depend on vc-adapter-system",
            ]
        );
    }
}
