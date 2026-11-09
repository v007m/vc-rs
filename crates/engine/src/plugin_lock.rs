//! `vibranium.lock`: each approved plugin, where it came from, what that resolved to, its
//! integrity and the permissions approved for it. `vc plugin add` and `vc plugin remove` write
//! it; a run refuses a plugin whose manifest asks for more than it grants.

use std::fmt::Write as _;

use vc_interfaces::PluginPermissions;
use vc_parser::{Node, write_text};

use crate::plugin_manifest::{permissions, required, yaml};

/// The lock format this engine reads and writes.
const LOCK_VERSION: &str = "1";

/// The head of every lock file.
const HEADER: &str = "\
# The plugins `vc plugin add` approved, with what each may reach. Written by vc; commit it and
# review changes to it like code. A run refuses a plugin that asks for more than this grants.
";

/// One approved plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locked {
    pub id: String,
    pub source: String,
    pub resolved: String,
    pub integrity: String,
    pub permissions: PluginPermissions,
}

/// The plugins in a lock file's `text`.
///
/// # Errors
///
/// What is wrong with it: not YAML, another lock version, or a plugin missing a field.
pub fn read(text: &str) -> Result<Vec<Locked>, String> {
    let root = yaml(text)?;
    let version = required(&root, "lockVersion")?;
    if version != LOCK_VERSION {
        return Err(format!("it is lock version {version}; this vc reads version {LOCK_VERSION}"));
    }
    root.get("plugins").map(Node::items).unwrap_or_default().iter().map(locked).collect()
}

fn locked(item: &Node) -> Result<Locked, String> {
    let id = required(item, "id")?;
    let named = |reason: String| format!("plugin `{id}`: {reason}");
    Ok(Locked {
        source: required(item, "source").map_err(named)?,
        resolved: required(item, "resolved").map_err(named)?,
        integrity: required(item, "integrity").map_err(named)?,
        permissions: permissions(item.get("permissions")).map_err(named)?,
        id,
    })
}

/// `plugins` with `plugin` in place of the one with its ID, in ID order.
pub fn with(mut plugins: Vec<Locked>, plugin: Locked) -> Vec<Locked> {
    plugins.retain(|old| old.id != plugin.id);
    plugins.push(plugin);
    plugins.sort_by(|a, b| a.id.cmp(&b.id));
    plugins
}

/// The lock file for `plugins`.
pub fn render(plugins: &[Locked]) -> String {
    let mut out = format!("{HEADER}lockVersion: {LOCK_VERSION}\n");
    if plugins.is_empty() {
        out.push_str("plugins: []\n");
        return out;
    }
    out.push_str("plugins:\n");
    for plugin in plugins {
        let _ = writeln!(out, "  - id: {}", write_text(&plugin.id, false));
        for (key, value) in
            [("source", &plugin.source), ("resolved", &plugin.resolved), ("integrity", &plugin.integrity)]
        {
            let _ = writeln!(out, "    {key}: {}", write_text(value, false));
        }
        out.push_str("    permissions:\n");
        let granted = &plugin.permissions;
        for (key, values) in [("network", &granted.network), ("secrets", &granted.secrets), ("files", &granted.files)] {
            let listed = values.iter().map(|value| write_text(value, true)).collect::<Vec<_>>().join(", ");
            let _ = writeln!(out, "      {key}: [{listed}]");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use vc_interfaces::PluginPermissions;

    use super::{Locked, read, render, with};

    fn plugin(id: &str, secrets: &[&str]) -> Locked {
        Locked {
            id: id.to_owned(),
            source: format!("npm:@acme/{id}@^2"),
            resolved: format!("@acme/{id}@2.1.0"),
            integrity: "sha512-abc==".to_owned(),
            permissions: PluginPermissions {
                network: vec!["api.acme.dev".to_owned()],
                secrets: secrets.iter().map(|&secret| secret.to_owned()).collect(),
                files: Vec::new(),
            },
        }
    }

    #[test]
    fn a_lock_is_written_in_id_order_and_read_back() {
        let plugins = with(with(Vec::new(), plugin("report", &["reportToken"])), plugin("acmeAuth", &[]));
        let text = render(&plugins);
        assert!(text.starts_with("# The plugins `vc plugin add` approved"), "{text}");
        let body = concat!(
            "lockVersion: 1\nplugins:\n",
            "  - id: acmeAuth\n    source: npm:@acme/acmeAuth@^2\n    resolved: \"@acme/acmeAuth@2.1.0\"\n",
            "    integrity: sha512-abc==\n    permissions:\n",
            "      network: [api.acme.dev]\n      secrets: []\n      files: []\n",
            "  - id: report\n    source: npm:@acme/report@^2\n    resolved: \"@acme/report@2.1.0\"\n",
            "    integrity: sha512-abc==\n    permissions:\n",
            "      network: [api.acme.dev]\n      secrets: [reportToken]\n      files: []\n",
        );
        assert!(text.ends_with(body), "{text}");
        assert_eq!(read(&text).unwrap(), plugins);
        let replaced = with(plugins, plugin("report", &[]));
        assert_eq!(replaced.len(), 2);
        assert_eq!(replaced[1].permissions.secrets, Vec::<String>::new());
    }

    #[test]
    fn an_empty_lock_keeps_its_list_and_a_wrong_one_says_why() {
        assert_eq!(read(&render(&[])).unwrap(), []);
        assert!(render(&[]).ends_with("lockVersion: 1\nplugins: []\n"));
        assert_eq!(read("lockVersion: 2\nplugins: []\n").unwrap_err(), "it is lock version 2; this vc reads version 1");
        assert_eq!(read("lockVersion: 1\nplugins:\n  - id: a\n").unwrap_err(), "plugin `a`: it has no `source`");
    }
}
