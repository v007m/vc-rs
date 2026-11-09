//! A plugin's `vibranium-plugin.yaml`: its ID, version and kind, the program that starts it,
//! and what it may reach.
//!
//! ```yaml
//! id: htmlReport
//! version: 1.2.0
//! kind: reporter
//! entry: {command: python3, args: [report.py]}
//! permissions: {network: [], secrets: [reportToken], files: [.vibranium/reports]}
//! ```

use vc_interfaces::{PluginKind, PluginManifest, PluginPermissions};
use vc_model::SourceFileId;
use vc_parser::{Node, Syntax, Value};

/// `text` read as a manifest.
///
/// # Errors
///
/// What is wrong with it, for a refusal: not YAML, a field missing, an ID that is not
/// camelCase or a kind no host runs.
pub fn manifest(text: &str) -> Result<PluginManifest, String> {
    let root = yaml(text)?;
    let id = required(&root, "id")?;
    if !vc_compiler::is_camel_id(&id) {
        return Err(format!("its id `{id}` is not a camelCase word, such as `{}`", vc_compiler::camel_id(&id)));
    }
    let version = required(&root, "version")?;
    let kind = required(&root, "kind")?;
    let kind = PluginKind::named(&kind).ok_or_else(|| {
        let kinds = PluginKind::ALL.map(PluginKind::name).join(", ");
        format!("its kind `{kind}` is none of {kinds}")
    })?;
    let entry = root.get("entry").ok_or_else(|| "it has no `entry`".to_owned())?;
    let command = required(entry, "command").map_err(|_| "it has no `entry.command`".to_owned())?;
    let args = texts(Some(entry), "args")?;
    let permissions = permissions(root.get("permissions"))?;
    Ok(PluginManifest { id, version, kind, command, args, permissions })
}

/// `text` parsed as YAML.
///
/// # Errors
///
/// When it is not YAML.
pub fn yaml(text: &str) -> Result<Node, String> {
    let (root, diagnostics) = vc_parser::parse(SourceFileId::new(0), Syntax::Yaml, text);
    root.filter(|_| !diagnostics.has_errors()).ok_or_else(|| "it is not valid YAML".to_owned())
}

/// The scalar under `key` as text: `version: 1` reads as `1`.
///
/// # Errors
///
/// When it is missing, empty or not a scalar.
pub fn required(node: &Node, key: &str) -> Result<String, String> {
    let text = node.get(key).and_then(Node::as_scalar).map(|scalar| scalar.text.trim().to_owned());
    text.filter(|text| !text.is_empty()).ok_or_else(|| format!("it has no `{key}`"))
}

/// The `network`, `secrets` and `files` lists under `node`; each is empty when left out.
///
/// # Errors
///
/// When one is not a list of text.
pub fn permissions(node: Option<&Node>) -> Result<PluginPermissions, String> {
    Ok(PluginPermissions {
        network: texts(node, "network")?,
        secrets: texts(node, "secrets")?,
        files: texts(node, "files")?,
    })
}

/// The list of scalars under `key`; empty when it is left out.
fn texts(node: Option<&Node>, key: &str) -> Result<Vec<String>, String> {
    let Some(list) = node.and_then(|node| node.get(key)) else { return Ok(Vec::new()) };
    if !matches!(list.value, Value::Seq(_)) {
        return Err(format!("`{key}` is {}, not a list", list.kind_name()));
    }
    let text = |item: &Node| item.as_scalar().map(|scalar| scalar.text.clone());
    list.items().iter().map(|item| text(item).ok_or_else(|| format!("`{key}` holds {}", item.kind_name()))).collect()
}

#[cfg(test)]
mod tests {
    use vc_interfaces::{PluginKind, PluginPermissions};

    use super::manifest;

    #[test]
    fn reads_a_manifest_with_its_permissions() {
        let text = concat!(
            "id: htmlReport\nversion: 1\nkind: reporter\n",
            "entry: {command: python3, args: [report.py, --quiet]}\n",
            "permissions:\n  secrets: [reportToken]\n  files: [.vibranium/reports]\n",
        );
        let read = manifest(text).unwrap();
        assert_eq!((read.id.as_str(), read.version.as_str(), read.kind), ("htmlReport", "1", PluginKind::Reporter));
        assert_eq!(read.command, "python3");
        assert_eq!(read.args, ["report.py", "--quiet"]);
        let expected = PluginPermissions {
            network: Vec::new(),
            secrets: vec!["reportToken".to_owned()],
            files: vec![".vibranium/reports".to_owned()],
        };
        assert_eq!(read.permissions, expected);
    }

    #[test]
    fn says_what_is_wrong() {
        let base = "version: 1\nkind: reporter\nentry: {command: x}\n";
        let wrong = |text: &str| manifest(text).unwrap_err();
        assert_eq!(wrong(base), "it has no `id`");
        assert_eq!(
            wrong(&format!("id: html-report\n{base}")),
            "its id `html-report` is not a camelCase word, such as `htmlReport`"
        );
        assert_eq!(
            wrong("id: a\nversion: 1\nkind: pager\n"),
            "its kind `pager` is none of reporter, auth, importer, executor"
        );
        assert_eq!(wrong("id: a\nversion: 1\nkind: auth\nentry: {}\n"), "it has no `entry.command`");
        assert_eq!(
            wrong(&format!("id: a\n{base}permissions: {{secrets: apiKey}}\n")),
            "`secrets` is a value, not a list"
        );
        assert_eq!(wrong("id: [\n"), "it is not valid YAML");
    }
}
