//! The web UI's effective settings (M9b): `vc config explain` for many keys at once, each layer
//! a value of its own. Values are masked by where they come from, never by loading a secret
//! (D232): a value that reads `${secret.*}` and a var named like a declared secret or a
//! `redact.fields` entry are masked whole, then the config's `redact.rules` apply. No secret
//! source is asked, so an unavailable one changes nothing.

use serde_json::{Value, json};
use vc_executor::{MASKED, MaskRules, Redactor};
use vc_interfaces::ConfigEntry;
use vc_model::{Namespace, Part, Program, Template, TemplateValue};

use crate::config_explain::{Layer, every_key, layers_of, unknown};
use crate::explain::{Explainer, VARS};
use crate::plan::{self, profile_flag};
use crate::shown::Shown;
use crate::{Engine, EngineError, ExplainRequest};

/// One value a key was set to, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConfigLayer {
    /// The value, masked.
    pub value: String,
    /// The layer that set it, as `vc config explain` names it: `built-in`,
    /// `vibranium.config.yaml`, `profile staging`, `.env`, `VIB_X`, `--set`.
    pub source: String,
}

/// A setting or `vars.<name>`, with the layers that set it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConfigValue {
    /// `defaults.timeout`, `vars.baseUrl`.
    pub key: String,
    /// The value in force first, then each it replaced, latest first.
    pub layers: Vec<ConfigLayer>,
}

/// What [`Engine::config_values`] found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConfigValues {
    /// One per key, in the order asked, else every setting then every var.
    pub values: Vec<ConfigValue>,
    /// Notices from the compile.
    pub notices: Vec<String>,
}

impl Engine {
    /// The layers of each of `keys` under `request`; with no keys, every setting `vc config
    /// explain` knows and every var a layer sets, leaving out those nothing sets.
    ///
    /// # Errors
    ///
    /// [`EngineError::Invalid`] when the project has errors, [`EngineError::Setting`] for a bad
    /// setting or an unknown profile, and [`EngineError::Selection`] for a key that is no
    /// setting or a var nothing sets, with the closest keys as help.
    pub async fn config_values(&self, keys: &[String], request: &ExplainRequest) -> Result<ConfigValues, EngineError> {
        let flags: Vec<ConfigEntry> = profile_flag(&request.profiles).into_iter().collect();
        let layered = self.layered(&request.sets, &flags)?;
        let (active, names) = plan::profiles(layered.program.as_ref(), layered.settings.profiles.as_ref())?;
        let explainer =
            Explainer { program: layered.program.as_ref(), active: &active, names: &names, entries: &layered.entries };
        let found = found(&explainer, keys)?;
        let masker = self.masker(&layered.program);
        let mut values = Vec::with_capacity(found.len());
        for (key, layers) in found {
            values.push(masker.value(key, layers).await);
        }
        Ok(ConfigValues { values, notices: layered.notices })
    }

    /// The redactor a run would mask with, told the declared secrets' names but never their
    /// values: explain loads no secret (D232).
    fn masker(&self, program: &Program) -> Masker {
        let rules = MaskRules::standard().with_hooks(self.hook_runner(program), program).for_config(&program.config);
        let redactor = Redactor::for_config(&program.config).with_rules(rules);
        redactor.hide_fields(&program.config.secrets.declared);
        Masker { redactor }
    }
}

/// A key with its layers, lowest first.
type Keyed = (String, Vec<Layer>);

/// Each key with its layers, lowest first.
fn found(explainer: &Explainer<'_>, keys: &[String]) -> Result<Vec<Keyed>, EngineError> {
    if keys.is_empty() {
        let every = every_key(explainer).into_iter().filter_map(|key| Some((layers_of(explainer, &key)?, key)));
        return Ok(every.filter(|(layers, _)| !layers.is_empty()).map(|(layers, key)| (key, layers)).collect());
    }
    keys.iter()
        .map(|key| {
            let layers = layers_of(explainer, key).filter(|layers| !layers.is_empty());
            layers.map(|layers| (key.clone(), layers)).ok_or_else(|| unknown(explainer, key))
        })
        .collect()
}

/// `value` as shown, or [`MASKED`] whole when it reads `${secret.*}` (D232).
pub fn shown_unless_secret(shown: &Shown<'_>, value: &TemplateValue) -> String {
    if reads_secret(value) { MASKED.to_owned() } else { shown.value(value) }
}

/// Whether `value` reads a secret anywhere in it.
fn reads_secret(value: &TemplateValue) -> bool {
    match value {
        TemplateValue::String(template) => template_reads_secret(template),
        TemplateValue::List(items) => items.iter().any(reads_secret),
        TemplateValue::Map(map) => map.iter().any(|(_, item)| reads_secret(item)),
        TemplateValue::Extend { base, changes, .. } => template_reads_secret(base) || reads_secret(changes),
        TemplateValue::Null | TemplateValue::Bool(_) | TemplateValue::Number(_) | TemplateValue::Cel(_) => false,
    }
}

/// Whether `template` has a `${secret.*}` part.
fn template_reads_secret(template: &Template) -> bool {
    template.parts.iter().any(|part| matches!(part, Part::Ref(reference) if reference.ns == Namespace::Secret))
}

/// How the values are masked.
struct Masker {
    redactor: Redactor,
}

impl Masker {
    /// `key` and its `layers`, lowest first, as the value in force first, each masked.
    async fn value(&self, key: String, layers: Vec<Layer>) -> ConfigValue {
        let field = key.strip_prefix(VARS).unwrap_or(&key).to_owned();
        let mut shown = Vec::with_capacity(layers.len());
        for (value, source) in layers.into_iter().rev() {
            let value = self.masked(&field, value).await;
            shown.push(ConfigLayer { value, source });
        }
        ConfigValue { key, layers: shown }
    }

    /// `value` as the field `field`, masked.
    async fn masked(&self, field: &str, value: String) -> String {
        let masked = self.redactor.masked(json!({ field: value })).await;
        match masked.get(field) {
            Some(Value::String(text)) => text.clone(),
            _ => MASKED.to_owned(),
        }
    }
}

#[cfg(test)]
#[path = "config_values_tests.rs"]
mod tests;
