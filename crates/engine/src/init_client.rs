//! A new project from the web UI's onboarding (D716, D717): the Init wizard's choices checked,
//! then `vc init`'s files previewed with a digest of the changes, and written only with that
//! digest, each file only while it is still absent or the text it was made from. Every file is
//! at a path the engine fixes under its root, the folder `vc ui` was started in, never one a
//! client names. The Demo is the same with the sample files, which call `vc demo`'s service.
//! A refusal never echoes a client's value.

use crate::authoring::refused;
use crate::edit::{Changes, changes_digest, stale_unless};
use crate::init::Start;
use crate::{Engine, EngineError};

/// The sample's api file, which says the demo project is here already.
const SAMPLE_API: &str = "services/sample/sample.api.yaml";

/// Why a write found the changes differ from the preview's.
const STALE: &str = "the new project: nothing was written: the folder or the choices changed since the preview";

/// The longest project name, profile or service a client may give.
const WORD_MAX: usize = 64;

/// The longest base URL a client may give.
const URL_MAX: usize = 2048;

/// The Init wizard's choices, as a client sends them; each is optional.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built by the apps from their own requests; every user is in this repo (D160)"
)]
pub struct InitChoices {
    /// The project's name; the folder's when `None`.
    pub name: Option<String>,
    /// The first service, a lower-case file ID word, given its own api file.
    pub service: Option<String>,
    /// The `baseUrl` var: an `http` or `https` URL with no user name or password in it.
    pub base_url: Option<String>,
    /// The default profile's ID; `default` when `None`.
    pub profile: Option<String>,
    /// With the sample api and test files.
    pub sample: bool,
}

/// A new project's files, previewed or written.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct InitChange {
    /// The files, each new but `.gitignore`.
    pub changes: Changes,
    /// The digest of the changes, which the write gives back.
    pub digest: String,
    /// The folder they go in, with the home folder as `~`.
    pub root: String,
}

impl Engine {
    /// A new project in the engine's root with `choices`: previewed, or written when `write`
    /// gives the digest the preview gave.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] for a choice that does not fit, a project that is here already or a
    /// change the guard refuses; [`EngineError::Stale`] when the changes differ from the preview's.
    pub fn client_init(&self, choices: &InitChoices, write: Option<&str>) -> Result<InitChange, EngineError> {
        checked(choices)?;
        let start = Start {
            sample: choices.sample,
            name: choices.name.as_deref(),
            profile: choices.profile.as_deref(),
            service: choices.service.as_deref(),
            base_url: choices.base_url.as_deref(),
        };
        let changes = self.init_from(&start)?;
        self.init_written(changes, write)
    }

    /// The demo project in the engine's root, `vc init`'s with the sample: previewed, or written
    /// when `write` gives the digest the preview gave; no changes when its sample is here already.
    ///
    /// # Errors
    ///
    /// As [`Engine::client_init`].
    pub fn client_demo(&self, write: Option<&str>) -> Result<InitChange, EngineError> {
        if self.existing(std::iter::once(SAMPLE_API))?.is_empty() {
            return self.client_init(&InitChoices { sample: true, ..InitChoices::default() }, write);
        }
        self.init_written(Changes::default(), write)
    }

    /// `changes` with their digest, written once `write` gives it.
    fn init_written(&self, changes: Changes, write: Option<&str>) -> Result<InitChange, EngineError> {
        let digest = changes_digest(&changes);
        if let Some(expected) = write {
            stale_unless(expected, &digest, STALE)?;
            self.write_each(&changes, true)?;
        }
        Ok(InitChange { changes, digest, root: self.shown_path(self.root()) })
    }
}

/// Refuses a choice that does not fit, without echoing it.
fn checked(choices: &InitChoices) -> Result<(), EngineError> {
    let fits = [
        ("name", choices.name.as_deref().is_none_or(name), "a name of at most 64 characters, on one line"),
        ("profile", choices.profile.as_deref().is_none_or(|id| word(id, false)), "a lower-case word, such as `dev`"),
        ("service", choices.service.as_deref().is_none_or(|id| word(id, true)), "a lower-case word, such as `users`"),
        ("baseUrl", choices.base_url.as_deref().is_none_or(base_url), "an http or https URL with no user or password"),
    ];
    if let Some((choice, _, help)) = fits.iter().find(|(_, fit, _)| !fit) {
        return Err(refused(format!("the new project: that {choice} does not fit"), help));
    }
    if choices.sample && choices.service.as_deref() == Some("sample") {
        return Err(refused("the new project: the sample has that service", "pick another service"));
    }
    Ok(())
}

/// Whether `text` is a project name: not blank, at most [`WORD_MAX`] characters, no control
/// characters.
fn name(text: &str) -> bool {
    !text.trim().is_empty() && text.chars().count() <= WORD_MAX && !text.chars().any(char::is_control)
}

/// Whether `id` is a profile (no dashes) or a service (dashes allowed).
fn word(id: &str, dashes: bool) -> bool {
    id.len() <= WORD_MAX && vc_compiler::is_file_id(id) && !id.contains('.') && (dashes || !id.contains('-'))
}

/// Whether `url` is an `http` or `https` URL with a host, no user or password, no template and
/// nothing that is not printable.
fn base_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) else { return false };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    url.len() <= URL_MAX
        && !authority.is_empty()
        && !authority.contains('@')
        && !url.contains("${")
        && url.chars().all(|part| part.is_ascii_graphic())
}

#[cfg(test)]
#[path = "init_client_tests.rs"]
mod tests;
