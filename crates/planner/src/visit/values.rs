//! The walker's leaves: values, templates, bodies and expectations, down to each reference.

use vc_model::{BodySpec, Examples, Expect, Matcher, MatcherArg, Part, PartSpec, Template, TemplateValue};

use super::{Visit, Walker};

impl<V: Visit> Walker<'_, V> {
    pub(super) fn examples(&mut self, examples: &Examples) {
        match examples {
            Examples::Rows(rows) => self.values(rows),
            Examples::File(template) | Examples::Template(template) => self.template(template),
        }
    }

    pub(super) fn body(&mut self, body: &BodySpec) {
        match body {
            BodySpec::None => {}
            BodySpec::Json(value) => self.value(value),
            BodySpec::Text(template) | BodySpec::File(template) | BodySpec::Base64(template) => {
                self.template(template);
            }
            BodySpec::Extend { base, changes, .. } => {
                self.template(base);
                self.value(changes);
            }
            BodySpec::Form(fields) => self.values(fields.values()),
            BodySpec::Multipart(parts) => {
                for part in parts.values() {
                    match part {
                        PartSpec::Value(value) => self.value(value),
                        PartSpec::File(template) => self.template(template),
                    }
                }
            }
        }
    }

    pub(super) fn expect(&mut self, expect: &Expect) {
        for check in &expect.checks {
            self.matcher(&check.matcher);
        }
    }

    fn matcher(&mut self, matcher: &Matcher) {
        match matcher {
            Matcher::Equals(value) | Matcher::StatusOf(value) => self.value(value),
            Matcher::All(calls) => {
                for call in calls {
                    self.matcher_arg(&call.arg);
                }
            }
            Matcher::Status(_) => {}
        }
    }

    fn matcher_arg(&mut self, arg: &MatcherArg) {
        match arg {
            MatcherArg::Value(value) => self.value(value),
            MatcherArg::Range(bounds) => bounds.iter().for_each(|bound| self.matcher_arg(bound)),
            MatcherArg::Matcher(matcher) => self.matcher(matcher),
            MatcherArg::Matchers(matchers) => matchers.iter().for_each(|matcher| self.matcher(matcher)),
            MatcherArg::Duration(_) | MatcherArg::Size(_) => {}
        }
    }

    pub(super) fn values<'v>(&mut self, values: impl IntoIterator<Item = &'v TemplateValue>) {
        for value in values {
            self.value(value);
        }
    }

    pub(super) fn value(&mut self, value: &TemplateValue) {
        match value {
            TemplateValue::String(template) => self.template(template),
            TemplateValue::List(items) => self.values(items),
            TemplateValue::Map(entries) => self.values(entries.values()),
            TemplateValue::Cel(program) => program.refs.iter().for_each(|reference| self.visitor.reference(reference)),
            TemplateValue::Extend { base, changes, .. } => {
                self.template(base);
                self.value(changes);
            }
            TemplateValue::Null | TemplateValue::Bool(_) | TemplateValue::Number(_) => {}
        }
    }

    pub(super) fn templates<'t>(&mut self, templates: impl IntoIterator<Item = &'t Template>) {
        for template in templates {
            self.template(template);
        }
    }

    pub(super) fn template(&mut self, template: &Template) {
        for part in &template.parts {
            if let Part::Ref(reference) = part {
                self.visitor.reference(reference);
            }
        }
    }
}
