#!/usr/bin/env bash
# Mutation testing with a score floor (D119): fails when fewer than 85 % of the viable mutants are
# caught. The full run covers the floor crates; the diff run also covers the server, the contracts
# and every adapter. Usage: mutants.sh diff <base-ref> | mutants.sh full
set -euo pipefail
cd "$(dirname "$0")/.."

readonly MIN_PERCENT=85
readonly CRATES="executor planner template compiler"
# <folder under crates/>:<package> pairs that only the diff run covers; adapters are added below.
readonly DIFF_ONLY="apps/server:vc-server contracts:vc-contracts"
# Generated code is never mutated.
readonly EXCLUDES=(--exclude 'crates/contracts/src/gen/**')
mode=${1:-}
base=${2:-origin/master}

packages=()
diff_file=""
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT

diff_crates() {
  local crate dir
  for crate in $CRATES; do echo "$crate:vc-$crate"; done
  for crate in $DIFF_ONLY; do echo "$crate"; done
  for dir in crates/adapters/*/; do
    dir=${dir%/}
    echo "${dir#crates/}:vc-adapter-${dir##*/}"
  done
}

case "$mode" in
  full)
    for crate in $CRATES; do packages+=(-p "vc-$crate"); done
    ;;
  diff)
    merge_base=$(git merge-base "$base" HEAD)
    for pair in $(diff_crates); do
      if ! git diff --quiet "$merge_base" -- "crates/${pair%%:*}"; then packages+=(-p "${pair#*:}"); fi
    done
    diff_file="$out/changes.diff"
    git diff "$merge_base" >"$diff_file"
    ;;
  *)
    echo "usage: $0 diff <base-ref> | full" >&2
    exit 64
    ;;
esac

if [[ ${#packages[@]} -eq 0 ]]; then
  echo "no changes in the mutation-tested crates; skipping"
  exit 0
fi

args=("${packages[@]}" "${EXCLUDES[@]}" --output "$out" --test-tool nextest)
if [[ -n "$diff_file" ]]; then args+=(--in-diff "$diff_file"); fi

status=0
cargo mutants "${args[@]}" || status=$?
# 0: all caught, 2: some missed, 3: some timed out; anything else is a setup failure.
if [[ $status -ne 0 && $status -ne 2 && $status -ne 3 ]]; then exit "$status"; fi

count() {
  local file="$out/mutants.out/$1"
  if [[ -s "$file" ]]; then grep -c . "$file"; else echo 0; fi
}
# A timed-out mutant was detected (the tests did not finish), so it counts as caught.
caught=$(($(count caught.txt) + $(count timeout.txt)))
viable=$((caught + $(count missed.txt)))
if [[ $viable -eq 0 ]]; then
  echo "no viable mutants"
  exit 0
fi
percent=$((caught * 100 / viable))
echo "mutation score: $caught/$viable caught ($percent %), floor $MIN_PERCENT %"
[[ $percent -ge $MIN_PERCENT ]]
