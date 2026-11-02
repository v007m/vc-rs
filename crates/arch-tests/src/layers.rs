//! The LLD's layer table: the workspace crates each crate may use as a normal or build dependency.
//! Crates for later milestones are listed too, so adding them does not touch this file.

const CORE: &[&str] = &["vc-model", "vc-diagnostics"];
const ADAPTER: &[&str] = &["vc-model", "vc-diagnostics", "vc-interfaces"];
const APP: &[&str] = &["vc-engine", "vc-contracts"];

const TABLE: &[(&str, &[&str])] = &[
    ("vc-model", &[]),
    ("vc-diagnostics", &["vc-model"]),
    ("vc-parser", CORE),
    ("vc-template", CORE),
    ("vc-interfaces", CORE),
    ("vc-contracts", &["vc-model"]),
    ("vc-compiler", &["vc-model", "vc-diagnostics", "vc-parser", "vc-template"]),
    ("vc-planner", CORE),
    ("vc-executor", &["vc-model", "vc-diagnostics", "vc-interfaces", "vc-template"]),
    ("vc-adapter-system", ADAPTER),
    ("vc-adapter-http", ADAPTER),
    ("vc-adapter-auth", ADAPTER),
    ("vc-adapter-secrets", ADAPTER),
    ("vc-adapter-hooks", ADAPTER),
    ("vc-adapter-store", ADAPTER),
    ("vc-adapter-reporters", ADAPTER),
    ("vc-adapter-notifiers", ADAPTER),
    ("vc-adapter-importers", ADAPTER),
    ("vc-adapter-plugins", &["vc-model", "vc-diagnostics", "vc-interfaces", "vc-contracts"]),
    ("vc-loadtest", &["vc-model", "vc-interfaces", "vc-executor"]),
    ("vc-engine", ENGINE),
    ("vc-tui", &["vc-engine"]),
    ("vc-mcp", APP),
    ("vc-server", APP),
    ("vc-cli", &["vc-engine", "vc-tui", "vc-mcp", "vc-server"]),
    ("vc-testkit", &["vc-model", "vc-interfaces"]),
    ("vc-arch-tests", &[]),
    ("vc-mock-server", &[]),
];

/// L0–L4: the engine is the composition root, the only crate that names adapters.
const ENGINE: &[&str] = &[
    "vc-model",
    "vc-diagnostics",
    "vc-parser",
    "vc-template",
    "vc-interfaces",
    "vc-contracts",
    "vc-compiler",
    "vc-planner",
    "vc-executor",
    "vc-loadtest",
    "vc-adapter-system",
    "vc-adapter-http",
    "vc-adapter-auth",
    "vc-adapter-secrets",
    "vc-adapter-hooks",
    "vc-adapter-store",
    "vc-adapter-reporters",
    "vc-adapter-notifiers",
    "vc-adapter-importers",
    "vc-adapter-plugins",
];

/// The crates `package` may depend on, or `None` when it is not in the table.
pub fn allowed(package: &str) -> Option<&'static [&'static str]> {
    TABLE.iter().find(|(name, _)| *name == package).map(|(_, deps)| *deps)
}

#[cfg(test)]
mod tests {
    use super::{TABLE, allowed};

    #[test]
    fn every_allowed_dependency_is_itself_in_the_table() {
        let unknown: Vec<_> = TABLE.iter().flat_map(|(_, deps)| deps.iter()).filter(|d| allowed(d).is_none()).collect();
        assert!(unknown.is_empty(), "{unknown:?}");
    }

    #[test]
    fn adapters_never_use_each_other() {
        let edges = TABLE.iter().filter(|(name, _)| name.starts_with("vc-adapter-"));
        assert!(edges.flat_map(|(_, deps)| deps.iter()).all(|d| !d.starts_with("vc-adapter-")));
    }
}
