#!/bin/sh
# Holds the criterion medians to the wall-clock budgets (D120). Run after `mise run bench`.
# Exits non-zero when a budget is breached or a result is missing; nightly.yml opens an issue then.
set -eu

dir="${CRITERION_DIR:-target/criterion}"
failed=0

median_ms() {
  file="$dir/$1/new/estimates.json"
  if [ ! -f "$file" ]; then
    return 1
  fi
  jq -r '.median.point_estimate / 1000000' "$file"
}

# check <bench> <budget in ms>; a budget of 0 only reports the time.
check() {
  if ! ms="$(median_ms "$1")"; then
    echo "MISSING $1: no criterion result in $dir/$1"
    failed=1
    return
  fi
  if [ "$2" = 0 ]; then
    printf 'INFO    %-22s %10.2f ms\n' "$1" "$ms"
  elif awk -v ms="$ms" -v budget="$2" 'BEGIN { exit !(ms < budget) }'; then
    printf 'OK      %-22s %10.2f ms  (budget %s ms)\n' "$1" "$ms" "$2"
  else
    printf 'BREACH  %-22s %10.2f ms  (budget %s ms)\n' "$1" "$ms" "$2"
    failed=1
  fi
}

check compile/cold_1000 500
check compile/warm_1000 100
check compile/changed_1000 0
check startup/version 20
check report/html_10k 1000

exit "$failed"
