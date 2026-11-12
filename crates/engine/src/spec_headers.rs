//! The headers an api file's `source.headers` sends when its spec is fetched (D313): rendered
//! under the active profiles from `${var.…}` (the config's, the profiles' and the file's own),
//! `${env.…}` and `${secret.…}`, and sent only to the origin of its `source.openapi`, so a
//! `$ref` to another host never carries them. They render from the config and the synced api
//! files alone; secrets are loaded only when a header reads one, and the secrets read are
//! masked in any error a fetch gives. The file keeps the templates as written. `vc import
//! openapi <url> --header` renders its headers the same way, from the config's vars, and
//! refuses a credential given as a literal value (D314, V4009).

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use http::{HeaderMap, HeaderName, HeaderValue, Uri};
use indexmap::IndexMap;
use serde_json::Value;
use vc_diagnostics::Code;
use vc_executor::Redactor;
use vc_interfaces::{ConfigEntry, Environment, Secret, Secrets};
use vc_model::{
    ApiGroup, ApiSource, FileKind, Namespace, Pos, Program, Reference, ScalarSource, ScalarStyle, Segment,
    SourceFileId, SourceSpan, StringTable, Template, credential,
};
use vc_template::{PipeRegistry, RenderError, Renderer};

use crate::authoring::{Authoring, refused};
use crate::explain::Explainer;
use crate::plan::{self, profile_flag};
use crate::ports::{Layered, SecretSource};
use crate::project::{Compiled, Valid, invalid};
use crate::restricted::{Restricted, env_value, secret_value};
use crate::shown::Var;
use crate::{Engine, EngineError, ImportRequest};

/// How many `${var.x}` deep a header's vars are followed.
const DEPTH: usize = 8;

const ONLY: &str = "`source.headers` read only `${var.…}`, `${env.…}` and `${secret.…}`";
const HELP: &str = "set what it reads under the active profiles (`--profile`), or in `.env` for a `${secret.…}`";
const TEMPLATE_HELP: &str = "write `${secret.NAME}`, `${var.name}` or `${env.NAME}`; write `$${` for a literal `${`";

/// The ports a URL's port is left out of an origin for.
const DEFAULT_PORTS: [(&str, u16); 2] = [("https", 443), ("http", 80)];

/// The headers one spec is fetched with, the one origin they go to, and what masks the secrets
/// they read.
#[derive(Clone, Debug, Default)]
pub struct SpecHeaders {
    /// [`origin`] of `source.openapi`.
    origin: String,
    headers: HeaderMap,
    redactor: Option<Arc<Redactor>>,
}

impl SpecHeaders {
    /// The headers to send to `uri`: none unless it is at their origin.
    pub fn for_uri(&self, uri: &Uri) -> Option<&HeaderMap> {
        let at = origin(uri).is_some_and(|origin| origin == self.origin);
        (at && !self.headers.is_empty()).then_some(&self.headers)
    }

    /// `text` with each secret the headers read masked.
    pub fn masked(&self, text: &str) -> String {
        masked(self.redactor.as_deref(), text)
    }
}

/// `uri`'s `scheme://host[:port]`: the scheme and host in lower case, without userinfo, and the
/// port only when it is not the scheme's default; none for a URI without a scheme and host.
pub fn origin(uri: &Uri) -> Option<String> {
    let scheme = uri.scheme_str()?.to_ascii_lowercase();
    let host = uri.host()?.to_ascii_lowercase();
    let default = DEFAULT_PORTS.iter().find(|(named, _)| *named == scheme).map(|&(_, port)| port);
    let port = uri.port_u16().filter(|port| Some(*port) != default);
    Some(port.map_or_else(|| format!("{scheme}://{host}"), |port| format!("{scheme}://{host}:{port}")))
}

fn masked(redactor: Option<&Redactor>, text: &str) -> String {
    redactor.map_or_else(|| text.to_owned(), |redactor| redactor.text(text))
}

