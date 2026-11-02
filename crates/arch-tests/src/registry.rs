use crate::{Violation, Workspace, rules};

/// One architecture rule with a stable ID.
pub trait Rule {
    /// Stable rule ID printed with every violation, such as `ARCH-LAYER`.
    fn id(&self) -> &'static str;
    /// Every violation of this rule in `workspace`.
    fn check(&self, workspace: &Workspace) -> Vec<Violation>;
}

/// The table of rules run against the workspace.
pub struct RuleRegistry {
    rules: Vec<Box<dyn Rule>>,
}

impl RuleRegistry {
    /// Every rule the workspace must follow.
    #[must_use]
    pub fn standard() -> Self {
        Self { rules: rules::all() }
    }

    /// The registered rule IDs, in registration order.
    #[must_use]
    pub fn ids(&self) -> Vec<&'static str> {
        self.rules.iter().map(|rule| rule.id()).collect()
    }

    /// Runs every rule; the result is sorted and free of duplicates.
    #[must_use]
    pub fn check(&self, workspace: &Workspace) -> Vec<Violation> {
        let mut found: Vec<Violation> = self.rules.iter().flat_map(|rule| rule.check(workspace)).collect();
        found.sort();
        found.dedup();
        found
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::RuleRegistry;
    use crate::DepKind::Normal;
    use crate::fixtures::{package, workspace};

    #[test]
    fn rule_ids_are_unique_and_prefixed() {
        let ids = RuleRegistry::standard().ids();
        assert_eq!(ids.len(), 15, "{ids:?}");
        assert_eq!(ids.iter().collect::<BTreeSet<_>>().len(), ids.len(), "{ids:?}");
        assert!(ids.iter().all(|id| id.starts_with("ARCH-")), "{ids:?}");
    }

    #[test]
    fn check_collects_violations_from_every_rule() {
        let ws = workspace(vec![package("vc-model", &[("anyhow", Normal)]), package("vc-unknown", &[])], vec![]);
        let rules: BTreeSet<_> = RuleRegistry::standard().check(&ws).into_iter().map(|v| v.rule).collect();
        assert_eq!(rules, BTreeSet::from(["ARCH-ANYHOW", "ARCH-UNKNOWN-CRATE"]));
    }
}
