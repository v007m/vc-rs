#!/usr/bin/env bash
# Every machine-enforced quality gate for the workspace. Exit non-zero on the first failure.
# Intended for CI and for AI agents to run before declaring work done.
set -euo pipefail
cd "$(dirname "$0")"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"

step() { printf '\n== %s\n' "$1"; }

step "format"
cargo fmt --all --check

step "clippy (pedantic + nursery + workspace lints, warnings are errors)"
cargo clippy --workspace --all-targets -- -D warnings -W clippy::pedantic -W clippy::nursery

step "tests (unit, architecture rules, end-to-end against the mock server)"
cargo test --workspace

step "duplication (jscpd, >=50 tokens)"
npx -y jscpd@4 crates --min-tokens 50 --min-lines 5 --threshold 0 --reporters console --ignore "**/target/**" --silent

step "supply chain (cargo-deny: advisories, licenses, bans, sources)"
cargo deny check

step "unused dependencies (cargo-machete)"
cargo machete

if [[ "${GATES_MUTANTS:-0}" == "1" ]]; then
  step "mutation testing (cargo-mutants on engine; every viable mutant must be caught)"
  cargo mutants -p engine --timeout 180 -j 1
fi

printf '\nall gates passed\n'
