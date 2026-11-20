//! A config value rendered once before the run, such as an `http.tls` path or passphrase.

use vc_model::{Program, Template};
use vc_template::RenderError;

use super::render::{Render, Shared};
use super::run::RunValues;
use super::scope::Scope;
use crate::{Redactor, Schemas};

/// `template` rendered to text with the config's vars, `secret`, `env` and `dataset` in scope,
/// as the active profiles give them.
///
/// # Errors
///
/// When the template does not render, such as a secret that did not load (V5114).
pub fn config_text(program: &Program, values: &RunValues, template: &Template) -> Result<String, RenderError> {
    let (redactor, schemas) = (Redactor::new(&[]), Schemas::default());
    let shared = Shared { values, program, redactor: &redactor, schemas: &schemas };
    Render::over(shared, Scope::of_config(values), values.inputs().random.as_ref()).text(template)
}

#[cfg(test)]
mod tests {
    use vc_diagnostics::Code;
    use vc_interfaces::{Secret, Secrets};

    use super::config_text;
    use crate::RunValues;
    use crate::fixture::{self, Fixture};

    #[test]
    fn renders_config_vars_secrets_and_env() {
        let mut fixture = Fixture::new();
        fixture.program.config.vars.values = fixture.vars(&[("path", "certs/${env.TIER}")]);
        let mut secrets = Secrets::new();
        secrets.insert("PASS", Secret::new("p4ss-fixture"));
        let inputs = fixture::inputs(&[("TIER", "gold")]).with_secrets(secrets);
        let values = RunValues::new(&fixture.program, inputs);
        let path = fixture.template("${var.path}/a.pem");
        assert_eq!(config_text(&fixture.program, &values, &path), Ok("certs/gold/a.pem".to_owned()));
        let pass = fixture.template("${secret.PASS}");
        assert_eq!(config_text(&fixture.program, &values, &pass), Ok("p4ss-fixture".to_owned()));
        let error = config_text(&fixture.program, &values, &fixture.template("${secret.NOPE}")).unwrap_err();
        assert_eq!(error.code, Some(Code::V5114));
    }
}
