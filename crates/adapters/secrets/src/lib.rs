//! Secret providers: the environment and `.env` (the default) and Varlock.
//!
//! Each provider implements `vc_interfaces::SecretProvider` and is chosen by
//! `secrets.provider` from the [`SecretProviders`] registry (D163): [`EnvSecrets`] reads the
//! run's environment snapshot, `.env` under the process environment; [`VarlockSecrets`] runs
//! `varlock load --format json` once per run through a `CommandRunner`, never through a shell.

mod env;
mod registry;
mod varlock;

pub use env::EnvSecrets;
pub use registry::SecretProviders;
pub use varlock::VarlockSecrets;