impl Engine {
    /// The rendered `source.headers` of each api file in `file_ids` that has some and syncs
    /// from a URL, by file ID, under `profiles` (the default ones when empty).
    ///
    /// # Errors
    ///
    /// [`EngineError::Invalid`] when the config or a synced api file with headers has errors,
    /// the errors of reading the settings and the secrets the headers read, and
    /// [`EngineError::Edit`] for a header that does not render or is not a valid header.
    pub(crate) async fn spec_headers(
        &self,
        profiles: &[String],
        file_ids: &[String],
    ) -> Result<HashMap<String, SpecHeaders>, EngineError> {
        let Some(valid) = self.header_program(file_ids)? else { return Ok(HashMap::new()) };
        if profiles.is_empty() && sourced(&valid.0, file_ids).next().is_none() {
            return Ok(HashMap::new());
        }
        Ok(self.rendered((valid, profiles), &Wanted::Files(file_ids)).await?.into_iter().collect())
    }

    /// The `--header`s of `vc import openapi <url>`, rendered under its profiles from the
    /// config's vars, the environment and the secrets they read, for the spec's origin (D314).
    ///
    /// # Errors
    ///
    /// [`EngineError::Edit`] for a header given with a spec not fetched from a URL, given twice
    /// or not a template, a credential given as a literal value (V4009) and a header that does
    /// not render; [`EngineError::Invalid`] when the config has errors.
    pub(crate) async fn given_headers(&self, request: &ImportRequest) -> Result<SpecHeaders, EngineError> {
        if request.headers.is_empty() {
            return Ok(SpecHeaders::default());
        }
        let at = request.text.is_none().then(|| request.source.parse::<Uri>().ok()).flatten();
        let origin = at.as_ref().and_then(origin).ok_or_else(|| {
            refused("`--header` is sent only with a spec fetched from a URL".to_owned(), "give the spec's URL")
        })?;
        let headers = given(&request.headers)?;
        let Compiled { program, diagnostics, files, notices } = self.compiled_apis(&[])?;
        let Some(program) = program.filter(|_| !diagnostics.has_errors()) else {
            return Err(invalid(diagnostics, files, notices));
        };
        let wanted = Wanted::Given { origin, headers: &headers };
        let rendered = self.rendered(((program, notices), &request.profiles), &wanted).await?;
        Ok(rendered.into_iter().next().map(|(_, headers)| headers).unwrap_or_default())
    }

    /// The headers `wanted` names, rendered from `valid` under `profiles` (the default ones when
    /// empty), with only the secrets they read loaded, by file ID.
    async fn rendered(
        &self,
        (valid, profiles): (Valid, &[String]),
        wanted: &Wanted<'_>,
    ) -> Result<Vec<(String, SpecHeaders)>, EngineError> {
        let flags: Vec<ConfigEntry> = profile_flag(profiles).into_iter().collect();
        let layered = self.layered_over(valid, (&[], &flags))?;
        let program = layered.program.as_ref();
        let (active, names) = plan::profiles(program, layered.settings.profiles.as_ref())?;
        let explainer = Explainer { program, active: &active, names: &names, entries: &layered.entries };
        let scoped = scoped(&explainer, wanted);
        let read = secrets_read(&scoped);
        let secrets = if read.is_empty() {
            Secrets::new()
        } else {
            self.loaded_secrets(&SecretSource::layered(&layered, &names)).await?
        };
        let redactor = hiding(&secrets, &read);
        let rendering = Rendering { layered: &layered, secrets: &secrets, redactor, pipes: PipeRegistry::builtin() };
        scoped.iter().map(|scope| Ok((scope.file_id.to_owned(), rendering.headers(scope)?))).collect()
    }

    /// The program the headers of `file_ids` render from; none when it has errors but none of
    /// those files has headers, so the sync goes on as it would without them.
    fn header_program(&self, file_ids: &[String]) -> Result<Option<Valid>, EngineError> {
        let compiled = self.compiled_apis(file_ids)?;
        if let Some(program) = compiled.program.clone().filter(|_| !compiled.diagnostics.has_errors()) {
            return Ok(Some((program, compiled.notices)));
        }
        let project = Authoring { diagnostics: compiled.diagnostics, files: compiled.files };
        if file_ids.iter().any(|file_id| has_headers(&project, file_id)) {
            return Err(invalid(project.diagnostics, project.files, compiled.notices));
        }
        Ok(None)
    }
}

/// Whether the api file `file_id` writes a `source.headers` with any header in it.
fn has_headers(project: &Authoring, file_id: &str) -> bool {
    let opened = project.find(FileKind::Api, file_id).and_then(|found| Authoring::open(found).ok());
    let headers = |opened: &crate::authoring::Opened<'_>| {
        let source = opened.document.root().get("source");
        source.and_then(|source| source.get("headers")).is_some_and(|headers| !headers.entries().is_empty())
    };
    opened.is_some_and(|opened| headers(&opened))
}

