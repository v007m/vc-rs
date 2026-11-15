# Vibranium v2

[![Docs](https://img.shields.io/badge/docs-v007m.github.io%2Fvc--rs-7c3aed)](https://v007m.github.io/vc-rs/)

A git-native API testing engine: functional, contract and load tests of HTTP APIs, written as plain YAML and run from a CLI, a terminal UI, a web UI or an AI agent (MCP). It is a ground-up rewrite of [Vibranium v1](https://github.com/sarathm09/vibranium) in Rust.

**Documentation: <https://v007m.github.io/vc-rs/>** (press Cmd/Ctrl+K to search)

- [Quickstart](https://v007m.github.io/vc-rs/start/quickstart/) and [walkthroughs](https://v007m.github.io/vc-rs/walkthroughs/rest-crud/)
- [Playground](https://v007m.github.io/vc-rs/playground/): edit a project in the browser with live validation, then run it with the real engine, no install needed
- [Reference](https://v007m.github.io/vc-rs/reference/): every keyword, matcher, pipe, command and diagnostic
- [For AI agents](https://v007m.github.io/vc-rs/agents/): MCP setup, tools, the agent skill and [`llms.txt`](https://v007m.github.io/vc-rs/llms.txt)
- [JSON Schemas](https://v007m.github.io/vc-rs/reference/schemas/) for editors and agents: `https://v007m.github.io/vc-rs/schema/<config|api|test|load>.schema.json`

## Install

The current release is **v0.1.0**. On macOS and Linux, Homebrew builds it from source with the web UI, and installs `vc`, `vibranium` (the same command) and the `vc-demo` service:

```console
brew install v007m/tap/vibranium
```

`brew install --HEAD v007m/tap/vibranium` builds the latest `master` instead. There are no prebuilt downloads yet.

Without Homebrew, build with Cargo (Rust 1.99 or later). Drop `--tag v0.1.0` to build `master`:

```console
cargo install --git https://github.com/v007m/vc-rs --tag v0.1.0 vc-cli vc-demo --locked
```

That `vc` is built without the web UI (`vc ui`). For the UI, build from a clone with Node and pnpm:

```console
git clone https://github.com/v007m/vc-rs && cd vc-rs
pnpm install --frozen-lockfile --filter web && pnpm --filter web run build
cargo install --path crates/apps/cli --features web-ui --locked
cargo install --path crates/apps/demo --locked
```

See the [install guide](https://v007m.github.io/vc-rs/start/install/) for shell completion and the agent skill.

## A test in ten lines

```yaml
# services/sample/landing.test.yaml
name: User accounts
tests:
  - id: createsUser
    name: Creates a user with the given email
    call: sample#createUser
    vars: { email: ada@example.com }
    expect:
      status: 201
      body:
        match: { email: ada@example.com, id: { type: string } }
```

Make a project with `vc init`, save the test there as `services/sample/landing.test.yaml`, start the built-in demo service and run it:

```console
$ vc init && vc demo start --detach
$ vc run sample.landing
plan  1 test in 1 file · profile default

User accounts
✓ sample.landing#createsUser — Creates a user with the given email    5ms

1 passed                                                     6ms · profile default
reports  /home/ada/shop/.vibranium/reports/1/{ctrf.json,junit.xml,index.html}
```

## Status

v0.1.0 ships milestones M0–M9 of the [implementation plan](docs/design/lld.md#implementation-plan): `vc` with functional, contract and load tests, the terminal UI, the local web UI, MCP for agents, import from OpenAPI, HAR, cURL, Postman and Bruno, and plugins with alerts. The self-hosted team server (`vc serve`, M10) is not built.

## Contribute

- [Open an issue](https://github.com/v007m/vc-rs/issues/new/choose): bug report, feature request or docs issue.
- Report vulnerabilities privately: see [SECURITY.md](SECURITY.md).
- Send a pull request: [CONTRIBUTING.md](CONTRIBUTING.md) covers setup, `mise run gates`, the branch and PR flow, and how decisions are made. Everyone follows the [Code of Conduct](CODE_OF_CONDUCT.md).

Toolchain and tasks are pinned in `mise.toml` (Rust via `rust-toolchain.toml`). Every PR passes `mise run gates`. The spec is in [`docs/design/`](docs/design/): start with [`HANDOVER.md`](docs/design/HANDOVER.md). The docs site lives in [`site/`](site/).

Design documents:

- [Product requirements](docs/design/prd.md)
- [Technical design](docs/design/technical-design.md)
- [Low-level design (Rust)](docs/design/lld.md)
- [Syntax reference](docs/design/syntax-reference.md)
- [CLI and developer workflow](docs/design/cli-workflow.md)
- [Decision log](docs/design/decision-log.md)
- [Language decision](docs/design/language-decision.md)

Licence: Apache-2.0.
