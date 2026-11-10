//! Auth types: `basic`, `bearer`, `apiKey`, the OAuth 2.0 grants, `flow` (D106) and `plugin`, the
//! credentials an auth plugin returns ([`PluginAuth`]).
//!
//! Each type implements `vc_interfaces::AuthProvider` and is one entry in the [`AuthTypes`]
//! registry, which is itself a provider handing each request to the entry for its `type`.
//! Token endpoints are reached through the injected `Transport`; the authorization code type
//! signs in through the `Browser` port and keeps refresh tokens through the `Keychain` port.
//! Nothing here caches: the executor's token cache does.

mod code;
mod fixed;
mod oauth;
mod plugin;
mod redirect;
mod registry;
#[cfg(test)]
mod testing;
mod token;

pub use code::AuthorizationCode;
pub use fixed::{ApiKey, Basic, Bearer, Flow};
pub use oauth::{ClientCredentials, PasswordGrant, RefreshGrant};
pub use plugin::PluginAuth;
pub use registry::AuthTypes;
