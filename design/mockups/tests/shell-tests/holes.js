// Mini renderer for the .dc.html template: walks sc-if / sc-for with real renderVals() output
// and reports every {{hole}} that resolves to undefined (or an object in a text slot,
// or a non-function in an event slot).
const VOID = new Set(['area', 'base', 'br', 'col', 'embed', 'hr', 'img', 'input', 'link', 'meta', 'source', 'track', 'wbr']);

function parse(html) {
  const root = { tag: '#root', attrs: {}, children: [] };
  const stack = [root];
  const re = /<!--[\s\S]*?-->|<\/([a-zA-Z][\w-]*)\s*>|<([a-zA-Z][\w-]*)((?:\s+[^\s=>\/]+(?:\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+))?)*)\s*(\/?)>|([^<]+)/g;
  let m;
  const errors = [];
  while ((m = re.exec(html))) {
    if (m[0].startsWith('<!--')) continue;
    const top = stack[stack.length - 1];
    if (m[1]) {
      const name = m[1].toLowerCase();
      if (top.tag !== name) { errors.push('mismatched </' + name + '> while in <' + top.tag + '> near ' + html.slice(Math.max(0, m.index - 80), m.index + 20).replace(/\s+/g, ' ')); }
      // pop to matching
      for (let i = stack.length - 1; i > 0; i--) { if (stack[i].tag === name) { stack.length = i; break; } }
    } else if (m[2]) {
      const name = m[2].toLowerCase();
      const attrs = {};
      const ar = /([^\s=>\/]+)(?:\s*=\s*("([^"]*)"|'([^']*)'|([^\s>]+)))?/g;
      let a;
      while ((a = ar.exec(m[3] || ''))) attrs[a[1]] = a[3] !== undefined ? a[3] : a[4] !== undefined ? a[4] : a[5] !== undefined ? a[5] : true;
      const node = { tag: name, attrs, children: [] };
      top.children.push(node);
      if (!VOID.has(name) && !m[4]) stack.push(node);
    } else if (m[5]) {
      top.children.push({ text: m[5] });
    }
  }
  if (stack.length !== 1) errors.push('unclosed: ' + stack.slice(1).map((n) => n.tag).join(' > '));
  return { root, errors };
}

const HOLE = /\{\{\s*([^}]*?)\s*\}\}/g;
function lookup(path, scope, vals) {
  if (path === 'true') return true;
  if (path === 'false') return false;
  if (/^-?\d+(\.\d+)?$/.test(path)) return Number(path);
  if (/^'.*'$/.test(path)) return path.slice(1, -1);
  const parts = path.split('.');
  let cur;
  if (Object.prototype.hasOwnProperty.call(scope, parts[0])) cur = scope[parts[0]];
  else cur = vals[parts[0]];
  for (let i = 1; i < parts.length; i++) { if (cur === null || cur === undefined) return undefined; cur = cur[parts[i]]; }
  return cur;
}

function check(tree, vals, label, report) {
  const seen = report.seen;
  function visitAttrs(node, scope) {
    for (const [k, v] of Object.entries(node.attrs)) {
      if (typeof v !== 'string' || v.indexOf('{{') < 0) continue;
      if (k.startsWith('hint-')) continue;
      const whole = /^\{\{\s*([^}]*?)\s*\}\}$/.exec(v);
      const holes = [...v.matchAll(HOLE)].map((x) => x[1]);
      for (const h of holes) {
        const r = lookup(h, scope, vals);
        const key = node.tag + '@' + k + ' ' + h;
        seen.add(key);
        if (r === undefined) report.err.push(label + ': ' + key + ' is undefined');
        else if (/^on[A-Z]/.test(k) && typeof r !== 'function') report.err.push(label + ': ' + key + ' is not a function');
        else if (!whole && typeof r === 'object' && r !== null) report.err.push(label + ': ' + key + ' interpolates an object');
        else if (k === 'value' && node.tag !== 'sc-if' && typeof r !== 'string' && typeof r !== 'number') report.err.push(label + ': ' + key + ' value is ' + typeof r);
        else if ((k === 'checked' || k === 'disabled') && typeof r !== 'boolean') report.err.push(label + ': ' + key + ' should be boolean, is ' + typeof r);
        else report.ok.add(key);
      }
    }
  }
  function walk(node, scope) {
    if (node.text !== undefined) {
      for (const x of node.text.matchAll(HOLE)) {
        const h = x[1];
        const r = lookup(h, scope, vals);
        const key = 'text ' + h;
        seen.add(key);
        if (r === undefined) report.err.push(label + ': ' + key + ' is undefined');
        else if (typeof r === 'object' && r !== null) report.err.push(label + ': ' + key + ' renders an object');
        else if (typeof r === 'function') report.err.push(label + ': ' + key + ' renders a function');
        else report.ok.add(key);
      }
      return;
    }
    if (node.tag === 'sc-if') {
      const h = /^\{\{\s*([^}]*?)\s*\}\}$/.exec(node.attrs.value || '');
      if (!h) { report.err.push(label + ': sc-if without hole'); return; }
      const r = lookup(h[1], scope, vals);
      const key = 'sc-if ' + h[1];
      seen.add(key);
      if (r === undefined) { report.err.push(label + ': ' + key + ' is undefined'); return; }
      if (typeof r !== 'boolean') report.err.push(label + ': ' + key + ' is ' + typeof r + ' not boolean');
      report.ok.add(key);
      if (r) report.ifTrue.add(key); else report.ifFalse.add(key);
      if (!r) return;
      node.children.forEach((c) => walk(c, scope));
      return;
    }
    if (node.tag === 'sc-for') {
      const h = /^\{\{\s*([^}]*?)\s*\}\}$/.exec(node.attrs.list || '');
      const as = node.attrs.as;
      if (!h || !as) { report.err.push(label + ': bad sc-for'); return; }
      const list = lookup(h[1], scope, vals);
      const key = 'sc-for ' + h[1];
      seen.add(key);
      if (!Array.isArray(list)) { report.err.push(label + ': ' + key + ' is not an array'); return; }
      report.ok.add(key);
      if (list.length) report.forNonEmpty.add(key);
      list.forEach((item, i) => { const sc = Object.assign({}, scope, { [as]: item, $index: i }); node.children.forEach((c) => walk(c, sc)); });
      return;
    }
    visitAttrs(node, scope);
    node.children.forEach((c) => walk(c, scope));
  }
  walk(tree, {});
}

function allHoleKeys(tree) {
  const keys = new Set();
  (function walk(node) {
    if (node.text !== undefined) { for (const x of node.text.matchAll(HOLE)) keys.add('text ' + x[1]); return; }
    if (node.tag === 'sc-if') { const h = /\{\{\s*([^}]*?)\s*\}\}/.exec(node.attrs.value || ''); if (h) keys.add('sc-if ' + h[1]); }
    else if (node.tag === 'sc-for') { const h = /\{\{\s*([^}]*?)\s*\}\}/.exec(node.attrs.list || ''); if (h) keys.add('sc-for ' + h[1]); }
    else for (const [k, v] of Object.entries(node.attrs || {})) { if (typeof v === 'string' && !k.startsWith('hint-')) for (const x of v.matchAll(HOLE)) keys.add(node.tag + '@' + k + ' ' + x[1]); }
    (node.children || []).forEach(walk);
  })(tree);
  return keys;
}
module.exports = { parse, check, allHoleKeys };
