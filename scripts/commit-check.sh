#!/usr/bin/env bash
# Commit gates (D116) for every non-merge commit in <base-ref>..HEAD (default origin/master):
#   GATE-COMMIT    the subject is not a conventional commit
#   GATE-REFACTOR  a refactor: commit changes a test file or a diff hunk that holds an assertion
set -euo pipefail
cd "$(dirname "$0")/.."

base=${1:-origin/master}
if [[ $base == *..* ]]; then
  printf 'commit-check: pass a base ref such as origin/master, not a range (%s)\n' "$base" >&2
  exit 2
fi
readonly SUBJECT_RE='^(feat|fix|refactor|test|docs|chore|perf|build|ci)(\([^()]+\))?!?: [^ ]'
readonly REFACTOR_RE='^refactor(\([^()]+\))?!?:'
readonly TEST_FILE_RE='_tests?\.rs$|(^|/)tests\.rs$|\.(test|spec)\.[cm]?[jt]sx?$|(^|/)e2e/|(^|/)snapshots/[^/]+\.snap$'
readonly TEST_DIR_RE='(^|/)tests/'
# The web UI's tests/ folder under src/ is source, not tests: it holds the Tests page's components.
readonly SOURCE_DIR_RE='^web/src/'
failed=0

report() {
  printf 'FAILED %s: %s\n' "$1" "$2" >&2
  failed=1
}

touches_assertions() {
  local files
  files=$(git show --format= --name-only "$1")
  if grep -Eq "$TEST_FILE_RE" <<<"$files"; then return 0; fi
  # One awk reads every name, so no early exit in a pipe can SIGPIPE a reader under pipefail.
  if awk -v dir="$TEST_DIR_RE" -v src="$SOURCE_DIR_RE" '$0 ~ dir && $0 !~ src { found = 1 } END { exit !found }' \
    <<<"$files"; then return 0; fi
  # A hunk counts when it changes a line and mentions an assertion anywhere, context included, so
  # editing the expected value on its own line of a multi-line assert_eq! is caught as well.
  git show --format= --unified=3 "$1" -- '*.rs' | awk '
    /^@@/ { if (changed && asserts) found = 1; changed = 0; asserts = 0; next }
    /^(\+\+\+|---) / { next }
    /^[-+]/ { changed = 1 }
    /assert/ { asserts = 1 }
    END { if (changed && asserts) found = 1; exit !found }'
}

# A plain assignment, so a bad base fails under set -e instead of looping over nothing.
shas=$(git rev-list --no-merges --reverse "$base..HEAD")
for sha in $shas; do
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
