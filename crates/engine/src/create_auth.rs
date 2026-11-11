//! `vc new auth`: an auth definition appended to the config's `auth` list. A credential is
//! written only as a template (`${secret.PARTNER_TOKEN}`), never as itself; one the type needs
//! and the author left out is a `${secret.…}` placeholder, declared in `.env.schema`. Any other
//! key the type needs and the author left out is theirs to add: the guard lets its V1005 pass,
//! and `vc check` reports it.

use std::ops::Range;

use vc_compiler::{camel_id, is_camel_id};
use vc_diagnostics::{Code, Diagnostic};
use vc_model::{DEFAULT_SECRETS_SCHEMA, FileKind, auth_needs, secret_name};
use vc_parser::write_text;

use crate::authoring::{Authoring, item, refused};
use crate::create::{DOCS, ListItem, commented_item};
use crate::edit::{Changes, declaring, internal};
use crate::{Added, Engine, EngineError};

/// Each auth type, with the fields `vc new auth` asks for, in order: every key the type needs
/// ([`vc_model::AUTH_NEEDS`]) but a flow's `steps`, and the optional ones worth asking for.
const FIELDS: [(&str, &[&str]); 9] = [
    ("basic", &["username", "password"]),
    ("bearer", &["token"]),
    ("apiKey", &["name", "in", "value"]),
    ("oauth2ClientCredentials", &["tokenUrl", "clientId", "clientSecret", "audience"]),
    ("oauth2Password", &["tokenUrl", "clientId", "username", "password"]),
    ("oauth2AuthorizationCode", &["authorizeUrl", "tokenUrl", "clientId"]),
    ("oauth2Refresh", &["tokenUrl", "clientId", "refreshToken"]),
    ("flow", &[]),
    ("plugin", &["plugin"]),
];

/// The fields that hold a credential, so are written only as templates.
const SECRET: [&str; 5] = ["password", "token", "value", "clientSecret", "refreshToken"];

/// What `vc new auth` asks for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::exhaustive_structs,
    reason = "built in other crates, shared across the workspace; every user is in this repo (D160)"
)]
pub struct NewAuth {
    /// The auth's ID: `partnerToken`.
    pub id: String,
    /// Its type: `bearer`.
    pub auth_type: String,
    /// The fields given, each `(name, value)`; one left out is not written.
    pub fields: Vec<(String, String)>,
}

/// The fields `vc new auth` asks for, for `auth_type`; none for an unknown type.
#[must_use]
pub fn auth_fields(auth_type: &str) -> &'static [&'static str] {
    FIELDS.iter().find(|(name, _)| *name == auth_type).map_or(&[], |(_, fields)| fields)
}

impl Engine {
    /// The change that appends an auth definition to the config.
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] for an ID that breaks the convention (with the one it suggests),
    /// one the config already has, an unknown type, a field the type does not take, a
    /// credential that is not a template, or a change the guard refuses.
    pub fn new_auth(&self, request: &NewAuth) -> Result<Added, EngineError> {
        let id = request.id.as_str();
        if !is_camel_id(id) {
            let help = format!("did you mean `{}`? Auth IDs are camelCase nouns", camel_id(id));
            return Err(refused(format!("`{id}` breaks the ID convention"), &help));
        }
        let yaml = auth_yaml(request)?;
        let project = self.authoring()?;
        let config = project.entries().find(|(_, found)| found.kind() == Some(FileKind::Config));
        let opened = Authoring::open(config.ok_or_else(|| internal("a project without its config"))?)?;
        if item(opened.document.root(), "auth", id).is_some() {
            return Err(refused(format!("the config already has the auth `{id}`"), "pick another ID"));
        }
        let comment = format!("# {id} · {} · keys: {DOCS}#authentication", request.auth_type);
        let (change, line) = commented_item(&opened, &ListItem { list: "auth", yaml: &yaml, comment: &comment })?;
        let path = change.path.clone();
        let mut changes = Changes { files: vec![change], titles: vec![format!("Add auth {id}")] };
        self.declare_placeholders(request, &mut changes)?;
        let entry = Entry { path: &path, lines: line..line + yaml.lines().count() };
        let changes =
            self.guarded_expecting(&project, (changes, &|new: &Diagnostic, at: &str| entry.leaves(new, at)))?;
        Ok(Added { changes, id: id.to_owned(), path, line })
    }

    /// Declares in `.env.schema` each placeholder secret `request` gets; nothing when the project
    /// has no schema, since then no secret name is checked.
    fn declare_placeholders(&self, request: &NewAuth, changes: &mut Changes) -> Result<(), EngineError> {
        let missing = placeholders(request, auth_fields(&request.auth_type));
        let names: Vec<&str> = missing.iter().map(|(_, name)| name.as_str()).collect();
        let Some(schema) = self.secrets_schema()? else { return Ok(()) };
        if let Some((change, added)) = declaring(&schema, &names) {
            changes.titles.push(format!("Declare {} in {DEFAULT_SECRETS_SCHEMA}", added.join(", ")));
            changes.files.push(change);
        }
        Ok(())
    }
}

/// Where the new auth entry is written: its file and lines.
struct Entry<'p> {
    path: &'p str,
    lines: Range<usize>,
}

