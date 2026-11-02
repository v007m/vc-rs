#!/usr/bin/env bash
# Mutation testing for the crates with a score floor (D119): fails when fewer than 85 % of the
# viable mutants are caught. Usage: mutants.sh diff <base-ref> | mutants.sh full
set -euo pipefail
cd "$(dirname "$0")/.."

readonly MIN_PERCENT=85
readonly CRATES="executor planner template compiler"
mode=${1:-}
base=${2:-origin/master}

packages=()
diff_file=""
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT

case "$mode" in
  full)
    for crate in $CRATES; do packages+=(-p "vc-$crate"); done
    ;;
  diff)
    merge_base=$(git merge-base "$base" HEAD)
    for crate in $CRATES; do
      if ! git diff --quiet "$merge_base" -- "crates/$crate"; then packages+=(-p "vc-$crate"); fi
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
  echo "no changes in: $CRATES; skipping"
  exit 0
fi

args=("${packages[@]}" --output "$out" --test-tool nextest)
if [[ -n "$diff_file" ]]; then args+=(--in-diff "$diff_file"); fi

status=0
cargo mutants "${args[@]}" || status=$?
# 0: all caught, 2: some missed, 3: some timed out; anything else is a setup failure.
if [[ $status -ne 0 && $status -ne 2 && $status -ne 3 ]]; then exit "$status"; fi

count() {
  local file="$out/mutants.out/$1"
  if [[ -s "$file" ]]; then grep -c . "$file"; else echo 0; fi
}
caught=$(count caught.txt)
viable=$((caught + $(count missed.txt) + $(count timeout.txt)))
if [[ $viable -eq 0 ]]; then
  echo "no viable mutants"
  exit 0
fi
percent=$((caught * 100 / viable))
echo "mutation score: $caught/$viable caught ($percent %), floor $MIN_PERCENT %"
[[ $percent -ge $MIN_PERCENT ]]
