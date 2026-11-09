//! Plugins on disk: `vc plugin add` fetches one from a `path:` folder or an `npm:` package into a
//! staging folder, and once its permissions are approved it moves to `.vibranium/plugins/<id>/`.
//!
//! [`FsPluginStore`] is the `vc_interfaces::PluginStore` the engine uses. A folder's integrity is a
//! BLAKE3 hash over its files; an npm package's is the one npm reports. Programs (`npm`, `tar`) run
//! with their arguments as a list, never through a shell.

mod folder;
mod npm;
mod store;

pub use store::FsPluginStore;