impl Entry<'_> {
    /// Whether `new`, in the file at `at`, is a key the entry's type needs and the author left
    /// out: V1005 within the entry.
    fn leaves(&self, new: &Diagnostic, at: &str) -> bool {
        let line = usize::try_from(new.primary.span.start.line).unwrap_or(usize::MAX);
        new.code == Code::V1005 && at == self.path && self.lines.contains(&line)
    }
}

/// The auth's YAML, the `id:` line first, after checking its type and fields.
fn auth_yaml(request: &NewAuth) -> Result<String, EngineError> {
    let Some((auth_type, takes)) = FIELDS.iter().find(|(name, _)| *name == request.auth_type) else {
        let types: Vec<&str> = FIELDS.iter().map(|(name, _)| *name).collect();
        return Err(refused(
            format!("there is no auth type `{}`", request.auth_type),
            &format!("use one of {}", types.join(", ")),
        ));
    };
    let mut lines = vec![format!("id: {}", request.id), format!("type: {auth_type}")];
    for (field, value) in request.fields.iter().filter(|(_, value)| !value.trim().is_empty()) {
        if !takes.contains(&field.as_str()) {
            return Err(refused(
                format!("a {auth_type} auth has no `{field}`"),
                &format!("it takes {}", takes.join(", ")),
            ));
        }
        if SECRET.contains(&field.as_str()) && !value.contains("${") {
            let help =
                "write it as a template, such as `${secret.PARTNER_TOKEN}`, and declare the secret in `.env.schema`";
            return Err(refused(format!("`{field}` is a credential, so it is never written to YAML as itself"), help));
        }
        lines.push(format!("{field}: {}", write_text(value.trim(), false)));
    }
    let missing = placeholders(request, takes);
    lines.extend(missing.iter().map(|(field, name)| format!("{field}: ${{secret.{name}}}")));
    Ok(lines.join("\n"))
}

/// Each credential the type needs that was not given, with the secret written for it, named
/// after the auth and the field (`partnerToken`'s `token` is `PARTNER_TOKEN_TOKEN`).
fn placeholders<'t>(request: &NewAuth, takes: &[&'t str]) -> Vec<(&'t str, String)> {
    let needs = auth_needs(&request.auth_type).unwrap_or_default();
    let given = |field: &str| request.fields.iter().any(|(name, value)| name == field && !value.trim().is_empty());
    let missing = takes.iter().filter(|field| SECRET.contains(field) && needs.contains(field) && !given(field));
    missing.map(|field| (*field, secret_name(&format!("{} {field}", request.id)))).collect()
}

#[cfg(test)]
mod tests {
    use super::{FIELDS, NewAuth, auth_fields, auth_yaml};

    fn request(auth_type: &str, fields: &[(&str, &str)]) -> NewAuth {
        let fields = fields.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())).collect();
        NewAuth { id: "partnerToken".to_owned(), auth_type: auth_type.to_owned(), fields }
    }

    #[test]
    fn each_type_asks_for_its_own_fields() {
        assert_eq!(auth_fields("bearer"), ["token"]);
        assert_eq!(auth_fields("basic"), ["username", "password"]);
        assert_eq!(auth_fields("nope"), [""; 0]);
    }

    #[test]
    fn an_auth_is_written_with_the_fields_given() {
        let yaml = auth_yaml(&request("bearer", &[("token", "${secret.PARTNER_TOKEN}")])).ok();
        assert_eq!(yaml.as_deref(), Some("id: partnerToken\ntype: bearer\ntoken: ${secret.PARTNER_TOKEN}"));
        assert_eq!(
            auth_yaml(&request("bearer", &[("token", " ")])).ok().as_deref(),
            Some("id: partnerToken\ntype: bearer\ntoken: ${secret.PARTNER_TOKEN_TOKEN}")
        );
        assert_eq!(
            auth_yaml(&request("basic", &[("username", "${var.user}")])).ok().as_deref(),
            Some("id: partnerToken\ntype: basic\nusername: ${var.user}\npassword: ${secret.PARTNER_TOKEN_PASSWORD}")
        );
    }

    #[test]
    fn each_type_asks_for_every_key_it_needs_but_a_flow_s_steps() {
        let types: Vec<&str> = FIELDS.iter().map(|(name, _)| *name).collect();
        let built_in: Vec<&str> = vc_model::AUTH_NEEDS.iter().map(|(name, _)| *name).collect();
        assert_eq!(types, built_in);
        for (name, needs) in vc_model::AUTH_NEEDS {
            let asked = auth_fields(name);
            let left: Vec<&&str> = needs.iter().filter(|key| !asked.contains(key)).collect();
            assert_eq!(left, if name == "flow" { vec![&"steps"] } else { vec![] }, "{name}");
        }
    }

    #[test]
    fn refuses_a_credential_as_itself_a_foreign_field_and_an_unknown_type() {
        assert!(auth_yaml(&request("bearer", &[("token", "eyJhbGciOi")])).is_err());
        assert!(auth_yaml(&request("bearer", &[("username", "ann")])).is_err());
        assert!(auth_yaml(&request("bearr", &[])).is_err());
    }
}
