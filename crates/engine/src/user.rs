//! The user config as the layer under the project's (D129): a user setting counts only where
//! `vibranium.config.yaml` and its profiles leave the value unset.

use vc_interfaces::{ConfigEntry, ConfigSource, ConfigSourceError};
use vc_model::Program;

/// Whether the project's config sets a key itself.
type Sets = fn(&Program) -> bool;

/// Every key the user config may set but the `vars.` ones, and how to tell the project sets it.
const PROJECT: &[(&str, Sets)] = &[
    ("profile", |program| program.profiles.values().any(|profile| profile.default)),
    ("defaults.concurrency", |program| program.config.defaults.concurrency.is_some()),
    ("defaults.perHost", |program| program.config.defaults.per_host.is_some()),
    ("defaults.timeout", |program| program.config.defaults.timeout.is_some()),
    ("reports.dir", |program| program.config.reports_dir.is_some()),
    ("reports.formats", |program| !program.config.report_formats.is_empty()),
];

/// The prefix of the keys that set config vars.
const VARS: &str = "vars.";

/// `user`'s entries without the ones the project's config overrides.
pub struct UnderProject<'a> {
    user: &'a dyn ConfigSource,
    program: &'a Program,
}

impl<'a> UnderProject<'a> {
    /// The user layer `user`, under `program`'s config.
    pub const fn new(user: &'a dyn ConfigSource, program: &'a Program) -> Self {
        Self { user, program }
    }

    fn kept(&self, entry: &ConfigEntry) -> bool {
        if let Some(name) = entry.key.strip_prefix(VARS) {
            return !defines(self.program, name);
        }
        PROJECT.iter().find(|(key, _)| *key == entry.key).is_none_or(|(_, sets)| !sets(self.program))
    }
}

impl ConfigSource for UnderProject<'_> {
    fn entries(&self) -> Result<Vec<ConfigEntry>, ConfigSourceError> {
        Ok(self.user.entries()?.into_iter().filter(|entry| self.kept(entry)).collect())
    }
}

/// Whether the config or any profile defines the var `name`.
fn defines(program: &Program, name: &str) -> bool {
    let named = |var: &vc_model::VarName| program.str(var.sym()) == Some(name);
    let config = &program.config.vars;
    config.values.keys().any(named)
        || config.by_profile.values().any(|values| values.keys().any(named))
        || program.profiles.values().any(|profile| profile.vars.keys().any(named))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use vc_interfaces::{ConfigEntry, ConfigSource, ConfigSourceError};
    use vc_testkit::{MemoryCompileCache, MemoryEnvironment, MemorySources};

    use super::UnderProject;
    use crate::EngineBuilder;

    const CONFIG: &[u8] = b"syntaxVersion: 2\ndefaults: {timeout: 5s}\nvars: {userId: 1}\n\
        profiles:\n  - id: dev\n    default: true\n    vars: {region: eu}\n";

    struct Layer(Vec<ConfigEntry>);

    impl ConfigSource for Layer {
        fn entries(&self) -> Result<Vec<ConfigEntry>, ConfigSourceError> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn the_project_config_wins_over_the_user_config() {
        let engine = EngineBuilder::new(PathBuf::from("/project"))
            .sources(Arc::new(MemorySources::new(&[("vibranium.config.yaml", CONFIG)])))
            .compile_cache(Arc::new(MemoryCompileCache::new()))
            .environment(Arc::new(MemoryEnvironment::new(&[])))
            .build()
            .unwrap();
        let (program, _) = engine.program().unwrap();
        let pairs = [
            ("profile", "prod"),
            ("defaults.timeout", "9s"),
            ("defaults.concurrency", "3"),
            ("vars.userId", "2"),
            ("vars.region", "us"),
            ("vars.mine", "x"),
        ];
        let user = Layer(pairs.iter().map(|(key, value)| ConfigEntry::new(*key, *value, "~/config.yaml")).collect());
        let kept: Vec<String> =
            UnderProject::new(&user, &program).entries().unwrap().into_iter().map(|entry| entry.key).collect();
        assert_eq!(kept, ["defaults.concurrency", "vars.mine"]);
    }
}