/// Each api group in `file_ids` with headers and a URL to send them to, its file ID and origin.
fn sourced<'p>(
    program: &'p Program,
    file_ids: &'p [String],
) -> impl Iterator<Item = (&'p str, String, &'p ApiSource, &'p ApiGroup)> + 'p {
    program.api_groups.values().filter_map(move |group| {
        let file_id = program.str(group.id.sym()).filter(|id| file_ids.iter().any(|wanted| wanted == id))?;
        let source = group.source.as_ref().filter(|source| !source.headers.is_empty())?;
        let origin = source.openapi.parse::<Uri>().ok().as_ref().and_then(origin)?;
        Some((file_id, origin, source, group))
    })
}

/// The headers to render: the synced api files' `source.headers`, or `vc import`'s `--header`s.
enum Wanted<'w> {
    Files(&'w [String]),
    Given { origin: String, headers: &'w IndexMap<String, Template> },
}

/// One set of headers, what a refusal calls them, the origin they go to and the vars in force.
struct Scoped<'p> {
    /// The api file's ID; empty for `--header`s.
    file_id: &'p str,
    /// What a refusal names a header as, up to its name: `` `users`'s `source.headers. ``.
    called: String,
    origin: String,
    headers: &'p IndexMap<String, Template>,
    vars: Vec<Var>,
}

/// The headers `wanted` names, each set with its vars: the config's and active profiles', then
/// an api file's own over them.
fn scoped<'p>(explainer: &Explainer<'p>, wanted: &Wanted<'p>) -> Vec<Scoped<'p>> {
    let config = explainer.config_vars();
    let file_ids = match wanted {
        Wanted::Files(file_ids) => *file_ids,
        Wanted::Given { origin, headers } => {
            let called = "`--header ".to_owned();
            return vec![Scoped { file_id: "", called, origin: origin.clone(), headers, vars: config }];
        }
    };
    let files = sourced(explainer.program, file_ids).map(|(file_id, origin, source, group)| {
        let mut vars = config.clone();
        explainer.level(&mut vars, &group.vars, file_id);
        let called = format!("`{file_id}`'s `source.headers.");
        Scoped { file_id, called, origin, headers: &source.headers, vars }
    });
    files.collect()
}

/// The `--header` values as templates. A credential given as a literal value is refused
/// (V4009), naming the header only: `vc import` would write it into the api file.
fn given(headers: &[(String, String)]) -> Result<IndexMap<String, Template>, EngineError> {
    let mut strings = StringTable::new();
    let at = Pos::new(1, 1, 0);
    let source = ScalarSource { span: SourceSpan::new(SourceFileId::new(0), at, at), style: ScalarStyle::Plain };
    let mut given = IndexMap::new();
    for (name, text) in headers {
        let template = vc_template::parse(text, source, &mut strings)
            .map_err(|_| refused(format!("`--header {name}` is not a valid template"), TEMPLATE_HELP))?;
        if credential(name) && template.as_literal().is_some_and(|text| !text.trim().is_empty()) {
            let reason = format!("{}: `--header {name}` is a credential given as a literal value", Code::V4009);
            let help = format!("pass `--header '{name}: Bearer ${{secret.NAME}}'`, with `NAME=` in `.env.schema`");
            return Err(refused(reason, &help));
        }
        if given.keys().any(|named: &String| named.eq_ignore_ascii_case(name)) {
            return Err(refused(format!("`--header {name}` is given twice"), "give each header once"));
        }
        given.insert(name.clone(), template);
    }
    Ok(given)
}

/// The var `name` in force in `vars`; a var a run saves is known only then.
fn var<'v>(vars: &'v [Var], name: &str) -> Option<&'v Var> {
    vars.iter().find(|var| var.name == name && !var.saved)
}

