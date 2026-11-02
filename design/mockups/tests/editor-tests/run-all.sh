#!/bin/sh
# Rebuild the artboard from editor-src and run every suite, with and without js-yaml.
set -e
cd "$(dirname "$0")"
node ../editor-src/build.js
node core.test.js && node ui.test.js && node sweep.js | tail -1 && node static-holes.js | tail -1
NOJSYAML=1 node core.test.js && NOJSYAML=1 node ui.test.js
