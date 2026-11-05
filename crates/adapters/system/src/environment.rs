//! The environment snapshot (D133).

use std::collections::HashMap;

use vc_interfaces::Environment;

/// An [`Environment`] holding one snapshot of the process environment; values that are not
/// UTF-8 are left out.
#[derive(Clone, Debug, Default)]
pub struct SystemEnvironment {
    vars: HashMap<String, String>,
}

impl SystemEnvironment {
    /// The process environment as it is now.
    #[must_use]
    pub fn capture() -> Self {
        let utf8 = |(name, value): (std::ffi::OsString, std::ffi::OsString)| {
            Some((name.into_string().ok()?, value.into_string().ok()?))
        };
        Self { vars: std::env::vars_os().filter_map(utf8).collect() }
    }

    /// An environment holding exactly `vars`, such as one read from a `.env` file.
    #[must_use]
    pub fn from_vars(vars: impl IntoIterator<Item = (String, String)>) -> Self {
        Self { vars: vars.into_iter().collect() }
    }

    /// A snapshot of `environment`, such as one a test injected, so `.env` can fill under it.
    #[must_use]
    pub fn snapshot(environment: &dyn Environment) -> Self {
        Self::from_vars(
            environment.names().into_iter().filter_map(|name| Some((name.clone(), environment.var(&name)?))),
        )
    }

    /// This environment, with `fallback` filling the names it does not set: the project's
    /// `.env` under the process environment.
    #[must_use]
    pub fn with_fallback(mut self, fallback: impl IntoIterator<Item = (String, String)>) -> Self {
        for (name, value) in fallback {
            self.vars.entry(name).or_insert(value);
        }
        self
    }
}

impl Environment for SystemEnvironment {
    fn var(&self, name: &str) -> Option<String> {
        self.vars.get(name).cloned()
    }

    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.vars.keys().cloned().collect();
        names.sort_unstable();
        names
    }
}

#[cfg(test)]
mod tests {
    use vc_interfaces::Environment;
    use vc_testkit::environment_suite;

    use super::SystemEnvironment;

    #[test]
    fn system_environment_meets_the_environment_contract() {
        environment_suite(&mut |vars| {
            let vars = vars.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned()));
            Box::new(SystemEnvironment::from_vars(vars)) as Box<dyn Environment>
        });
    }

    #[test]
    fn the_capture_holds_the_process_environment() {
        let captured = SystemEnvironment::capture();
        let names: Vec<_> = std::env::vars_os().filter_map(|(name, _)| name.into_string().ok()).collect();
        assert_ne!(names.len(), 0);
        for name in names {
            assert_eq!(captured.var(&name), std::env::var(&name).ok(), "{name}");
        }
        assert_eq!(captured.var("VC_SURELY_NOT_SET_9F2C"), None);
    }

    #[test]
    fn a_fallback_fills_only_the_names_the_environment_does_not_set() {
        let own = SystemEnvironment::from_vars([("TOKEN".to_owned(), "process".to_owned())]);
        let fallback = [("TOKEN".to_owned(), "dotenv".to_owned()), ("BASE".to_owned(), "x".to_owned())];
        let merged = own.with_fallback(fallback);
        assert_eq!(merged.var("TOKEN").as_deref(), Some("process"));
        assert_eq!(merged.var("BASE").as_deref(), Some("x"));
    }

    #[test]
    fn a_snapshot_copies_another_environment() {
        let injected = vc_testkit::MemoryEnvironment::new(&[("A", "1"), ("B", "2")]);
        let copy = SystemEnvironment::snapshot(&injected);
        assert_eq!(copy.names(), ["A", "B"]);
        assert_eq!(copy.var("B").as_deref(), Some("2"));
    }
}
