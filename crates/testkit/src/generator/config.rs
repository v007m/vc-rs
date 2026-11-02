use std::fmt::Write as _;

use super::rng::{Rng, fill};

/// The settings, the shared `pageSize` var and the `vars:` key the service URLs follow.
const HEAD: &str = "syntaxVersion: 2
project: generated

defaults:
  timeout: 10s
  concurrency: 32
  perHost: 16
  headers:
    accept: application/json

http:
  retry:
    attempts: 2

vars:
  pageSize: 20
";

/// The profiles: `dev` is the default and `eu` changes the page size.
const PROFILES: &str = "
profiles:
  - id: dev
    default: true
  - id: eu
    vars:
      pageSize: 50
";

/// A profile that moves every service; its URLs follow.
const STAGING: &str = "  - id: staging
    vars:
";

/// A run-level item every test file can read as `${var.admin.itemId}`.
const RUN_BLOCKS: &str = "
beforeRun:
  - id: admin
    call: %first%#create
    vars:
      label: admin-%word%

afterRun:
  - command: echo generated run finished
";

/// The project config for these services (service names, in order).
pub fn render(services: &[String], rng: &mut Rng) -> String {
    let mut text = HEAD.to_owned();
    for name in services {
        let _ = writeln!(text, "  {name}Url: http://localhost:18080/{name}");
    }
    text.push_str(PROFILES);
    if let Some(first) = services.first() {
        text.push_str(STAGING);
        for name in services {
            let _ = writeln!(text, "      {name}Url: https://{name}.staging.example.test");
        }
        text.push_str(&fill(RUN_BLOCKS, &[("first", first), ("word", rng.word())]));
    }
    text
}
