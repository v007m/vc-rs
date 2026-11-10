//! Secret providers: the environment and `.env` (the default) and Varlock; and the OS
//! keychain.
//!
//! Each provider implements `vc_interfaces::SecretProvider` and is chosen by
//! `secrets.provider` from the [`SecretProviders`] registry (D163): [`EnvSecrets`] reads the
//! run's environment snapshot, `.env` under the process environment; [`VarlockSecrets`] runs
//! `varlock load --format json` once per run through a `CommandRunner`, never through a shell.
//! [`OsKeychain`] is the `vc_interfaces::Keychain` port over the OS keychain (`keyring-core`
//! and one native store per system).

mod env;
mod keychain;
mod registry;
mod varlock;

pub use env::EnvSecrets;
pub use keychain::OsKeychain;
pub use registry::SecretProviders;
pub use varlock::VarlockSecrets;
