// Static render of an artboard's template + renderVals() to plain HTML (v2 stylesheet inlined),
// plus a per-state hole check. Usage from a states script: const r = makeRenderer(artPath, outDir); r.save(name, vals)
const fs = require('fs'), path = require('path');
const { parse, check } = require('./holes');
const VOID = new Set(['area','base','br','col','embed','hr','img','input','link','meta','source','track','wbr']);
const esc = (s) => String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
function lookup(p, scope, vals) {
  if (p === 'true') return true; if (p === 'false') return false;
  const parts = p.split('.'); let cur = Object.prototype.hasOwnProperty.call(scope, parts[0]) ? scope[parts[0]] : vals[parts[0]];
  for (let i = 1; i < parts.length; i++) { if (cur == null) return undefined; cur = cur[parts[i]]; } return cur;
}
const interp = (s, scope, vals) => s.replace(/\{\{\s*([^}]*?)\s*\}\}/g, (a, p) => { const r = lookup(p, scope, vals); return r == null ? '' : String(r); });
function render(node, scope, vals, ctx) {
  if (node.text !== undefined) return esc(interp(node.text, scope, vals)).replace(/&amp;(#?\w+;)/g, '&$1');
  if (node.tag === 'sc-if') { const h = /\{\{\s*([^}]*?)\s*\}\}/.exec(node.attrs.value); return lookup(h[1], scope, vals) ? node.children.map((c) => render(c, scope, vals, ctx)).join('') : ''; }
  if (node.tag === 'sc-for') { const h = /\{\{\s*([^}]*?)\s*\}\}/.exec(node.attrs.list); const list = lookup(h[1], scope, vals) || []; return list.map((it, i) => node.children.map((c) => render(c, Object.assign({}, scope, { [node.attrs.as]: it, $index: i }), vals, ctx)).join('')).join(''); }
  if (node.tag === '#root') return node.children.map((c) => render(c, scope, vals, ctx)).join('');
  let attrs = '', value = null;
  for (const [k, v] of Object.entries(node.attrs)) {
    if (/^on[A-Z]/.test(k) || k.startsWith('hint-')) continue;
    if (v === true) { attrs += ' ' + k; continue; }
    const whole = /^\{\{\s*([^}]*?)\s*\}\}$/.exec(v);
    const r = whole ? lookup(whole[1], scope, vals) : interp(v, scope, vals);
    if (k === 'checked' || k === 'disabled') { if (r) attrs += ' ' + k; continue; }
    if (k === 'value') { value = r; if (node.tag === 'input') attrs += ' value="' + esc(r) + '"'; continue; }
    attrs += ' ' + k + '="' + esc(r).replace(/&amp;(#?\w+;)/g, '&$1') + '"';
  }
  if (node.tag === 'select') { ctx.sel = value; }
  if (node.tag === 'option' && ctx.sel != null && node.attrs.value === String(ctx.sel)) attrs += ' selected';
  if (VOID.has(node.tag)) return '<' + node.tag + attrs + '>';
  const inner = node.tag === 'textarea' ? esc(value || '') : node.children.map((c) => render(c, scope, vals, ctx)).join('');
  return '<' + node.tag + attrs + '>' + inner + '</' + node.tag + '>';
}
function makeRenderer(art, outDir) {
  const html = fs.readFileSync(art, 'utf8');
  const helmet = /<helmet>([\s\S]*?)<\/helmet>/.exec(html)[1];
  const tpl = html.slice(html.indexOf('<x-dc>') + 6, html.indexOf('</x-dc>')).replace(/<helmet>[\s\S]*?<\/helmet>/, '');
  const parsed = parse(tpl);
  if (parsed.errors.length) console.log('PARSE', parsed.errors);
  const css = fs.readFileSync(path.resolve(__dirname, '../vibmock/assets/vib-web2.css'), 'utf8').replace('/_blob/9ea3af80b64659e41e7d942696a9a0d2', 'file://' + path.resolve(__dirname, '../vibmock/assets/noise.png'));
  fs.mkdirSync(outDir, { recursive: true });
  const errs = [];
  return {
    save(name, vals) {
      const report = { err: [], ok: new Set(), seen: new Set(), ifTrue: new Set(), ifFalse: new Set(), forNonEmpty: new Set() };
      check(parsed.root, vals, name, report);
      errs.push(...report.err);
      const out = '<!doctype html><html lang="en"><head><meta charset="utf-8"><style>' + css + '</style>' + helmet + '</head><body>' + render(parsed.root, {}, vals, {}) + '</body></html>';
      fs.writeFileSync(path.join(outDir, name + '.html'), out);
    },
    errors() { return errs; }
  };
}
module.exports = { makeRenderer };
