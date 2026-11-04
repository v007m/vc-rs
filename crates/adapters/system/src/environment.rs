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
}

impl Environment for SystemEnvironment {
    fn var(&self, name: &str) -> Option<String> {
        self.vars.get(name).cloned()
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
}
