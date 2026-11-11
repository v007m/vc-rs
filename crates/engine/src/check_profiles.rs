//! `vc check`'s V3101 for a project whose runs have no active profile: it has profiles, none is
//! marked `default: true`, and no settings layer (the user config, `.env`, `VIB_PROFILE`) names
//! one. The compiler cannot see the layers; the engine reads them.

use vc_diagnostics::{Code, Diagnostic};
use vc_model::{Program, SourceFiles, SourceSpan};
use vc_parser::Syntax;

use crate::Engine;
use crate::check_alerts;

impl Engine {
    /// The V3101 error when a run with no `--profile` would have no active profile.
    pub(crate) fn no_active_profile(&self, program: &Program, files: &SourceFiles) -> Option<Diagnostic> {
        if program.profiles.is_empty() || program.profiles.values().any(|profile| profile.default) {
            return None;
        }
        let named = self.layered(&[], &[]).ok().and_then(|layered| layered.settings.profiles);
        if named.is_some_and(|named| !named.names.is_empty()) {
            return None;
        }
        let names: Vec<&str> = program.profiles.keys().filter_map(|id| program.str(id.sym())).collect();
        let first = names.first().copied().unwrap_or("dev");
        let span = profiles_key(files)?;
        let message = "no profile is marked `default: true`, so a run without `--profile` has none active";
        let help = format!(
            "mark one `default: true` (`- id: {first}` then `default: true`), or name one with `--profile` or \
             `VIB_PROFILE` on every run; the profiles are {}",
            names.join(", ")
        );
        Some(Diagnostic::new(Code::V3101, span, message).label("none marked default").help(help))
    }
}

/// The span of the config's `profiles` key.
fn profiles_key(files: &SourceFiles) -> Option<SourceSpan> {
    let (id, file) = check_alerts::config(files)?;
    let (root, _) = vc_parser::parse(id, Syntax::of(&file.path), &file.text);
    root?.entry("profiles").map(|entry| entry.key_span)
}
