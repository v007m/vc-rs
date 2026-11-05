//! Builds the immutable [`Program`] from a linked project with no errors: one table per kind,
//! each action merged with what it inherits (D123, D124).

mod action;
mod auth;
mod expect;
mod http_retry;
mod items;
mod read;
mod rows;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;

use indexmap::IndexMap;
use vc_model::{
    ActionId, ApiId, AuthId, AuthRef, Expect, FileKind, HookModules, HttpRetry, ModelError, Origin, Owner, ProfileId, Program,
    ProgramBuilder, SourceFiles, Sym, TemplateValue, VarDefs, VarName,
};
use vc_parser::Node;

use super::data::Data;
use super::project;
use super::project::{Project, Source};
use action::Place;
pub use read::{flag, method};

/// What a build step gives; it fails only when a table is full.
type Built<T> = Result<T, ModelError>;

/// The program, or `None` when a table overflowed.
pub fn program(
    (project, data): (&Project<'_>, &Data<'_>),
    (files, hooks): (SourceFiles, HookModules),
    secrets: &[String],
) -> Option<Arc<Program>> {
    let mut builder = ProgramBuilder::new(project.strings.clone(), files);
    builder.hooks(hooks);
    let (api_expects, http_retry) = (HashMap::new(), HttpRetry::default());
    let mut build = Build { project, data, builder, api_expects, http_retry, secrets };
    build.all().ok()?;
    Some(build.builder.freeze())
}

struct Build<'p, 'a> {
    project: &'p Project<'a>,
    data: &'p Data<'a>,
    builder: ProgramBuilder,
    /// Every api's own checks, by full ID, read before any call merges them.
    api_expects: HashMap<ApiId, Expect>,
    /// The config's `http.retry`, which an api's keys replace (D143).
    http_retry: HttpRetry,
    /// The secret names `.env.schema` declares (D165).
    secrets: &'p [String],
}

impl<'a> Build<'_, 'a> {
    fn all(&mut self) -> Built<()> {
        let project = self.project;
        let of = |kind: FileKind| project.sources.iter().filter(move |source| source.kind == kind);
        for source in of(FileKind::Api) {
            self.api_checks(source)?;
        }
        for source in of(FileKind::Config) {
            self.config(source)?;
            self.auth_defs(source)?;
        }
        for source in of(FileKind::Api) {
            self.api_file(source)?;
        }
        for source in of(FileKind::Test) {
            self.test_file(source)?;
        }
        self.datasets()
    }

    /// Reads every api's own `expect` under its full ID.
    fn api_checks(&mut self, source: &Source<'a>) -> Built<()> {
        for node in project::list(source.root, "apis") {
            let Some(id) = project::text(node, "id") else { continue };
            let id = ApiId::new(self.sym(&source.full(id))?);
            self.api_expects.insert(id, expect::read(source, node.get("expect"), Origin::Api(id)));
        }
        Ok(())
    }

    fn sym(&mut self, text: &str) -> Built<Sym> {
        self.builder.intern(text)
    }

    /// `vars`, and the file's per-profile overrides under `profiles`.
    fn vars(&mut self, source: &Source<'a>, node: &Node) -> Built<VarDefs> {
        let values = self.values(source, node.get("vars"))?;
        let mut by_profile = IndexMap::new();
        for profile in project::list(node, "profiles") {
            let Some(id) = project::text(profile, "id") else { continue };
            by_profile.insert(ProfileId::new(self.sym(id)?), self.values(source, profile.get("vars"))?);
        }
        Ok(VarDefs { values, by_profile })
    }

    /// The values of a `vars` or `save` map, by name.
    fn values(&mut self, source: &Source<'a>, node: Option<&Node>) -> Built<IndexMap<VarName, TemplateValue>> {
        let mut values = IndexMap::new();
        for entry in node.map(Node::entries).unwrap_or_default() {
            let Some(value) = read::value(source, &entry.value) else { continue };
            values.insert(VarName::new(self.sym(&entry.key)?), value);
        }
        Ok(values)
    }

    /// `auth`: an ID, `none`, or an ID with vars.
    fn auth(&mut self, source: &Source<'a>, node: &Node) -> Built<Option<AuthRef>> {
        let Some(auth) = node.get("auth") else { return Ok(None) };
        let Some(id) = auth.as_scalar().map(|id| id.text.as_str()).or_else(|| project::text(auth, "id")) else {
            return Ok(None);
        };
        if id == "none" {
            return Ok(Some(AuthRef::None));
        }
        let vars = VarDefs { values: self.values(source, auth.get("vars"))?, by_profile: IndexMap::new() };
        Ok(Some(AuthRef::Use { id: AuthId::new(self.sym(id)?), vars }))
    }

    /// A lifecycle block's actions, in order.
    fn block(&mut self, source: &Source<'a>, (nodes, owner): (&[Node], Owner)) -> Built<Vec<ActionId>> {
        let place = Place { owner, defaults: None, test: None };
        let mut ids = Vec::with_capacity(nodes.len());
        for node in nodes {
            let Some(action) = self.action(source, node, &place)? else { continue };
            ids.push(self.builder.action(action)?);
        }
        Ok(ids)
    }
}
