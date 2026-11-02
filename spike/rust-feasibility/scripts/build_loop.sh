#!/usr/bin/env bash
# Part D build-loop timings on the Part B workspace. Medians of 3 computed afterwards.
# Usage: build_loop.sh <workspace> <out.csv>
set -u
WS=$1; OUT=$2
export CARGO_BUILD_JOBS=2
cd "$WS"
EDIT=crates/engine/src/scheduler.rs
cp "$EDIT" /tmp/claude-0/scheduler.rs.orig
echo "step,run,seconds,exit" > "$OUT"

timeit() { # name run cmd...
  local name=$1 run=$2; shift 2
  local s e rc
  s=$(date +%s.%N); "$@" > /tmp/claude-0/bl-$name-$run.log 2>&1; rc=$?; e=$(date +%s.%N)
  echo "$name,$run,$(echo "$e - $s" | bc),$rc" >> "$OUT"
}

edit_engine() { # one real code change in engine per call
  cp /tmp/claude-0/scheduler.rs.orig "$EDIT"
  echo "const _BUILD_LOOP_EDIT: u64 = $(date +%s%N);" >> "$EDIT"
}

for run in 1 2 3; do
  T=/tmp/claude-0/pd-cold-$run
  rm -rf "$T"; timeit cold_debug_build $run cargo build --workspace --target-dir "$T"
  rm -rf "$T"; timeit cold_release_build $run cargo build --workspace --release --target-dir "$T"
  rm -rf "$T"
done

# warm the shared target dir for every profile/target kind used below
cargo check --workspace --all-targets >/dev/null 2>&1
cargo build --workspace >/dev/null 2>&1
cargo test --workspace --no-run >/dev/null 2>&1
cargo build --workspace --release >/dev/null 2>&1

for run in 1 2 3; do
  edit_engine; timeit incr_check $run cargo check --workspace
  edit_engine; timeit incr_debug_build $run cargo build --workspace
  edit_engine; timeit incr_test_build_and_run $run cargo test --workspace
  edit_engine; timeit incr_test_engine_only $run cargo test -p engine
  edit_engine; timeit incr_release_build $run cargo build --workspace --release
done
cp /tmp/claude-0/scheduler.rs.orig "$EDIT"
cargo build --workspace >/dev/null 2>&1
echo done >> "$OUT.done"
