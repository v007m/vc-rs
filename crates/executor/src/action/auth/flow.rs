//! A `flow` auth's steps (syntax reference "Multi-step auth with `flow`"): they run in a frame
//! of their own that starts with the definition's vars, and what they save renders `apply`.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use vc_interfaces::{ActionRunner, AuthError, Credentials, Secret};
use vc_model::{
    ActionId, ActionOutcome, AuthDef, OwnerKey, Pos, SourceFileId, SourceSpan, Template, TemplateValue,
};
use vc_template::RenderError;

use super::APPLY;
use crate::{ActionCx, ActionRequest, Frame, Render, Saved};

/// Runs one `flow` definition's steps for one set of vars and turns its `apply` into
/// credentials.
pub struct FlowRunner {
    cx: Arc<ActionCx>,
    auth: String,
    steps: Vec<ActionId>,
    given: Arc<Saved>,
    apply: Option<TemplateValue>,
    expires_in: Option<Template>,
}

impl fmt::Debug for FlowRunner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FlowRunner").field("auth", &self.auth).field("steps", &self.steps).finish_non_exhaustive()
    }
}

impl FlowRunner {
    /// The flow of `def`, run in `cx` with the vars `given`.
    #[must_use]
    pub fn new(cx: Arc<ActionCx>, def: &AuthDef, given: Arc<Saved>) -> Self {
        let auth = cx.program.str(def.id.sym()).unwrap_or_default().to_owned();
        let (steps, apply, expires_in) = (def.steps.clone(), def.settings.get(APPLY).cloned(), def.expires_in.clone());
        Self { cx, auth, steps, given, apply, expires_in }
    }

    fn request(&self, action: ActionId, frame: &Frame) -> ActionRequest {
        ActionRequest { action, owner: OwnerKey::Run, cx: Arc::clone(&self.cx), frame: frame.clone() }
    }

    /// Why the flow failed, naming the step and the code but never the failure's text, which
    /// may hold what a step sent or received.
    fn failed(&self, step: ActionId, code: Option<&str>) -> AuthError {
        let id = self.cx.action(step).and_then(|action| action.id).and_then(|id| self.cx.program.str(id));
        let step = id.map_or_else(|| format!("step {}", step.index().saturating_add(1)), |id| format!("step `{id}`"));
        let reason = code.map_or_else(|| format!("{step} failed"), |code| format!("{step} failed ({code})"));
        AuthError::Flow { auth: self.auth.clone(), reason }
    }

    /// Why `apply` or `expiresIn` did not render; the message only when it names what is missing.
    fn unrendered(&self, error: &RenderError) -> AuthError {
        let reason = if error.missing { error.message.clone() } else { "`apply` does not render".to_owned() };
        AuthError::Flow { auth: self.auth.clone(), reason }
    }

    /// `value` as how long the credentials hold.
    fn lifetime(&self, value: &Value) -> Result<Duration, AuthError> {
        let reason = || "`expiresIn` is not a number of seconds or a duration".to_owned();
        lifetime(value).ok_or_else(|| AuthError::Flow { auth: self.auth.clone(), reason: reason() })
    }

    /// The credentials `apply` and `expiresIn` render to after the last step.
    fn credentials(&self, last: ActionId, frame: &Frame) -> Result<Credentials, AuthError> {
        let request = self.request(last, frame);
        let render = Render::new(&request).map_err(|error| self.unrendered(&error))?;
        let applied = match &self.apply {
            Some(apply) => render.value(apply).map_err(|error| self.unrendered(&error))?,
            None => Value::Null,
        };
        let mut credentials = Credentials::default();
        credentials.headers = pairs(&applied, "headers");
        credentials.query = pairs(&applied, "query");
        credentials.cookies = pairs(&applied, "cookies");
        credentials.expires_in = match &self.expires_in {
            Some(template) => Some(self.lifetime(&render.template(template).map_err(|error| self.unrendered(&error))?)?),
            None => None,
        };
        Ok(credentials)
    }
}

#[async_trait]
impl ActionRunner for FlowRunner {
    async fn run(&self) -> Result<Credentials, AuthError> {
        let mut frame = Frame { saved: Arc::clone(&self.given), ..Frame::default() };
        let Some(&last) = self.steps.last() else {
            return Err(AuthError::Flow { auth: self.auth.clone(), reason: "the flow has no steps".into() });
        };
        for &step in &self.steps {
            let report = self.cx.executors.execute(self.request(step, &frame)).await;
            if let ActionOutcome::Failed(failure) = &report.outcome {
                return Err(self.failed(step, failure.code.as_deref()));
            }
            frame.absorb(&self.cx.program, step, &report);
        }
        self.credentials(last, &frame)
    }
}

/// The entries of the map `applied[key]`, each as text; nulls are left out.
fn pairs(applied: &Value, key: &str) -> Vec<(String, Secret)> {
    let Some(map) = applied.get(key).and_then(Value::as_object) else { return Vec::new() };
    let set = map.iter().filter(|(_, value)| !value.is_null());
    set.map(|(name, value)| (name.clone(), Secret::new(vc_template::to_text(value)))).collect()
}

/// How long credentials hold: a number of seconds or a duration such as `3600s`.
fn lifetime(value: &Value) -> Option<Duration> {
    match value {
        Value::Number(seconds) => seconds.as_u64().map(Duration::from_secs),
        Value::String(text) => {
            let span = SourceSpan::new(SourceFileId::new(0), Pos::new(1, 1, 0), Pos::new(1, 1, 0));
            vc_template::parse_duration(text.trim(), span).ok().map(|duration| duration.get())
        }
        _ => None,
    }
}
