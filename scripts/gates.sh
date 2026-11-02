#!/usr/bin/env bash
# Every machine-enforced quality gate (LLD "Design rules and quality gates"), in order.
# Stops at the first failure and prints "FAILED <gate-id>". Tools come from mise (`mise run gates`).
# GATES_MUTANTS=1 adds mutation testing on the diff against GATES_BASE (default origin/master).
set -euo pipefail
cd "$(dirname "$0")/.."

gate() {
  local id=$1 title=$2
  shift 2
  printf '\n==> %s: %s\n' "$id" "$title"
  if ! "$@"; then
    printf '\nFAILED %s\n' "$id" >&2
    exit 1
  fi
}

duplication() {
  pnpm exec jscpd crates bench --format rust --min-lines 10 --threshold 0 \
    --ignore '**/target/**,**/tests/fixtures/**' --reporters console --no-tips
}

tests() {
  cargo nextest run --workspace --locked && cargo test --workspace --doc --locked
}

gate GATE-FMT "formatting" cargo fmt --all --check
gate GATE-CLIPPY "clippy pedantic + nursery, warnings are errors" \
  cargo clippy --workspace --all-targets --locked -- -D warnings
gate GATE-ARCH "architecture rules" cargo nextest run -p vc-arch-tests --locked
gate GATE-DENY "licences, advisories, bans, sources" cargo deny check
gate GATE-MACHETE "unused dependencies" cargo machete
gate GATE-DUP "duplicated blocks of 10+ lines" duplication
gate GATE-TEST "tests and doctests" tests
if [[ "${GATES_MUTANTS:-0}" == "1" ]]; then
  gate GATE-MUTANTS "mutation score on the diff" scripts/mutants.sh diff "${GATES_BASE:-origin/master}"
fi

printf '\nall gates passed\n'
