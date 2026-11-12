//! The playground's environment: the worker has no process environment, so `${env.X}` finds
//! nothing and a var's default applies.

use std::sync::Arc;

use vc_interfaces::Environment;

/// An environment with no variables.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(clippy::exhaustive_structs, reason = "unit adapter; every user is in this repo (D160)")]
pub struct PageEnvironment;

impl Environment for PageEnvironment {
    fn var(&self, _name: &str) -> Option<String> {
        None
    }

    fn names(&self) -> Vec<String> {
        Vec::new()
    }
}

/// The page's environment, as the run's port.
pub fn page() -> Arc<dyn Environment> {
    Arc::new(PageEnvironment)
}

#[cfg(test)]
mod tests {
    use vc_interfaces::Environment;

    use super::PageEnvironment;

    /// The environment contract's case of an empty environment: no name is set.
    #[test]
    fn the_page_environment_sets_no_name() {
        assert_eq!(PageEnvironment.var("API_HOST"), None);
        assert_eq!(PageEnvironment.var(""), None);
        assert_eq!(PageEnvironment.names(), Vec::<String>::new());
    }
}
