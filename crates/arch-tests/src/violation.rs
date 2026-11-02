use std::fmt;

/// One broken rule: a stable rule ID, a workspace-relative location and a message.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Violation {
    /// Stable rule ID, such as `ARCH-LAYER`.
    pub rule: &'static str,
    /// Workspace-relative file, with `:line` for source findings.
    pub location: String,
    /// What is wrong.
    pub message: String,
}

impl Violation {
    /// Creates a violation of `rule` at `location`.
    pub fn new(rule: &'static str, location: impl Into<String>, message: impl Into<String>) -> Self {
        Self { rule, location: location.into(), message: message.into() }
    }
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}: {}", self.rule, self.location, self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::Violation;

    #[test]
    fn displays_rule_location_and_message() {
        let v =
            Violation::new("ARCH-LAYER", "crates/compiler/Cargo.toml", "vc-compiler must not depend on vc-interfaces");
        assert_eq!(
            v.to_string(),
            "ARCH-LAYER crates/compiler/Cargo.toml: vc-compiler must not depend on vc-interfaces"
        );
    }
}
