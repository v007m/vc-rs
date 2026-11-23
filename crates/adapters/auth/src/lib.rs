//! Auth types: `basic`, `bearer`, `apiKey`, the OAuth 2.0 grants, `flow` (D106), `mtls`, which
//! adds nothing because the HTTP client presents its certificate (D350), `awsSigV4`, whose
//! credentials sign each request as it leaves ([`AwsSigV4`], D434), and `plugin`, the
//! credentials an auth plugin returns ([`PluginAuth`]).
//!
//! Each type implements `vc_interfaces::AuthProvider` and is one entry in the [`AuthTypes`]
//! registry, which is itself a provider handing each request to the entry for its `type`.
//! Token endpoints are reached through the injected `Transport`; the authorization code type
//! signs in through the `Browser` port and keeps refresh tokens through the `Keychain` port.
//! Nothing here caches: the executor's token cache does. The WebAssembly build has no browser
//! sign-in ([`AuthTypes::headless`]): it cannot listen for the redirect.

#[cfg(not(target_family = "wasm"))]
mod code;
mod fixed;
mod oauth;
mod plugin;
#[cfg(not(target_family = "wasm"))]
mod redirect;
mod registry;
mod sigv4;
#[cfg(test)]
mod testing;
mod token;

#[cfg(not(target_family = "wasm"))]
pub use code::AuthorizationCode;
pub use fixed::{ApiKey, Basic, Bearer, Flow, Mtls};
pub use oauth::{ClientCredentials, PasswordGrant, RefreshGrant};
pub use plugin::PluginAuth;
pub use registry::AuthTypes;
pub use sigv4::AwsSigV4;
