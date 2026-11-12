# Vibranium v2

[![Docs](https://img.shields.io/badge/docs-v007m.github.io%2Fvc--rs-7c3aed)](https://v007m.github.io/vc-rs/)

A git-native API testing engine: functional, contract and load tests of HTTP APIs, written as plain YAML and run from a CLI, a terminal UI, a web UI or an AI agent (MCP). It is a ground-up rewrite of [Vibranium v1](https://github.com/sarathm09/vibranium) in Rust.

**Documentation: <https://v007m.github.io/vc-rs/>** (press Cmd/Ctrl+K to search)

- [Quickstart](https://v007m.github.io/vc-rs/start/quickstart/) and [walkthroughs](https://v007m.github.io/vc-rs/walkthroughs/rest-crud/)
- [Syntax playground](https://v007m.github.io/vc-rs/playground/): edit YAML in the browser with live validation
- [Reference](https://v007m.github.io/vc-rs/reference/): every keyword, matcher, pipe, command and diagnostic
- [For AI agents](https://v007m.github.io/vc-rs/agents/): MCP setup, tools, the agent skill and [`llms.txt`](https://v007m.github.io/vc-rs/llms.txt)
- [JSON Schemas](https://v007m.github.io/vc-rs/reference/schemas/) for editors and agents: `https://v007m.github.io/vc-rs/schema/<config|api|test|load>.schema.json`

## Install

Build `vc` and the demo service from source (release downloads are planned):

```console
cargo install --git https://github.com/v007m/vc-rs vc-cli vc-demo --locked
```

That `vc` is built without the web UI (`vc ui`). For the UI, build from a clone with Node and pnpm:

```console
git clone https://github.com/v007m/vc-rs && cd vc-rs
pnpm install --frozen-lockfile --filter web && pnpm --filter web run build
cargo install --path crates/apps/cli --features web-ui --locked
cargo install --path crates/apps/demo --locked
```

`brew install --HEAD v007m/tap/vibranium` builds both, with the UI ([install guide](https://v007m.github.io/vc-rs/start/install/)).

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

## Develop

Toolchain and tasks are pinned in `mise.toml` (Rust via `rust-toolchain.toml`). Every PR passes `mise run gates`. Start with [`CLAUDE.md`](CLAUDE.md) and [`docs/design/HANDOVER.md`](docs/design/HANDOVER.md); the docs site lives in [`site/`](site/).

Design documents:

- [Product requirements](docs/design/prd.md)
- [Technical design](docs/design/technical-design.md)
- [Low-level design (Rust)](docs/design/lld.md)
- [Syntax reference](docs/design/syntax-reference.md)
- [CLI and developer workflow](docs/design/cli-workflow.md)
- [Decision log](docs/design/decision-log.md)
- [Language decision](docs/design/language-decision.md)

Licence: Apache-2.0.
