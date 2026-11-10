//! Test fakes, fixture builders and one contract suite per interface trait (dev-dependency only).
//!
//! `generate_project` builds a realistic, seeded project of any size for benchmarks and
//! large-project tests.
//!
//! Every implementation of a `vc-interfaces` trait, fakes included, runs that trait's suite from
//! a test (`<snake_trait>_suite`, D112); `arch-tests` checks that it does:
//!
//! ```
//! use vc_interfaces::SourceProvider;
//! use vc_testkit::{MemorySources, source_provider_suite};
//!
//! source_provider_suite(&mut |files| Box::new(MemorySources::new(files)) as Box<dyn SourceProvider>);
//! ```

mod action_cache_suite;
mod approvals_suite;
mod auth_suite;
mod cache_suite;
mod clock_suite;
mod config_source_suite;
mod fake_auth;
mod fake_hooks;
mod fake_importer;
mod fake_notifiers;
mod fake_plugin;
mod fake_random;
mod fake_secrets;
mod fake_transport;
mod fake_vcs;
mod faulty_transport;
mod fixed_clock;
mod generator;
mod history_suite;
mod hooks_suite;
mod importer_suite;
mod memory_action_cache;
mod memory_approvals;
mod memory_cache;
mod memory_history;
mod memory_sources;
mod notifier_suite;
mod plugin_host_suite;
mod plugin_store_suite;
mod random_suite;
mod recording_reporter;
mod recording_transport;
mod reporter_suite;
mod secrets_suite;
mod source_suite;
mod transport_suite;
mod vcs_suite;
mod watcher_suite;

pub use action_cache_suite::{MakeCacheStore, cache_store_suite};
pub use approvals_suite::{MakeApprovals, agent_action_reader_suite, agent_action_writer_suite, approvals_suite};
pub use auth_suite::{action_runner_suite, auth_provider_suite, browser_suite, keychain_suite};
pub use cache_suite::{MakeCompileCache, compile_cache_store_suite};
pub use clock_suite::clock_suite;
pub use config_source_suite::config_source_suite;
pub use fake_auth::{FakeAuth, FixedFlow, MemoryKeychain, RecordingBrowser};
pub use fake_hooks::{FakeHooks, Script};
pub use fake_importer::FakeImporter;
pub use fake_notifiers::{FakeNotifiers, Sent, SentLog};
pub use fake_plugin::{FakePlugin, PluginScript};
pub use fake_random::{MemoryEnvironment, ScriptedRandom};
pub use fake_secrets::{Answer, FakeCommands, MemorySecrets};
pub use fake_transport::{FakeTransport, Respond, status};
pub use fake_vcs::FakeVcs;
pub use faulty_transport::{Fault, FaultyTransport};
pub use fixed_clock::FixedClock;
pub use generator::{GeneratedFile, GeneratedProject, generate_project};
pub use history_suite::{MakeHistory, history_reader_suite, history_writer_suite};
pub use hooks_suite::{MakeHookRunner, SUITE_HOOKS, SUITE_JS, hook_runner_suite};
pub use importer_suite::importer_suite;
pub use memory_action_cache::MemoryCacheStore;
pub use memory_approvals::MemoryApprovals;
pub use memory_cache::MemoryCompileCache;
pub use memory_history::MemoryHistory;
pub use memory_sources::MemorySources;
pub use notifier_suite::{ChannelCase, MakeNotifier, notifier_factory_suite, notifier_suite};
pub use plugin_host_suite::{MakePluginHost, plugin_host_suite, suite_plugin_manifest};
pub use plugin_store_suite::{MakePluginStore, plugin_store_suite};
pub use random_suite::{MakeEnvironment, MakeRandom, environment_suite, random_suite};
pub use recording_reporter::{RecordingReporter, ReportLog, Reported};
pub use recording_transport::{Recorded, RecordingTransport, Recordings};
pub use reporter_suite::{MakeReporter, reporter_suite};
pub use secrets_suite::{MakeSecretProvider, command_runner_suite, secret_provider_suite};
pub use source_suite::{MakeSources, MakeWriter, source_provider_suite, source_writer_suite};
pub use transport_suite::transport_suite;
pub use vcs_suite::{MakeVcs, VcsHistory, vcs_suite};
pub use watcher_suite::{SaveFile, file_watcher_suite};
