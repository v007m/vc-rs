#!/usr/bin/env bash
# Commit gates (D116) for every non-merge commit in <base-ref>..HEAD (default origin/master):
#   GATE-COMMIT    the subject is not a conventional commit
#   GATE-REFACTOR  a refactor: commit changes a test file or a line containing an assertion
set -euo pipefail
cd "$(dirname "$0")/.."

base=${1:-origin/master}
readonly SUBJECT_RE='^(feat|fix|refactor|test|docs|chore|perf|build|ci)(\([^()]+\))?!?: [^ ]'
readonly REFACTOR_RE='^refactor(\([^()]+\))?!?:'
readonly TEST_FILE_RE='(^|/)tests/|_test\.rs$|(^|/)tests\.rs$'
failed=0

report() {
  printf 'FAILED %s: %s\n' "$1" "$2" >&2
  failed=1
}

touches_assertions() {
  local files changed
  files=$(git show --format= --name-only "$1")
  if grep -Eq "$TEST_FILE_RE" <<<"$files"; then return 0; fi
  changed=$(git show --format= --unified=0 "$1" -- '*.rs' | grep -E '^[-+]' | grep -Ev '^(\+\+\+|---) ' || true)
  [[ $changed == *assert* ]]
}

for sha in $(git rev-list --no-merges --reverse "$base..HEAD"); do
  subject=$(git log -1 --format=%s "$sha")
  short=$(git rev-parse --short "$sha")
  if ! [[ $subject =~ $SUBJECT_RE ]]; then
    report GATE-COMMIT "$short \"$subject\" is not a conventional commit (type(scope)!: subject)"
  fi
  if [[ $subject =~ $REFACTOR_RE ]] && touches_assertions "$sha"; then
    report GATE-REFACTOR "$short \"$subject\" changes test assertions; move that change to a test: or feat: commit"
  fi
done

if [[ $failed -ne 0 ]]; then exit 1; fi
echo "commit checks passed for $base..HEAD"
