//! The client certificate a call presents (D350, D390): the one its auth's `clientCert` names,
//! none for `clientCert: none`, else whichever identity's `hosts` match the URL.

use std::collections::BTreeMap;

use vc_diagnostics::Code;
use vc_interfaces::{CertChoice, CertId};
use vc_model::{Action, Api, ClientCert, Failure, Program, TlsClientId};

use super::chosen;

/// The identities the run loaded, each with the `CertId` its transport knows it by. Only those
/// the active profiles allow are here (D354).
pub type ClientCerts = BTreeMap<TlsClientId, CertId>;

/// The certificate choice for `call` under the run's `loaded` identities.
///
/// # Errors
///
/// V5114 when the call's auth names an identity the active profiles did not load.
pub fn cert_choice(program: &Program, loaded: &ClientCerts, call: (&Api, &Action)) -> Result<CertChoice, Failure> {
    match chosen(program, call).and_then(|(def, _)| def.client_cert.as_ref()) {
        None => Ok(CertChoice::Host),
        Some(ClientCert::Off) => Ok(CertChoice::Off),
        Some(ClientCert::Named(id)) => {
            loaded.get(id).map(|cert| CertChoice::Named(*cert)).ok_or_else(|| unloaded(program, *id))
        }
    }
}

/// V5114 for the identity `id`, which no active profile loads: it names the profiles that do.
fn unloaded(program: &Program, id: TlsClientId) -> Failure {
    let name = program.str(id.sym()).unwrap_or("?");
    let client = program.config.http_tls.clients.get(&id);
    let profiles = client.into_iter().flat_map(|client| &client.profiles);
    let flags: Vec<_> =
        profiles.map(|profile| format!("`--profile {}`", program.str(profile.sym()).unwrap_or("?"))).collect();
    let message = format!("the client certificate `{name}` did not load; it is only sent under {}", flags.join(" or "));
    Failure::new(message).with_code(Code::V5114.to_string())
}

#[cfg(test)]
#[path = "cert_tests.rs"]
mod tests;
