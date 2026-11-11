#!/usr/bin/env node
// GATE-JSLICENSE: reads `pnpm licenses list --prod --json --recursive` on stdin and fails when a
// production JS dependency has a licence outside deny.toml's `[licenses] allow` list, so Rust and
// JS share one allow-list. SPDX expressions follow cargo-deny: `A OR B` needs one side allowed,
// `A AND B` both, and `A WITH exception` must be listed as a whole.
// Usage: js-licenses.mjs <deny.toml>
import { readFileSync } from 'node:fs';

// Drops each line's `# …` comment (a `#` outside a quoted string), so a quoted licence named in a
// comment is not read as allowed.
function withoutComments(toml) {
  return toml.replace(/^((?:[^"#\n]|"[^"\n]*")*)#.*$/gm, '$1');
}

function allowList(denyToml) {
  const section = withoutComments(denyToml).split(/^\[licenses\]\s*$/m)[1]?.split(/^\[/m)[0] ?? '';
  const list = section.match(/^allow\s*=\s*\[([^\]]*)\]/m);
  if (!list) throw new Error('deny.toml has no `allow` list under [licenses]');
  return new Set([...list[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]));
}

function tokens(expression) {
  return expression.replace(/[()]/g, ' $& ').trim().split(/\s+/);
}

// Recursive descent over: or := and ("OR" and)*; and := atom ("AND" atom)*;
// atom := "(" or ")" | id ["WITH" id].
function allowed(expression, allow) {
  const list = tokens(expression);
  let at = 0;
  const atom = () => {
    if (list[at] === '(') {
      at += 1;
      const inner = or();
      at += 1;
      return inner;
    }
    let id = list[at++] ?? '';
    if (list[at] === 'WITH') {
      id = `${id} WITH ${list[at + 1]}`;
      at += 2;
    }
    return allow.has(id);
  };
  const and = () => {
    let ok = atom();
    while (list[at] === 'AND') {
      at += 1;
      ok = atom() && ok;
    }
    return ok;
  };
  const or = () => {
    let ok = and();
    while (list[at] === 'OR') {
      at += 1;
      ok = and() || ok;
    }
    return ok;
  };
  return or() && at === list.length;
}

const allow = allowList(readFileSync(process.argv[2] ?? 'deny.toml', 'utf8'));
const report = JSON.parse(readFileSync(0, 'utf8') || '{}');
const denied = Object.entries(report).filter(([licence]) => !allowed(licence, allow));
for (const [licence, packages] of denied) {
  const names = packages.map((p) => `${p.name}@${p.versions.join(',')}`).join(', ');
  console.error(`licence "${licence}" is not in deny.toml's allow list: ${names}`);
}
const count = Object.values(report).flat().length;
console.log(`${count} production JS packages checked against ${allow.size} allowed licences`);
process.exit(denied.length === 0 ? 0 : 1);
