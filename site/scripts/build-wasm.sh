#!/usr/bin/env bash
# Builds the playground's runner (crates/browser, D293) for wasm32-wasip1 with the size-tuned
# `wasm` profile and puts it where the site serves it: public/playground/vc-playground.wasm
# (gitignored). The page loads it only when Run or Check is first pressed.
set -euo pipefail
cd "$(dirname "$0")/../.."

target=wasm32-wasip1
rustup target add "$target" >/dev/null
CARGO_INCREMENTAL=0 cargo build --locked -p vc-browser --bin vc-playground --target "$target" --profile wasm
built="target/$target/wasm/vc-playground.wasm"
mkdir -p site/public/playground
cp "$built" site/public/playground/vc-playground.wasm
printf 'vc-playground.wasm: %s bytes, %s gzipped\n' "$(wc -c <"$built" | tr -d ' ')" "$(gzip -9 -c "$built" | wc -c | tr -d ' ')"
