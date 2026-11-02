// Checks a .dc.html artboard: every {{hole}} resolves against renderVals() (loop scopes included),
// event holes are functions, tags balance, no sc-for inside table/select/svg, and handlers run without throwing.
const fs = require('fs');
const file = process.argv[2];
const src = fs.readFileSync(file, 'utf8');
const tpl = src.slice(src.indexOf('<x-dc>'), src.indexOf('</x-dc>'));
const m = src.match(/<script type="text\/x-dc" data-dc-script data-props='([^']*)'>([\s\S]*?)<\/script>/);
if (!m) { console.log('NO SCRIPT BLOCK'); process.exit(1); }
const props = JSON.parse(m[1].replace(/&#39;/g, "'").replace(/&amp;/g, '&'));
const code = m[2];
class DCLogic {
  constructor() { this.props = {}; this.state = null; }
  setState(o) { const n = typeof o === 'function' ? o(this.state) : o; this.state = Object.assign({}, this.state, n); }
  forceUpdate() {}
}
const Component = new Function('DCLogic', code + '\nreturn Component;')(DCLogic);
const problems = [];
const init = process.argv[3] ? JSON.parse(process.argv[3]) : null;
const mk = () => { const c = new Component(); c.props = {}; if (init) c.state = Object.assign({}, init); return c; };
let inst = mk();
let vals = inst.renderVals();

function resolve(obj, path) {
  let cur = obj;
  for (const p of path.split('.')) { if (cur == null) return undefined; cur = cur[p]; }
  return cur;
}
// walk template in order
const re = /<sc-for\b[^>]*>|<\/sc-for>|\{\{\s*([^}]+?)\s*\}\}/g;
const stack = [];
let mm; let holes = 0;
function lookup(path) {
  const root = path.split('.')[0];
  for (let i = stack.length - 1; i >= 0; i--) {
    if (stack[i].as === root) {
      const rest = path.split('.').slice(1).join('.');
      const items = stack[i].items || [];
      if (!items.length) return { unverified: true };
      if (!rest) return { ok: true };
      const hit = items.some((it) => resolve(it, rest) !== undefined);
      return { ok: hit };
    }
  }
  if (root === '$index') return { ok: true };
  if (/^(true|false|null|-?\d+(\.\d+)?)$/.test(path)) return { ok: true };
  return { ok: resolve(vals, path) !== undefined, val: resolve(vals, path) };
}
while ((mm = re.exec(tpl))) {
  const t = mm[0];
  if (t.startsWith('<sc-for')) {
    const list = (t.match(/list="\{\{\s*([^}]+?)\s*\}\}"/) || [])[1];
    const as = (t.match(/as="([^"]+)"/) || [])[1];
    // resolve list in current scope
    let items = [];
    const root = list.split('.')[0];
    let found = false;
    for (let i = stack.length - 1; i >= 0; i--) {
      if (stack[i].as === root) {
        found = true;
        const rest = list.split('.').slice(1).join('.');
        for (const it of stack[i].items || []) { const v = resolve(it, rest); if (Array.isArray(v)) items = items.concat(v); }
        break;
      }
    }
    if (!found) { const v = resolve(vals, list); if (!Array.isArray(v)) problems.push(`sc-for list not an array: ${list}`); else items = v; }
    stack.push({ as, items });
    holes++;
  } else if (t === '</sc-for>') {
    stack.pop();
  } else {
    holes++;
    const path = mm[1];
    const r = lookup(path);
    if (r.unverified) problems.push(`unverified (empty list) hole: ${path}`);
    else if (!r.ok) problems.push(`UNRESOLVED hole: {{${path}}}`);
  }
}
// event attributes must be functions
const evRe = /\son[A-Z][a-zA-Z]*="\{\{\s*([^}]+?)\s*\}\}"/g;
let ev; let evs = 0;
while ((ev = evRe.exec(tpl))) {
  evs++;
  const p = ev[1]; const root = p.split('.')[0];
  if (resolve(vals, p) !== undefined && typeof resolve(vals, p) !== 'function') problems.push(`event hole not a function: ${p}`);
}
// tag balance
const voids = new Set(['area','base','br','col','embed','hr','img','input','link','meta','source','track','wbr']);
const tagRe = /<\/?([a-zA-Z][a-zA-Z0-9-]*)\b[^>]*?(\/?)>/g;
const ts = []; let tm;
const body = tpl.replace(/<!--[\s\S]*?-->/g, '');
while ((tm = tagRe.exec(body))) {
  const name = tm[1].toLowerCase(); const closing = tm[0].startsWith('</'); const self = tm[2] === '/';
  if (voids.has(name) || self) continue;
  if (!closing) ts.push({ name, pos: tm.index });
  else {
    const top = ts.pop();
    if (!top || top.name !== name) { problems.push(`tag mismatch: </${name}> closes <${top && top.name}> near ${body.slice(Math.max(0, tm.index - 80), tm.index + 20).replace(/\s+/g, ' ')}`); break; }
  }
}
if (ts.length > 1) problems.push(`unclosed tags: ${ts.map((t) => t.name).join(',')}`);
// sc-for inside table/select/svg
for (const ctx of ['table', 'select', 'svg']) {
  const r = new RegExp(`<${ctx}\\b[\\s\\S]*?</${ctx}>`, 'g'); let c;
  while ((c = r.exec(body))) if (/<sc-(for|if)\b/.test(c[0])) problems.push(`sc-for/sc-if inside <${ctx}>`);
}
// unquoted attributes
const uq = body.match(/<[a-zA-Z][^>]*\s[a-zA-Z-]+=[^"'\s>{][^>]*>/g);
if (uq) problems.push('unquoted attribute: ' + uq[0].slice(0, 80));
// emoji
if (/[\u{1F300}-\u{1FAFF}\u{2600}-\u{27BF}]/u.test(src)) problems.push('emoji-range character present');
// smoke: call every handler (top level and first loop item level) and re-render
let calls = 0;
function smoke(getFns) {
  const c = mk(); let v = c.renderVals();
  const fns = getFns(v);
  for (const [name, fn] of fns) {
    const c2 = mk(); c2.state = c.state ? { ...c.state } : null;
    const v2 = c2.renderVals();
    const f = getFns(v2).find((x) => x[0] === name);
    if (!f) continue;
    try { f[1]({ target: { value: 'status:failed tag:smoke', checked: true }, preventDefault() {}, stopPropagation() {} }); c2.renderVals(); calls++; }
    catch (e) { problems.push(`handler ${name} threw: ${e.message}`); }
  }
}
smoke((v) => Object.entries(v).filter(([, f]) => typeof f === 'function'));
smoke((v) => {
  const out = [];
  for (const [k, arr] of Object.entries(v)) if (Array.isArray(arr)) arr.slice(0, 3).forEach((it, i) => { if (it && typeof it === 'object') for (const [kk, f] of Object.entries(it)) if (typeof f === 'function') out.push([`${k}[${i}].${kk}`, f]); });
  return out;
});
console.log(`${file.split('/').pop()}: ${holes} holes, ${evs} event bindings, ${calls} handlers smoke-run, preview ${JSON.stringify(props.$preview)}`);
if (problems.length) { console.log(problems.map((p) => '  - ' + p).join('\n')); process.exitCode = 1; } else console.log('  OK: every hole resolves');