/// The secrets the headers of `scoped` read, through the vars they read.
fn secrets_read(scoped: &[Scoped<'_>]) -> BTreeSet<String> {
    let mut read = BTreeSet::new();
    for scope in scoped {
        let mut pending: Vec<&Reference> = scope.headers.values().flat_map(Template::references).collect();
        let mut seen = BTreeSet::new();
        while let Some(reference) = pending.pop() {
            let Some(Segment::Key(name)) = reference.path.segments.first() else { continue };
            if reference.ns == Namespace::Secret {
                read.insert(name.clone());
            } else if reference.ns == Namespace::Var && seen.insert(name.clone()) {
                pending.extend(var(&scope.vars, name).into_iter().flat_map(|var| var.value.references()));
            }
        }
    }
    read
}

/// What masks the secrets `read`, as loaded; none when the headers read none.
fn hiding(secrets: &Secrets, read: &BTreeSet<String>) -> Option<Arc<Redactor>> {
    if read.is_empty() {
        return None;
    }
    let redactor = Redactor::new(&[]);
    redactor.hide_values(read.iter().filter_map(|name| secrets.get(name)).map(Secret::expose));
    Some(Arc::new(redactor))
}

/// What every file's headers render with.
struct Rendering<'r> {
    layered: &'r Layered,
    secrets: &'r Secrets,
    redactor: Option<Arc<Redactor>>,
    pipes: PipeRegistry,
}

impl Rendering<'_> {
    /// `scope`'s headers, rendered.
    fn headers(&self, scope: &Scoped<'_>) -> Result<SpecHeaders, EngineError> {
        let lookup = Lookup {
            program: &self.layered.program,
            vars: &scope.vars,
            environment: self.layered.environment.as_ref(),
            secrets: self.secrets,
            pipes: &self.pipes,
            chain: Vec::new(),
        };
        let mut headers = HeaderMap::new();
        for (name, template) in scope.headers {
            let unsent = |reason: String| refused(format!("{}{name}` {reason}", scope.called), HELP);
            let text = Renderer::new(&self.pipes, &lookup.restricted()).text(template).map_err(|error| {
                unsent(format!("does not render: {}", masked(self.redactor.as_deref(), &error.to_string())))
            })?;
            let name =
                HeaderName::from_bytes(name.as_bytes()).map_err(|_| unsent("is not a header name".to_owned()))?;
            let value = HeaderValue::from_bytes(text.as_bytes())
                .map_err(|_| unsent("renders to text a header cannot hold".to_owned()))?;
            headers.append(name, value);
        }
        Ok(SpecHeaders { origin: scope.origin.clone(), headers, redactor: self.redactor.clone() })
    }
}

/// Answers a header's lookups: the vars in force for its file, the environment, the secrets.
struct Lookup<'l> {
    program: &'l Program,
    vars: &'l [Var],
    environment: &'l dyn Environment,
    secrets: &'l Secrets,
    pipes: &'l PipeRegistry,
    /// The vars being rendered, outermost first.
    chain: Vec<&'l str>,
}

impl Lookup<'_> {
    /// `${var.name}`, its own templates rendered the same way: refused when it refers back to
    /// itself or nests deeper than [`DEPTH`].
    fn var(&self, name: &str) -> Result<Value, RenderError> {
        let found = var(self.vars, name);
        let found =
            found.ok_or_else(|| RenderError::missing(format!("`var.{name}` is not set under the active profiles")))?;
        if let Some(at) = self.chain.iter().position(|named| *named == name) {
            let cycle: Vec<String> =
                self.chain.iter().skip(at).chain([&name]).map(|var| format!("`var.{var}`")).collect();
            return Err(RenderError::invalid(format!("`var.{name}` refers to itself: {}", cycle.join(" → "))));
        }
        if self.chain.len() >= DEPTH {
            return Err(RenderError::invalid(format!("`var.{name}` nests deeper than {DEPTH} levels")));
        }
        let mut chain = self.chain.clone();
        chain.push(&found.name);
        let deeper = Lookup { chain, ..*self };
        Renderer::new(self.pipes, &deeper.restricted()).value(&found.value)
    }

    /// The resolver of the namespaces a header may read.
    fn restricted(&self) -> Restricted<'_> {
        Restricted {
            reads: vec![
                (Namespace::Var, Box::new(|name: &str| self.var(name))),
                (Namespace::Env, Box::new(|name: &str| env_value(self.program, self.environment, name))),
                (Namespace::Secret, Box::new(|name: &str| secret_value(self.secrets, name))),
            ],
            only: ONLY,
        }
    }
}

#[cfg(test)]
#[path = "spec_headers_tests.rs"]
mod tests;
