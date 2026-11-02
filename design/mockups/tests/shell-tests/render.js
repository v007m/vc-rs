// Static render of template + renderVals() for visual checks (no event wiring), v2 stylesheet, local Geist.
const fs = require('fs'), path = require('path');
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
  if (node.text !== undefined) return esc(interp(node.text, scope, vals)).replace(/&amp;(\w+;|#\d+;)/g, '&$1');
  if (node.tag === 'sc-if') { const h = /\{\{\s*([^}]*?)\s*\}\}/.exec(node.attrs.value); return lookup(h[1], scope, vals) ? node.children.map((c) => render(c, scope, vals, ctx)).join('') : ''; }
  if (node.tag === 'sc-for') { const h = /\{\{\s*([^}]*?)\s*\}\}/.exec(node.attrs.list); const list = lookup(h[1], scope, vals) || []; return list.map((it, i) => node.children.map((c) => render(c, Object.assign({}, scope, { [node.attrs.as]: it, $index: i }), vals, ctx)).join('')).join(''); }
  if (node.tag === '#root') return node.children.map((c) => render(c, scope, vals, ctx)).join('');
  let attrs = '', value = null;
  for (const [k, v] of Object.entries(node.attrs)) {
    if (/^on[A-Z]/.test(k) || k.startsWith('hint-')) continue;
    if (v === true) { attrs += ' ' + k; continue; }
    const whole = /^\{\{\s*([^}]*?)\s*\}\}$/.exec(v);
    const dec = (x) => x.replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&#39;/g, "'").replace(/&amp;/g, '&');
    const r = whole ? lookup(whole[1], scope, vals) : interp(dec(v), scope, vals);
    if (k === 'checked' || k === 'disabled') { if (r) attrs += ' ' + k; continue; }
    if (k === 'value') { value = r; if (node.tag === 'input') attrs += ' value="' + esc(r) + '"'; continue; }
    attrs += ' ' + k + '="' + esc(r) + '"';
  }
  if (node.tag === 'select') { ctx.sel = value; }
  if (node.tag === 'option' && ctx.sel != null && node.attrs.value === String(ctx.sel)) attrs += ' selected';
  if (VOID.has(node.tag)) return '<' + node.tag + attrs + '>';
  const inner = node.tag === 'textarea' ? esc(value || '') : node.children.map((c) => render(c, scope, vals, ctx)).join('');
  return '<' + node.tag + attrs + '>' + inner + '</' + node.tag + '>';
}
function page(html, vals) {
  const helmet = /<helmet>([\s\S]*?)<\/helmet>/.exec(html)[1].replace(/<link[^>]*fonts\.googleapis[^>]*>/, '');
  const tpl = html.slice(html.indexOf('<x-dc>') + 6, html.indexOf('</x-dc>')).replace(/<helmet>[\s\S]*?<\/helmet>/, '');
  const css = fs.readFileSync(path.resolve(__dirname, '../vibmock/assets/vib-web2.css'), 'utf8').replace('/_blob/9ea3af80b64659e41e7d942696a9a0d2', 'file://' + path.resolve(__dirname, '../vibmock/assets/noise.png'));
  const fonts = fs.readFileSync(path.join(__dirname, 'fonts.css'), 'utf8');
  if (/1e69a6d642232c67587a282225e98669/.test(html)) console.warn('WARNING: v1 stylesheet still linked');
  return '<!doctype html><html><head><meta charset="utf-8"><style>' + fonts + css + '</style>' + helmet + '</head><body>' + render(parse(tpl).root, {}, vals, {}) + '</body></html>';
}
module.exports = { page };
