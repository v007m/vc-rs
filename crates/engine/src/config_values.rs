//! The web UI's effective settings (M9b): `vc config explain` for many keys at once, each layer
//! a value of its own. Every value leaves masked as a run's events are: a declared secret's
//! value wherever it appears, a var named like a declared secret or a `redact.fields` entry
//! whole, then the config's `redact.rules`. A value that reads `${secret.*}` is masked whole
//! too. When the secrets cannot be loaded, every value but the built-in ones is masked, and a
//! notice says so (D229, D232).

use serde_json::{Value, json};
use vc_executor::{MASKED, MaskRules, Redactor, RuleHooks};
use vc_interfaces::{ConfigEntry, Secrets};
use vc_model::{Namespace, Part, Template, TemplateValue};

use crate::config_explain::{BUILT_IN, Layer, every_key, layers_of, unknown};
use crate::explain::{Explainer, VARS};
use crate::plan::{self, profile_flag};
use crate::ports::Layered;
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
    /// Notices from the compile and from loading the secrets.
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
        let mut layered = self.layered(&request.sets, &flags)?;
        let (active, names) = plan::profiles(layered.program.as_ref(), layered.settings.profiles.as_ref())?;
        let explainer =
            Explainer { program: layered.program.as_ref(), active: &active, names: &names, entries: &layered.entries };
        let found = found(&explainer, keys)?;
        let masker = self.masker(&mut layered).await;
        let mut values = Vec::with_capacity(found.len());
        for (key, layers) in found {
            values.push(masker.value(key, layers).await);
        }
        Ok(ConfigValues { values, notices: layered.notices })
    }

    /// Checks the layers before a long-lived server starts, so a bad `--set` or a profile no
    /// project has, from `--profile`, `VIB_PROFILE` or the user config, is refused up front. A
    /// project with errors passes: the server reports them.
    ///
    /// # Errors
    ///
    /// [`EngineError::Setting`] for a bad `--set` or an unknown profile.
    pub fn check_layers(&self, request: &ExplainRequest) -> Result<(), EngineError> {
        let flags: Vec<ConfigEntry> = profile_flag(&request.profiles).into_iter().collect();
        let layered = match self.layered(&request.sets, &flags) {
            Ok(layered) => layered,
            Err(EngineError::Invalid(_)) => return Ok(()),
            Err(error) => return Err(error),
        };
        let named = layered.settings.profiles.as_ref().filter(|named| !named.names.is_empty());
        if named.is_some() {
            plan::profiles(layered.program.as_ref(), named)?;
        }
        Ok(())
    }

    /// `text` masked as a run's values are; all of it when the project cannot be layered or its
    /// secrets cannot be loaded.
    pub(crate) async fn masked_text(&self, text: &str) -> String {
        let Ok(mut layered) = self.layered(&[], &[]) else { return MASKED.to_owned() };
        self.masker(&mut layered).await.text(text).await
    }

    /// The redactor a run would mask with, told the declared secrets' values and names (and its
    /// `function:` rules given them); when they cannot be loaded, one that masks every value
    /// but the built-in ones.
    pub(crate) async fn masker(&self, layered: &mut Layered) -> Masker {
        let program = &layered.program;
        let loaded = self.loaded_secrets(&program.config.secrets, &layered.environment).await;
        let secrets = loaded.as_ref().map_or_else(|_| Secrets::new(), Clone::clone);
        let hooks = RuleHooks::new(self.hook_runner(program), secrets);
        let rules = MaskRules::standard().with_hooks(hooks, program).for_config(&program.config);
        let redactor = Redactor::for_config(&program.config).with_rules(rules);
        redactor.hide_fields(&program.config.secrets.declared);
        match loaded {
            Ok(secrets) => {
                redactor.hide_values(secrets.iter().map(|(_, secret)| secret.expose()));
                Masker { redactor, blind: false }
            }
            Err(error) => {
                layered.notices.push(format!("{error}; every value but the built-in ones is masked"));
                Masker { redactor, blind: true }
            }
        }
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

/// `value` as shown, or [`MASKED`] whole when it reads `${secret.*}`.
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
pub struct Masker {
    redactor: Redactor,
    /// The secrets could not be loaded: mask every value but the built-in ones.
    blind: bool,
}

impl Masker {
    /// `key` and its `layers`, lowest first, as the value in force first, each masked.
    pub async fn value(&self, key: String, layers: Vec<Layer>) -> ConfigValue {
        let field = key.strip_prefix(VARS).unwrap_or(&key).to_owned();
        let mut shown = Vec::with_capacity(layers.len());
        for (value, source) in layers.into_iter().rev() {
            let value = self.masked(&field, value, &source).await;
            shown.push(ConfigLayer { value, source });
        }
        ConfigValue { key, layers: shown }
    }

    /// `text` masked; all of it when the secrets could not be loaded.
    pub async fn text(&self, text: &str) -> String {
        if self.blind { MASKED.to_owned() } else { self.redactor.masked_text(text).await }
    }

    /// `value` as the field `field` set by `source`, masked.
    pub async fn masked(&self, field: &str, value: String, source: &str) -> String {
        if self.blind && source != BUILT_IN {
            return MASKED.to_owned();
        }
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
