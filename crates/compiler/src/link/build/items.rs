//! The config, profiles, api files and test files, with their apis and tests.

use indexmap::IndexMap;
use vc_diagnostics::Diagnostics;
use vc_model::{
    Api, ApiGroup, ApiId, Bodies, Config, Defaults, Examples, FileId, GeneratorDef, HistoryConfig, HttpRetry, Keep,
    Lifecycle, Origin, Owner, Profile, ProfileId, RequestHooks, RunBlocks, SaveSpec, Test, TestBody, TestFile, TestId,
    VarDefs,
};
use vc_parser::Node;
use vc_template::FakeRegistry;

use super::action::Place;
use super::{Build, Built, auth, expect, http_retry, read};
use crate::link::fakes;
use crate::link::project;
use crate::link::project::Source;
use crate::shapes;

impl<'a> Build<'_, 'a> {
    /// The config, its profiles, and `beforeRun` and `afterRun`.
    pub fn config(&mut self, source: &Source<'a>) -> Built<()> {
        let root = source.root;
        let defaults = root.get("defaults");
        let reports = root.get("reports");
        let redact = root.get("redact");
        self.http_retry = http_retry::over(source, root, &HttpRetry::default()).unwrap_or_default();
        let config = Config {
            syntax_version: read::number(root, "syntaxVersion").unwrap_or_default(),
            project: read::owned(root, "project"),
            defaults: defaults.map(|defaults| Self::defaults(source, defaults)).unwrap_or_default(),
            http_retry: self.http_retry.clone(),
            vars: VarDefs { values: self.values(source, root.get("vars"))?, by_profile: IndexMap::new() },
            reports_dir: reports.and_then(|reports| read::owned(reports, "dir")),
            report_formats: reports.map(|reports| read::texts(reports, "formats")).unwrap_or_default(),
            html_single_file: reports
                .and_then(|reports| reports.get("html"))
                .is_some_and(|html| project::text(html, "layout") == Some("single")),
            redact_headers: redact.map(|redact| read::texts(redact, "headers")).unwrap_or_default(),
            redact_fields: redact.map(|redact| read::texts(redact, "fields")).unwrap_or_default(),
            history: root.get("history").map(|history| Self::history(source, history)).unwrap_or_default(),
            ..Config::default()
        };
        self.builder.config(auth::with_secrets(config, source, self.secrets));
        for profile in project::list(root, "profiles") {
            let Some(id) = project::text(profile, "id") else { continue };
            let id = ProfileId::new(self.sym(id)?);
            let vars = self.values(source, profile.get("vars"))?;
            let name = read::owned(profile, "name");
            self.builder.profile(Profile {
                id,
                name,
                default: read::flag(shapes::profile(), profile, "default"),
                vars,
            });
        }
        let before_run = self.block(source, (project::list(root, "beforeRun"), Owner::Run))?;
        let after_run = self.block(source, (project::list(root, "afterRun"), Owner::Run))?;
        self.builder.run_blocks(RunBlocks { before_run, after_run });
        self.generators(source)
    }

    /// The named `generators`, as the built-in calls they make (D174); the link checks have
    /// already reported any that do not lower.
    fn generators(&mut self, source: &Source<'a>) -> Built<()> {
        let mut rejected = Diagnostics::new();
        for named in fakes::lower(source, &FakeRegistry::builtin(), &mut rejected) {
            let id = self.sym(named.id)?;
            self.builder.generator(GeneratorDef { id, fake: named.call.name.to_owned(), args: named.call.args });
        }
        Ok(())
    }

    fn defaults(source: &Source<'_>, node: &Node) -> Defaults {
        Defaults {
            timeout: read::duration(source, node, "timeout"),
            concurrency: read::number(node, "concurrency"),
            per_host: read::number(node, "perHost"),
            headers: read::templates(source, node, "headers"),
        }
    }

    fn history(source: &Source<'_>, node: &Node) -> HistoryConfig {
        let defaults = HistoryConfig::default();
        let keep = match project::text(node, "keep") {
            Some("all") => Keep::All,
            written => written.and_then(|text| text.parse().ok()).map_or(defaults.keep, Keep::Runs),
        };
        HistoryConfig {
            bodies: project::text(node, "bodies").and_then(Bodies::parse).unwrap_or(defaults.bodies),
            max_body_size: read::size(source, node, "maxBodySize").unwrap_or(defaults.max_body_size),
            keep,
        }
    }

    /// An api file and its apis.
    pub fn api_file(&mut self, source: &Source<'a>) -> Built<()> {
        let root = source.root;
        let group = FileId::new(self.sym(source.id.as_deref().unwrap_or_default())?);
        let mut apis = Vec::new();
        for node in project::list(root, "apis") {
            let Some(api) = self.api(source, node, group)? else { continue };
            apis.push(api.id);
            self.builder.api(api);
        }
        let base_url = root.get("baseUrl").and_then(|url| read::template(source, url));
        let auth = self.auth(source, root)?.unwrap_or_default();
        let headers = read::templates(source, root, "headers");
        let (name, vars, hooks) = (read::owned(root, "name"), self.vars(source, root)?, hooks(root));
        self.builder.api_group(ApiGroup { id: group, name, base_url, auth, headers, vars, hooks, apis });
        Ok(())
    }

    fn api(&mut self, source: &Source<'a>, node: &Node, group: FileId) -> Built<Option<Api>> {
        let (Some(id), Some(path)) =
            (project::text(node, "id"), node.get("path").and_then(|path| read::template(source, path)))
        else {
            return Ok(None);
        };
        let id = ApiId::new(self.sym(&source.full(id))?);
        let cleanup = match node.get("cleanup") {
            Some(cleanup) => self.block(source, (std::slice::from_ref(cleanup), Owner::Instance))?.first().copied(),
            None => None,
        };
        let expect = self.api_expects.get(&id).cloned().unwrap_or_default();
        let save_on = expect::save_on(source, node.get("saveOn"), &expect);
        Ok(Some(Api {
            id,
            name: read::owned(node, "name"),
            group,
            method: read::method(node),
            path,
            vars: self.vars(source, node)?,
            query: read::templates(source, node, "query"),
            headers: read::templates(source, node, "headers"),
            body: node.get("body").and_then(|body| read::body(source, body)).unwrap_or_default(),
            auth: self.auth(source, node)?.unwrap_or_default(),
            timeout: read::duration(source, node, "timeout"),
            http_retry: http_retry::over(source, node, &self.http_retry),
            expect,
            save: SaveSpec { values: self.values(source, node.get("save"))? },
            save_on,
            cleanup,
            hooks: hooks(node),
            deprecated: read::flag(shapes::api(), node, "deprecated"),
            span: node.span,
        }))
    }

    /// A test file, its lifecycle blocks and its tests.
    pub fn test_file(&mut self, source: &Source<'a>) -> Built<()> {
        let root = source.root;
        let file = FileId::new(self.sym(source.id.as_deref().unwrap_or_default())?);
        let lifecycle = Lifecycle {
            before_all: self.block(source, (project::list(root, "beforeAll"), Owner::File(file)))?,
            after_all: self.block(source, (project::list(root, "afterAll"), Owner::File(file)))?,
            before_each: self.block(source, (project::list(root, "beforeEach"), Owner::Instance))?,
            after_each: self.block(source, (project::list(root, "afterEach"), Owner::Instance))?,
        };
        let mut tests = Vec::new();
        for node in project::list(root, "tests") {
            let Some(test) = self.test(source, node, file)? else { continue };
            tests.push(test.id);
            self.builder.test(test);
        }
        let (name, tags, vars, skip) =
            (read::owned(root, "name"), read::tags(root), self.vars(source, root)?, read::skip(root));
        self.builder.test_file(TestFile { id: file, name, tags, vars, skip, lifecycle, tests });
        Ok(())
    }

    fn test(&mut self, source: &Source<'a>, node: &Node, file: FileId) -> Built<Option<Test>> {
        let Some(id) = project::text(node, "id") else { return Ok(None) };
        let id = TestId::new(self.sym(&source.full(id))?);
        let before = self.block(source, (project::list(node, "before"), Owner::Instance))?;
        let Some(body) = self.body(source, node, id)? else { return Ok(None) };
        let after = self.block(source, (project::list(node, "after"), Owner::Instance))?;
        Ok(Some(Test {
            id,
            file,
            name: read::owned(node, "name"),
            tags: read::tags(node),
            before,
            after,
            body,
            vars: self.vars(source, node)?,
            expect: expect::read(source, node.get("expect"), Origin::Test(id)),
            examples: self
                .rows(source, node.get("examples"))
                .map(Examples::Rows)
                .or_else(|| read::examples(source, node.get("examples"))),
            repeat: read::number(node, "repeat").unwrap_or(1),
            retry: read::retry(source, node),
            timeout: read::duration(source, node, "timeout"),
            skip: read::skip(node),
            standalone: read::flag(shapes::test(), node, "standalone"),
            span: node.span,
        }))
    }

    /// The test's call, or its steps with the test's keys as defaults; a test-level `save` reads
    /// the last step's response (D124).
    fn body(&mut self, source: &Source<'a>, node: &Node, id: TestId) -> Built<Option<TestBody>> {
        if node.get("call").is_some() {
            let place = Place { owner: Owner::Instance, defaults: None, test: Some(id) };
            let Some(action) = self.action(source, node, &place)? else { return Ok(None) };
            return Ok(Some(TestBody::Call(self.builder.action(action)?)));
        }
        let place = Place { owner: Owner::Instance, defaults: Some((id, node)), test: None };
        let steps = project::list(node, "steps");
        let mut ids = Vec::with_capacity(steps.len());
        for (index, step) in steps.iter().enumerate() {
            let Some(mut action) = self.action(source, step, &place)? else { continue };
            if index + 1 == steps.len() {
                for (name, value) in self.values(source, node.get("save"))? {
                    action.save.values.entry(name).or_insert(value);
                }
            }
            ids.push(self.builder.action(action)?);
        }
        Ok(Some(TestBody::Steps(ids)))
    }
}

fn hooks(node: &Node) -> RequestHooks {
    RequestHooks {
        before_request: read::hooks(node, "beforeRequest"),
        after_response: read::hooks(node, "afterResponse"),
    }
}
