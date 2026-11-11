//! One call as `vc explain` shows it: the method and URL, the vars, the headers and the auth in
//! effect with the level that set each, the checks with the default status, and the cleanup.

use std::fmt::Write as _;

use indexmap::IndexMap;
use vc_model::{Action, Api, AuthRef, CheckKind, Template, TemplateValue};

use super::api::SentHeader;
use super::{CONFIG, Explainer};
use crate::shown::{Shown, Var};

/// The api a call sends and the action that calls it; no action when an api is explained alone.
pub struct Called<'a> {
    pub api: &'a Api,
    pub action: Option<&'a Action>,
}

/// The headers one level sets on a call, with that level's origin.
type HeaderLevel<'h> = (String, &'h IndexMap<String, Template>);

/// The origin of the action's own `vars`, `headers` and `auth`.
pub const CALL: &str = "call";

impl Explainer<'_> {
    /// The method and URL, the vars, the headers, the auth, the checks and the cleanup.
    pub(super) fn request(&self, called: &Called<'_>, vars: &[Var]) -> String {
        let api = called.api;
        let shown = Shown { program: self.program, vars };
        let base = self.program.api_groups.get(&api.group).and_then(|group| group.base_url.as_ref());
        let base = base.map(|base| shown.template(base)).unwrap_or_default();
        let mut out = format!("  {} {base}{}\n  vars\n", method(api), shown.template(&api.path));
        for var in vars {
            let _ = writeln!(out, "    {}  {}  ({})", var.name, shown.value(&var.value), var.origin);
        }
        out.push_str(&self.headers(called, &shown));
        out.push_str(&self.auth(called));
        out.push_str(&self.expect(called, &shown));
        if let Some(cleanup) = api.cleanup {
            let _ = writeln!(out, "  cleanup  {}", self.cleanup(cleanup));
        }
        out
    }

    /// The headers sent, as [`Explainer::sent_headers`] gives them.
    fn headers(&self, called: &Called<'_>, shown: &Shown<'_>) -> String {
        let sent = self.sent_headers(called, shown);
        if sent.is_empty() {
            return String::new();
        }
        let mut out = String::from("  headers\n");
        for header in sent {
            let _ = writeln!(out, "    {}  {}  ({})", header.name, header.value, header.from);
        }
        out
    }

    /// The headers sent, merged from the config's `defaults.headers`, the api file's, the api's
    /// and the call's; a later one takes the place of an earlier one of the same name, in any
    /// case. Values are shown, not masked.
    pub(super) fn sent_headers(&self, called: &Called<'_>, shown: &Shown<'_>) -> Vec<SentHeader> {
        let levels = self.header_levels(called);
        let mut merged: Vec<(&str, &Template, &str)> = Vec::new();
        for (origin, headers) in &levels {
            for (name, template) in *headers {
                let header = (name.as_str(), template, origin.as_str());
                match merged.iter_mut().find(|(known, ..)| known.eq_ignore_ascii_case(name)) {
                    Some(known) => *known = header,
                    None => merged.push(header),
                }
            }
        }
        let sent = merged.into_iter().map(|(name, template, from)| SentHeader {
            name: name.to_owned(),
            value: shown.template(template),
            from: from.to_owned(),
        });
        sent.collect()
    }

    /// Each level that sets headers on the call, furthest first, with its origin.
    pub(super) fn header_levels<'h>(&'h self, called: &Called<'h>) -> Vec<HeaderLevel<'h>> {
        let api = called.api;
        let mut levels = vec![(CONFIG.to_owned(), &self.program.config.defaults.headers)];
        if let Some(group) = self.program.api_groups.get(&api.group) {
            levels.push((format!("file {}", self.text(group.id.sym())), &group.headers));
        }
        levels.push((format!("api {}", self.text(api.id.sym())), &api.headers));
        levels.extend(called.action.map(|action| (CALL.to_owned(), &action.headers)));
        levels
    }

    /// `auth  admin  (api users#create)`: as [`Explainer::auth_used`] finds it, or `none`;
    /// nothing when no level sets one.
    fn auth(&self, called: &Called<'_>) -> String {
        match self.auth_used(called) {
            Some((AuthRef::Use { id, .. }, origin)) => format!("  auth  {}  ({origin})\n", self.text(id.sym())),
            Some((_, origin)) => format!("  auth  none  ({origin})\n"),
            None => String::new(),
        }
    }

    /// The first of the call's, the api's and the api file's `auth` that does not inherit, with
    /// the level that set it.
    pub(super) fn auth_used<'c>(&'c self, called: &Called<'c>) -> Option<(&'c AuthRef, String)> {
        let api = called.api;
        let group = self.program.api_groups.get(&api.group);
        let levels = [
            called.action.and_then(|action| action.auth.as_ref()).map(|auth| (auth, CALL.to_owned())),
            Some((&api.auth, format!("api {}", self.text(api.id.sym())))),
            group.map(|group| (&group.auth, format!("file {}", self.text(group.id.sym())))),
        ];
        levels.into_iter().flatten().find(|(auth, _)| !matches!(auth, AuthRef::Inherit))
    }

    /// The merged checks with the values they expect, after the `status: 2xx` a run judges a
    /// call by when none checks the status.
    fn expect(&self, called: &Called<'_>, shown: &Shown<'_>) -> String {
        let expect = called.action.map_or(&called.api.expect, |action| &action.expect);
        let mut out = String::from("  expect\n");
        if !expect.checks.iter().any(|check| check.kind == CheckKind::Status) {
            out.push_str("    status: 2xx  (default)\n");
        }
        let show = |value: &TemplateValue| shown.value(value);
        for line in vc_planner::shown_check_lines(self.program, expect, &show) {
            let _ = writeln!(out, "    {line}");
        }
        out
    }
}

/// `GET`, `POST`…
fn method(api: &Api) -> String {
    format!("{:?}", api.method).to_uppercase()
}
