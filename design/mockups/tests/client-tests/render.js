// Static render of template + renderVals() for visual checks (no event wiring).
const fs = require('fs'), path = require('path');
const { loadComponent, readArtboard } = require('./load');
const { parse } = require('./holes');
const VOID = new Set(['area','base','br','col','embed','hr','img','input','link','meta','source','track','wbr']);
const esc = (s) => String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;').replace(/"/g,'&quot;');
function lookup(p, scope, vals) {
  if (p === 'true') return true; if (p === 'false') return false;
  const parts = p.split('.'); let cur = Object.prototype.hasOwnProperty.call(scope, parts[0]) ? scope[parts[0]] : vals[parts[0]];
  for (let i = 1; i < parts.length; i++) { if (cur == null) return undefined; cur = cur[parts[i]]; } return cur;
}
const interp = (s, scope, vals) => s.replace(/\{\{\s*([^}]*?)\s*\}\}/g, (a, p) => { const r = lookup(p, scope, vals); return r == null ? '' : String(r); });
function render(node, scope, vals, ctx) {
  if (node.text !== undefined) return esc(interp(node.text, scope, vals)).replace(/&amp;(\w+;)/g, '&$1');
  if (node.tag === 'sc-if') { const h = /\{\{\s*([^}]*?)\s*\}\}/.exec(node.attrs.value); return lookup(h[1], scope, vals) ? node.children.map((c) => render(c, scope, vals, ctx)).join('') : ''; }
  if (node.tag === 'sc-for') { const h = /\{\{\s*([^}]*?)\s*\}\}/.exec(node.attrs.list); const list = lookup(h[1], scope, vals) || []; return list.map((it, i) => node.children.map((c) => render(c, Object.assign({}, scope, { [node.attrs.as]: it, $index: i }), vals, ctx)).join('')).join(''); }
  if (node.tag === '#root') return node.children.map((c) => render(c, scope, vals, ctx)).join('');
  let attrs = '', value = null;
  for (const [k, v] of Object.entries(node.attrs)) {
    if (/^on[A-Z]/.test(k) || k.startsWith('hint-')) continue;
    if (v === true) { attrs += ' ' + k; continue; }
    const whole = /^\{\{\s*([^}]*?)\s*\}\}$/.exec(v);
    const r = whole ? lookup(whole[1], scope, vals) : interp(v, scope, vals);
    if (k === 'checked' || k === 'disabled' || k === 'selected') { if (r) attrs += ' ' + k; continue; }
    if (k === 'value') { value = r; if (node.tag === 'input') attrs += ' value="' + esc(r) + '"'; continue; }
    attrs += ' ' + k + '="' + esc(r) + '"';
  }
  if (node.tag === 'select') { ctx.sel = value; }
  if (node.tag === 'option' && ctx.sel != null && node.attrs.value === String(ctx.sel)) attrs += ' selected';
  if (VOID.has(node.tag)) return '<' + node.tag + attrs + '>';
  let inner = node.tag === 'textarea' ? esc(value || '') : node.children.map((c) => render(c, scope, vals, ctx)).join('');
  return '<' + node.tag + attrs + '>' + inner + '</' + node.tag + '>';
}
// Builds a standalone page from an artboard's html + renderVals() output, with the v2 stylesheet,
// the local grain texture and locally installed Geist fonts (Google Fonts is not reachable here).
function pageFrom(html, vals) {
  const helmet = /<helmet>([\s\S]*?)<\/helmet>/.exec(html)[1].replace(/<link[^>]*fonts\.googleapis[^>]*>/g, '');
  const tpl = html.slice(html.indexOf('<x-dc>') + 6, html.indexOf('</x-dc>')).replace(/<helmet>[\s\S]*?<\/helmet>/, '');
  const sheet = /href="\/_blob\/81dcd3b549d078b575adb6aed82e1861"/.test(html) ? 'vib-web2.css' : 'vib-web.css';
  const css = fs.readFileSync(path.resolve(__dirname, '../vibmock/assets/' + sheet), 'utf8').replace('/_blob/9ea3af80b64659e41e7d942696a9a0d2', 'file://' + path.resolve(__dirname, '../vibmock/assets/noise.png'));
  const fonts = fs.readFileSync(path.resolve(__dirname, '../geistdl/fonts.css'), 'utf8');
  return '<!doctype html><html><head><meta charset="utf-8"><style>' + fonts + css + '</style>' + helmet + '</head><body>' + render(parse(tpl).root, {}, vals, {}) + '</body></html>';
}
function page(vals) { return pageFrom(readArtboard(), vals); }
module.exports = { page, pageFrom };
