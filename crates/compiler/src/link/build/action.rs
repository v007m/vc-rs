//! One action, with the test-level defaults a step inherits (D124) and its checks merged over
//! the api's (D123).

use vc_model::{Action, ActionKind, ApiId, AuthRef, BodySpec, Expect, Origin, Owner, Poll, SaveSpec, TestId, VarDefs};
use vc_parser::Node;

use super::{Build, Built, expect, read};
use crate::link::project;
use crate::link::project::Source;
use crate::shapes;

/// The test a step takes its defaults from, and its node.
pub type Defaults<'n> = Option<(TestId, &'n Node)>;

/// Where an action sits and what it inherits.
pub struct Place<'n> {
    /// Who owns its results.
    pub owner: Owner,
    /// The test whose `auth`, `headers`, `query`, `body`, `poll` and `expect` are this step's
    /// defaults (D124).
    pub defaults: Defaults<'n>,
    /// Set when the node is a test with `call`: its vars, checks, retry and repeat belong to the
    /// test.
    pub test: Option<TestId>,
}

/// The request parts an action sends over the api's.
struct Request {
    auth: Option<AuthRef>,
    headers: indexmap::IndexMap<String, vc_model::Template>,
    query: indexmap::IndexMap<String, vc_model::Template>,
    body: Option<BodySpec>,
}

impl<'a> Build<'_, 'a> {
    /// The action at `node`, or `None` when it names nothing to run.
    pub fn action(&mut self, source: &Source<'a>, node: &Node, place: &Place<'_>) -> Built<Option<Action>> {
        let Some(kind) = self.kind(source, node)? else { return Ok(None) };
        let origin = place.test.map_or(self.builder.next_action().map(Origin::Action)?, Origin::Test);
        let defaults = place.defaults.filter(|_| matches!(kind, ActionKind::Call(_) | ActionKind::Test(_)));
        let request = self.request(source, node, defaults.map(|(_, test)| test))?;
        let expect = self.checks(source, (node, &kind), (origin, defaults));
        let poll = node.get("poll").or_else(|| defaults.and_then(|(_, test)| test.get("poll")));
        let vars = if place.test.is_some() { VarDefs::default() } else { self.vars(source, node)? };
        let id = project::text(node, "id").filter(|_| place.test.is_none()).map(|id| self.sym(id)).transpose()?;
        Ok(Some(Action {
            id,
            kind,
            owner: place.owner,
            vars,
            auth: request.auth,
            headers: request.headers,
            query: request.query,
            body: request.body,
            expect,
            save: SaveSpec { values: self.values(source, node.get("save"))? },
            poll: poll.and_then(|poll| poll_of(source, poll, origin)),
            retry: read::retry(source, node).filter(|_| place.test.is_none()),
            keep: read::flag(shapes::action(), node, "keep"),
            cache: None,
            repeat: read::looping(source, node).filter(|_| place.test.is_none()),
            span: node.span,
        }))
    }

    /// `call`, `test`, `function` or `command`.
    fn kind(&mut self, source: &Source<'a>, node: &Node) -> Built<Option<ActionKind>> {
        if let Some(call) = project::text(node, "call") {
            return Ok(Some(ActionKind::Call(ApiId::new(self.sym(&source.resolve(call))?))));
        }
        if let Some(test) = project::text(node, "test") {
            return Ok(Some(ActionKind::Test(TestId::new(self.sym(&source.resolve(test))?))));
        }
        if let Some(function) = project::text(node, "function") {
            return Ok(Some(ActionKind::Function(read::hook(function))));
        }
        let command = node.get("command").and_then(|command| read::template(source, command));
        Ok(command.map(ActionKind::Command))
    }

    /// The step's own request parts, over the test's defaults; headers and query merge by name.
    fn request(&mut self, source: &Source<'a>, node: &Node, defaults: Option<&Node>) -> Built<Request> {
        let auth = match self.auth(source, node)? {
            Some(auth) => Some(auth),
            None => defaults.map(|test| self.auth(source, test)).transpose()?.flatten(),
        };
        let merged = |key: &str| {
            let mut map = defaults.map(|test| read::templates(source, test, key)).unwrap_or_default();
            map.extend(read::templates(source, node, key));
            map
        };
        let body = node.get("body").or_else(|| defaults.and_then(|test| test.get("body")));
        let body = body.and_then(|body| read::body(source, body));
        Ok(Request { auth, headers: merged("headers"), query: merged("query"), body })
    }

    /// The api's checks, then the test's defaults, then the action's own (D123); a `test:` step
    /// has no api, and a `function` or `command` step only its own.
    fn checks(
        &self,
        source: &Source<'_>,
        (node, kind): (&Node, &ActionKind),
        (origin, defaults): (Origin, Defaults<'_>),
    ) -> Expect {
        let own = expect::read(source, node.get("expect"), origin);
        let api = match kind {
            ActionKind::Call(api) => self.api_expects.get(api),
            ActionKind::Test(_) => None,
            ActionKind::Function(_) | ActionKind::Command(_) | ActionKind::Inline(_) => return own,
        };
        let test = defaults.map(|(id, test)| expect::read(source, test.get("expect"), Origin::Test(id)));
        let empty = Expect::default();
        expect::merge(&[api.unwrap_or(&empty), test.as_ref().unwrap_or(&empty), &own])
    }
}

/// `poll`: the checks that end it, the pause and the time limit, all three required (D156).
fn poll_of(source: &Source<'_>, poll: &Node, origin: Origin) -> Option<Poll> {
    let every = read::duration(source, poll, "every")?;
    let timeout = read::duration(source, poll, "timeout")?;
    Some(Poll { until: expect::until(source, poll, origin), every, timeout })
}
