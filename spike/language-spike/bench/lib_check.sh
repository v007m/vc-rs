#!/usr/bin/env bash
# Registry metadata for the library-support criterion. Output: results/libraries.json
# npm: `npm view` (latest version + time.modified). crates: crates.io API.
# Go: proxy.golang.org is blocked in this sandbox, so the latest tag is fetched
# from GitHub via git and its commit date read with a depth-1 fetch.
set -u
OUT=${1:-$(dirname "$0")/../results/libraries.json}
TMP=$(mktemp -d)
echo "{" > "$OUT"

echo '"npm": {' >> "$OUT"
first=1
for p in yaml js-yaml zod ajv @marcbachmann/cel-js cel-js @bufbuild/cel @bufbuild/protobuf @connectrpc/connect @grpc/grpc-js \
  @modelcontextprotocol/sdk ink @opentelemetry/sdk-node oauth4webapi better-sqlite3 commander citty vscode-jsonrpc json-rpc-2.0 undici; do
  j=$(npm view "$p" version time.modified --json 2>/dev/null | tr -d '\n')
  [ $first = 1 ] || echo "," >> "$OUT"; first=0
  printf '"%s": %s' "$p" "${j:-null}" >> "$OUT"
done
echo '},' >> "$OUT"

echo '"crates": {' >> "$OUT"
first=1
for c in serde-saphyr saphyr serde_yaml garde jsonschema cel cel-interpreter prost tonic connectrpc rmcp ratatui opentelemetry oauth2 rusqlite clap jsonrpsee reqwest tokio; do
  j=$(curl -s -A "vibranium-spike" "https://crates.io/api/v1/crates/$c" | python3 -c "import json,sys
d=json.load(sys.stdin)['crate'];print(json.dumps({'version':d['max_stable_version'] or d['max_version'],'updated':d['updated_at'][:10],'recent_downloads':d['recent_downloads']}))" 2>/dev/null)
  [ $first = 1 ] || echo "," >> "$OUT"; first=0
  printf '"%s": %s' "$c" "${j:-null}" >> "$OUT"
done
echo '},' >> "$OUT"

echo '"go_github": {' >> "$OUT"
first=1
for r in google/cel-go goccy/go-yaml yaml/go-yaml go-playground/validator santhosh-tekuri/jsonschema protocolbuffers/protobuf-go \
  connectrpc/connect-go grpc/grpc-go modelcontextprotocol/go-sdk mark3labs/mcp-go charmbracelet/bubbletea open-telemetry/opentelemetry-go \
  golang/oauth2 mattn/go-sqlite3 ncruces/go-sqlite3 spf13/cobra alecthomas/kong sourcegraph/jsonrpc2 hashicorp/go-plugin; do
  tag=$(git ls-remote --tags --refs --sort=-v:refname "https://github.com/$r" 2>/dev/null | sed 's#.*refs/tags/##' | grep -E '^(v[0-9]+\.[0-9]+\.[0-9]+|[0-9]+\.[0-9]+\.[0-9]+)$' | head -1)
  date=""
  if [ -n "$tag" ]; then
    d="$TMP/$(echo "$r" | tr / _)"
    git init -q --bare "$d" && git -C "$d" fetch -q --depth 1 "https://github.com/$r" "refs/tags/$tag:refs/tags/$tag" 2>/dev/null \
      && date=$(git -C "$d" log -1 --format=%cs "$tag" 2>/dev/null)
  fi
  [ $first = 1 ] || echo "," >> "$OUT"; first=0
  printf '"%s": {"latest_tag": "%s", "tag_commit_date": "%s"}' "$r" "$tag" "$date" >> "$OUT"
done
echo '}' >> "$OUT"
echo "}" >> "$OUT"
rm -rf "$TMP"
python3 -m json.tool "$OUT" > /dev/null && echo "wrote $OUT"
