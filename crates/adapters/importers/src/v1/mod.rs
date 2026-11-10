//! `vc migrate v1`: a v1 project (`config.json` and its tests directory) as the files of a v2
//! project (D200). Each v1 collection becomes an api file, each scenario a test file. What has
//! no v2 form is left out and listed: a note when the meaning is kept another way, a
//! `# TODO(vc migrate)` above the item, with the v1 text, when it is lost.

mod apis;
mod config;
mod deps;
mod expect;
mod scenario;
mod source;
mod template;
mod unset;

use std::collections::BTreeMap;

use serde_json::Value;
use vc_interfaces::{ImportError, ImportNote};

use self::apis::Ids;
use self::config::{Accounts, CONFIG_PATH};
use self::source::Source;
use self::template::Templates;

/// A v1 project as v2 files, for the engine to render, check and write.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct V1Migration {
    /// The config first, then the api and test files.
    pub files: Vec<MigratedFile>,
    /// The secret names the auth reads, for `.env.schema`.
    pub secrets: Vec<String>,
    /// What was changed on the way, such as a dataset that is now a Faker call.
    pub notes: Vec<ImportNote>,
}

/// One v2 file of a migration.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct MigratedFile {
    /// Its path in the project, such as `services/users/users.api.yaml`.
    pub path: String,
    /// What it holds.
    pub doc: Value,
    /// What has no v2 form: `at` is the ID of the item it was in, empty for the file; the
    /// message keeps the v1 text and goes in a `# TODO(vc migrate)` comment.
    pub todos: Vec<ImportNote>,
}

/// The v2 files of the v1 project in `files` (path relative to the v1 folder → text).
///
/// # Errors
///
/// [`ImportError::Invalid`] when there is no `config.json`, or a file it reads is not JSON.
pub fn migrate_v1(files: &BTreeMap<String, String>) -> Result<V1Migration, ImportError> {
    let source = Source::read(files)?;
    let env = source.config.get("env_vars").and_then(Value::as_object);
    let (accounts, ids, templates) = (Accounts::new(&source.config), Ids::new(&source), Templates::new(env));
    let mut converter = Converter { source: &source, accounts, ids, templates };
    let mut files = converter.api_files();
    files.extend(converter.test_files());
    let (config, secrets) = config::config(&source, &mut converter.templates);
    files.insert(0, MigratedFile { path: CONFIG_PATH.to_owned(), doc: config, todos: Vec::new() });
    unset::fill(&mut files);
    Ok(V1Migration { files, secrets, notes: converter.templates.notes })
}

/// What the parts of a migration share.
struct Converter<'s> {
    source: &'s Source,
    accounts: Accounts<'s>,
    ids: Ids,
    templates: Templates,
}

/// The endpoints of a v1 scenario.
fn endpoints(scenario: &Value) -> impl Iterator<Item = &Value> {
    scenario.get("endpoints").and_then(Value::as_array).into_iter().flatten()
}

/// The text at `key` of `value`, empty when there is none.
fn text<'v>(value: &'v Value, key: &str) -> &'v str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// A v1 millisecond count as a v2 duration: whole seconds as `5s`, the rest as `700ms`.
fn duration(millis: u64) -> String {
    if millis >= 1000 && millis.is_multiple_of(1000) { format!("{}s", millis / 1000) } else { format!("{millis}ms") }
}

/// A v1 collection or scenario name as a word of a file ID: lowercase, `-` between words.
fn file_word(name: &str) -> String {
    let words: Vec<String> = name
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    if words.is_empty() { "tests".to_owned() } else { words.join("-") }
}

#[cfg(test)]
mod tests;
