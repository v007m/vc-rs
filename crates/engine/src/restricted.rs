//! A resolver for templates that may read only some namespaces, such as an alert channel's
//! settings: each namespace it reads is a reader by name, and every other reference, and any
//! expression, is refused with one message.

use serde_json::Value;
use vc_interfaces::{Environment, Secrets};
use vc_model::{CelProgram, Namespace, Program, Reference, Segment};
use vc_template::{Chance, RenderError, Resolve};

/// Reads the value a name has in one namespace.
pub type Read<'r> = Box<dyn Fn(&str) -> Result<Value, RenderError> + 'r>;

/// Answers `${ns.name…}` for the namespaces in `reads` only; anything else is refused with
/// `only`.
pub struct Restricted<'r> {
    /// The namespaces read, each with its reader.
    pub reads: Vec<(Namespace, Read<'r>)>,
    /// What the template may read, as the refusal says it.
    pub only: &'r str,
}

impl Resolve for Restricted<'_> {
    fn resolve(&self, reference: &Reference) -> Result<Value, RenderError> {
        let read = self.reads.iter().find(|(ns, _)| *ns == reference.ns);
        let named = reference.path.segments.split_first();
        let (Some((ns, read)), Some((Segment::Key(name), rest))) = (read, named) else {
            return Err(RenderError::invalid(self.only));
        };
        let root = vc_template::root_name(*ns).unwrap_or_default();
        vc_template::select(root, &read(name)?, rest)
    }

    fn evaluate(&self, _program: &CelProgram) -> Result<Value, RenderError> {
        Err(RenderError::invalid(self.only))
    }

    fn chance(&self) -> &dyn Chance {
        &Steady
    }
}

/// `${env.name}` from `environment`: refused for a name `.env.schema` declares, which is read
/// as `${secret.name}` so it is masked.
pub fn env_value(program: &Program, environment: &dyn Environment, name: &str) -> Result<Value, RenderError> {
    if program.config.secrets.declares(name) {
        let reason = format!("`env.{name}` is a declared secret; use `${{secret.{name}}}` so it is masked");
        return Err(RenderError::invalid(reason));
    }
    let value = environment.var(name);
    value.map(Value::String).ok_or_else(|| RenderError::missing(format!("`env.{name}` is not set")))
}

/// `${secret.name}` from the `secrets` loaded.
pub fn secret_value(secrets: &Secrets, name: &str) -> Result<Value, RenderError> {
    let missing = || RenderError::missing(format!("`secret.{name}` is not loaded"));
    secrets.get(name).map(|secret| Value::String(secret.expose().to_owned())).ok_or_else(missing)
}

/// The random source of templates that have no random values.
pub struct Steady;

impl Chance for Steady {
    fn next_u64(&self) -> u64 {
        0
    }
}
