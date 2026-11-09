//! The sample project the engine's unit tests plan and list.

use std::path::PathBuf;
use std::sync::Arc;

use vc_interfaces::{SourceProvider, Vcs};
use vc_model::Date;
use vc_testkit::{FakeVcs, FixedClock, MemoryApprovals, MemoryCompileCache, MemoryEnvironment, MemorySources};

use crate::{Engine, EngineBuilder};

const CONFIG: (&str, &[u8]) = (
    "vibranium.config.yaml",
    b"syntaxVersion: 2\nprofiles:\n  - id: dev\n    default: true\n  - id: staging\nvars: {baseUrl: 'http://x'}\n",
);
const USERS: (&str, &[u8]) = (
    "services/users/users.api.yaml",
    b"baseUrl: ${var.baseUrl}\napis:\n  - id: list\n    name: List users\n    path: /users\n",
);
const CRUD: (&str, &[u8]) = (
    "services/users/crud.test.yaml",
    b"tags: [smoke]\ntests:\n  - id: listsUsers\n    name: Lists users\n    call: users#list\n    tags: [fast, smoke]\n    repeat: 2\n  - id: skipsUsers\n    call: users#list\n    skip: {reason: flaky}\n",
);
const ORDERS: (&str, &[u8]) =
    ("services/orders/flow.test.yaml", b"tests:\n  - id: listsOrders\n    call: users#list\n");

/// A config whose profiles have the `agents` blocks of the reference's example, and a bare one.
const AGENTS: &[u8] = b"syntaxVersion: 2
vars: {baseUrl: 'http://x'}
redact: {fields: [password]}
profiles:
  - id: dev
    default: true
    agents: {run: true, call: [GET, POST, PUT, DELETE], writeDefinitions: true}
  - id: staging
    agents: {run: true, call: [GET, POST], approve: [PUT, PATCH, DELETE], writeDefinitions: ask}
  - id: prod
    agents: {call: [GET], approve: [POST]}
  - id: bare
";

/// An engine over a project with the profiles of [`AGENTS`], keeping its approval queue and
/// agent log in `queue`.
pub fn agent_engine(queue: &MemoryApprovals) -> Engine {
    let sources: Arc<dyn SourceProvider> = Arc::new(MemorySources::new(&[("vibranium.config.yaml", AGENTS)]));
    let ports = (Arc::new(queue.clone()), Arc::new(queue.clone()));
    builder(FakeVcs::default()).sources(sources).approvals(ports.0, ports.1).build().unwrap()
}

/// An engine over the sample project, with `vcs` for `--changed`.
pub fn engine(vcs: impl Vcs + 'static) -> Engine {
    builder(vcs).build().unwrap()
}

/// The builder of [`engine`], for a test that adds ports.
pub fn builder(vcs: impl Vcs + 'static) -> EngineBuilder {
    let sources: Arc<dyn SourceProvider> = Arc::new(MemorySources::new(&[CONFIG, USERS, CRUD, ORDERS]));
    EngineBuilder::new(PathBuf::from("/project"))
        .sources(sources)
        .compile_cache(Arc::new(MemoryCompileCache::new()))
        .clock(Arc::new(FixedClock(Date::new(2026, 11, 5).unwrap())))
        .environment(Arc::new(MemoryEnvironment::new(&[])))
        .vcs(Arc::new(vcs))
}
