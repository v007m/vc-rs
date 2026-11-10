#!/usr/bin/env bash
# Gate self-test (D121): applies each tests/gate-fixtures/*.patch to a scratch worktree of HEAD and
# asserts that exactly the gate named on its first line rejects it. First line format:
#   # expect: <GATE-ID> [<more text the output must contain>...]
# GATE-COMMIT / GATE-REFACTOR fixtures are mailbox patches (`git format-patch`), applied with
# `git am` (one or more commits) and checked by commit-check.sh. GATE-MUTANTS fixtures run gates.sh with GATES_MUTANTS=1.
# All others are plain diffs checked by gates.sh. Arch fixtures name the arch-tests rule ID too,
# e.g. "# expect: GATE-ARCH ARCH-LAYER". Fixtures add new files where possible so they keep applying.
# Tests the committed HEAD, so commit gate changes before running it.
set -euo pipefail
cd "$(dirname "$0")/.."

repo=$(pwd)
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo/target}"
scratch=$(mktemp -d)
wt="$scratch/wt"
# check_fixture runs as an `if` condition, where set -e is off, so setup steps return this explicitly.
readonly SETUP_FAILED=99

drop_worktree() {
  git worktree remove --force "$wt" >/dev/null 2>&1 || true
  git worktree prune
}
# The scratch worktree shares the target dir, so vc-arch-tests is left built with the scratch
# CARGO_MANIFEST_DIR; cleaning it makes the next gates run rebuild it against this checkout.
forget_arch_tests() {
  cargo clean -p vc-arch-tests >/dev/null 2>&1 || true
}
trap 'drop_worktree; rm -rf "$scratch"; forget_arch_tests' EXIT

refresh_lockfile() {
  (cd "$wt" && { cargo metadata --format-version 1 --offline >/dev/null 2>&1 ||
    cargo metadata --format-version 1 >/dev/null; })
}

check_fixture() {
  local body=$1 gate=$2
  case "$gate" in
    GATE-COMMIT | GATE-REFACTOR)
      local base
      base=$(git -C "$wt" rev-parse HEAD)
      git -C "$wt" -c user.name=gates-selftest -c user.email=gates-selftest@example.invalid \
        am --quiet "$body" || return "$SETUP_FAILED"
      "$wt/scripts/commit-check.sh" "$base"
      ;;
    *)
      git -C "$wt" apply --index "$body" || return "$SETUP_FAILED"
      refresh_lockfile || return "$SETUP_FAILED"
      if [[ "$gate" == GATE-MUTANTS ]]; then
        GATES_MUTANTS=1 GATES_BASE=HEAD "$wt/scripts/gates.sh"
      else
        "$wt/scripts/gates.sh"
      fi
      ;;
  esac
}

verdict() {
  local log=$1 gate=$2 token failed
  shift 2
  failed=$(grep -o 'FAILED GATE-[A-Z]*' "$log" | sort -u || true)
  [[ "$failed" == "FAILED $gate" ]] || return 1
  for token in "$@"; do grep -qF -- "$token" "$log" || return 1; done
}

failures=0
total=0
for patch in tests/gate-fixtures/*.patch; do
  name=$(basename "$patch")
  read -r -a expect <<<"$(head -n 1 "$patch" | sed -n 's/^# expect: //p')"
  if [[ ${#expect[@]} -eq 0 ]]; then
    echo "FAIL $name: first line must be '# expect: <GATE-ID> ...'"
    failures=$((failures + 1))
    continue
  fi
  total=$((total + 1))
  log="$scratch/$name.log"
  tail -n +2 "$patch" >"$scratch/body.patch"
  git worktree add --detach --quiet "$wt" HEAD
  if [[ -d "$repo/node_modules" ]]; then ln -s "$repo/node_modules" "$wt/node_modules"; fi
  printf '%s ... ' "$name"
  status=0
  check_fixture "$scratch/body.patch" "${expect[0]}" >"$log" 2>&1 || status=$?
  if [[ $status -eq 0 ]]; then
    echo "FAIL: every gate passed, expected ${expect[0]}"
    failures=$((failures + 1))
  elif [[ $status -eq $SETUP_FAILED ]]; then
    echo "FAIL: the fixture did not apply to HEAD; last lines of the output:"
    tail -n 25 "$log" | sed 's/^/    /'
    failures=$((failures + 1))
  elif verdict "$log" "${expect[@]}"; then
    echo "ok, rejected by ${expect[0]}"
  else
    echo "FAIL: expected ${expect[*]}; last lines of the output:"
    tail -n 25 "$log" | sed 's/^/    /'
    failures=$((failures + 1))
  fi
  drop_worktree
done

echo
echo "$((total - failures))/$total fixtures rejected by their expected gate"
[[ $failures -eq 0 && $total -gt 0 ]]
