#!/usr/bin/env bash
# Every machine-enforced quality gate (LLD "Design rules and quality gates"), in order.
# Stops at the first failure and prints "FAILED <gate-id>". Tools come from mise (`mise run gates`).
# GATES_MUTANTS=1 adds mutation testing on the diff against GATES_BASE (default origin/master).
# GATE-PROTO checks proto/ for breaking changes against GATES_PROTO_BASE (default origin/master).
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

# --recursive covers every workspace package; without it pnpm lists only the root's dependencies.
js_licenses() {
  pnpm licenses list --prod --json --recursive | node scripts/js-licenses.mjs deny.toml
}

# The base copy comes from `git archive`: buf's `.git#ref=` input cannot read a linked worktree.
proto_contract() {
  local base=${GATES_PROTO_BASE:-origin/master} against status=0
  if [[ ! -f proto/buf.yaml ]]; then
    echo "notice: no proto/ here; skipping"
    return 0
  fi
  buf lint proto || return 1
  if ! git rev-parse --verify --quiet "$base^{commit}" >/dev/null; then
    echo "base $base not found; fetch it or set GATES_PROTO_BASE" >&2
    return 1
  fi
  if ! git cat-file -e "$base:proto/buf.yaml" 2>/dev/null; then
    echo "notice: proto/ does not exist at $base; skipping buf breaking"
    return 0
  fi
  against=$(mktemp -d)
  git archive "$base" proto | tar -x -C "$against" || status=$?
  if [[ $status -eq 0 ]]; then buf breaking proto --against "$against/proto" || status=$?; fi
  rm -rf "$against"
  return "$status"
}

tests() {
  cargo nextest run --workspace --locked && cargo test --workspace --doc --locked
}

gate GATE-FMT "formatting" cargo fmt --all --check
gate GATE-CLIPPY "clippy pedantic + nursery, warnings are errors" \
  cargo clippy --workspace --all-targets --locked -- -D warnings
gate GATE-ARCH "architecture rules" cargo nextest run -p vc-arch-tests --locked
gate GATE-DENY "licences, advisories, bans, sources" cargo deny check
gate GATE-JSLICENSE "licences of production JS dependencies (deny.toml allow list)" js_licenses
gate GATE-PROTO "buf lint, and no breaking change in proto/" proto_contract
gate GATE-MACHETE "unused dependencies" cargo machete
gate GATE-DUP "duplicated blocks of 10+ lines" duplication
gate GATE-TEST "tests and doctests" tests
if [[ "${GATES_MUTANTS:-0}" == "1" ]]; then
  gate GATE-MUTANTS "mutation score on the diff" scripts/mutants.sh diff "${GATES_BASE:-origin/master}"
fi

printf '\nall gates passed\n'
