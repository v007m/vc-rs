// Loads the built artboard, instantiates Component with a stub DCLogic, and renders its template
// with a small interpreter (sc-for / sc-if / {{holes}}) to find unresolved names and collect handlers.
const fs = require('fs'), vm = require('vm');
const SCR = '/tmp/claude-0/-home-claude/80b4900a-8223-5c77-874b-c1dda2eeca25/scratchpad';
const HTML = SCR + '/vibmock/project/WebEditor.dc.html';
function load(opts = {}) {
  const html = fs.readFileSync(HTML, 'utf8');
  const src = /<script type="text\/x-dc" data-dc-script[^>]*>([\s\S]*?)<\/script>/.exec(html)[1];
  const timers = [];
  const ctx = { console, Date, Math, JSON, setTimeout: (f, ms) => { timers.push({ f, ms }); return timers.length; }, clearTimeout() {} };
  ctx.DCLogic = class { constructor(props) { this.props = props || {}; this.state = {}; } setState(u) { const p = typeof u === 'function' ? u(this.state, this.props) : u; this.state = Object.assign({}, this.state, p); if (this.componentDidUpdate) this.componentDidUpdate(); } forceUpdate() {} };
  if (opts.jsyaml !== false && process.env.NOJSYAML !== '1') ctx.jsyaml = require(SCR + '/npmtmp/package');
  vm.createContext(ctx);
  vm.runInContext(src + '\n;this.Component = Component;', ctx);
  Object.assign(ctx, ctx.Component.logic);
  ctx.flush = (maxMs = 1e9) => { let n = 0; while (timers.length && n < 50) { const t = timers.shift(); if (t.ms <= maxMs) t.f(); n++; } };
  ctx.timers = timers;
  ctx.html = html;
  return ctx;
}
// ---- template parsing ----
const VOID = new Set(['input', 'br', 'img', 'meta', 'link', 'hr', 'source', 'col', 'area', 'base', 'wbr']);
function parseTemplate(html) {
  let body = /<x-dc>([\s\S]*)<\/x-dc>/.exec(html)[1];
  body = body.replace(/<helmet>[\s\S]*?<\/helmet>/, '');
  const root = { tag: '#root', attrs: {}, kids: [] }, stack = [root];
  const re = /<!--[\s\S]*?-->|<\/([a-zA-Z0-9-]+)\s*>|<([a-zA-Z0-9-]+)((?:\s+[^\s=>\/]+(?:\s*=\s*(?:"[^"]*"|'[^']*'))?)*)\s*(\/?)>|([^<]+)/g;
  let m;
  while ((m = re.exec(body))) {
    if (m[0].startsWith('<!--')) continue;
    if (m[1]) {
      const t = m[1].toLowerCase(), top = stack.pop();
      if (!top || top.tag !== t) throw new Error('mismatched </' + t + '> (open: ' + (top && top.tag) + ') near ' + body.slice(m.index - 80, m.index + 20));
      continue;
    }
    if (m[2]) {
      const tag = m[2].toLowerCase(), attrs = {}, ar = /([^\s=>\/]+)(?:\s*=\s*("([^"]*)"|'([^']*)'))?/g;
      let a; while ((a = ar.exec(m[3] || ''))) attrs[a[1]] = a[3] !== undefined ? a[3] : a[4] !== undefined ? a[4] : true;
      const node = { tag, attrs, kids: [] };
      stack[stack.length - 1].kids.push(node);
      if (!VOID.has(tag) && !m[4]) stack.push(node);
      continue;
    }
    if (m[5] && m[5].trim()) stack[stack.length - 1].kids.push({ text: m[5] });
  }
  if (stack.length !== 1) throw new Error('unclosed: ' + stack.map(s => s.tag).join('>'));
  return root;
}
function resolve(expr, scope) {
  expr = expr.trim();
  if (expr === 'true') return true; if (expr === 'false') return false;
  if (/^-?\d+(\.\d+)?$/.test(expr)) return +expr;
  const parts = expr.split('.');
  let v = scope;
  for (const p of parts) { if (v == null) return undefined; v = v[p]; }
  return v;
}
function render(root, vals) {
  const out = { missing: [], badHandlers: [], buttons: [], inputs: [], count: 0 };
  function holes(str, scope, where, isHandler) {
    const re = /\{\{\s*([^}]+?)\s*\}\}/g; let m;
    while ((m = re.exec(str))) {
      const v = resolve(m[1], scope);
      if (v === undefined) out.missing.push(where + ' {{' + m[1] + '}}');
      if (isHandler && typeof v !== 'function') out.badHandlers.push(where + ' {{' + m[1] + '}} -> ' + typeof v);
    }
  }
  function walk(n, scope, trail, pcls) {
    if (n.text !== undefined) { holes(n.text, scope, trail + ' text'); return; }
    out.count++;
    if (n.tag === 'sc-for') {
      const list = resolve(/\{\{\s*([^}]+?)\s*\}\}/.exec(n.attrs.list)[1], scope);
      if (!Array.isArray(list)) { out.missing.push(trail + ' sc-for list ' + n.attrs.list + ' is ' + typeof list); return; }
      list.forEach((item, i) => { const s = Object.assign(Object.create(scope), { [n.attrs.as]: item, $index: i }); n.kids.forEach(k => walk(k, s, trail + '>' + n.attrs.as + '[' + i + ']', pcls)); });
      return;
    }
    if (n.tag === 'sc-if') {
      const e = /\{\{\s*([^}]+?)\s*\}\}/.exec(n.attrs.value)[1], v = resolve(e, scope);
      if (v === undefined) out.missing.push(trail + ' sc-if {{' + e + '}}');
      if (v) n.kids.forEach(k => walk(k, scope, trail, pcls));
      return;
    }
    const attrVals = {};
    for (const [k, v] of Object.entries(n.attrs)) {
      if (typeof v !== 'string') continue;
      const isH = /^on[A-Z]/.test(k);
      holes(v, scope, trail + ' <' + n.tag + ' ' + k + '>', isH);
      const m = /^\{\{\s*([^}]+?)\s*\}\}$/.exec(v);
      attrVals[k] = m ? resolve(m[1], scope) : v.replace(/\{\{\s*([^}]+?)\s*\}\}/g, (_, e) => resolve(e, scope));
    }
    const label = () => { const t = []; (function tx(x) { if (x.text) t.push(x.text.trim()); (x.kids || []).forEach(tx); })(n); return (attrVals['aria-label'] || t.join(' ') || attrVals.title || '').replace(/\{\{[^}]*\}\}/g, '').trim(); };
    if (n.tag === 'button' || (n.tag === 'a' && attrVals.href)) out.buttons.push({ tag: n.tag, label: label() || attrVals.title, on: attrVals.onClick || attrVals.onMouseDown, disabled: attrVals.disabled === true, title: attrVals.title, current: attrVals['aria-pressed'] === 'true' || attrVals['aria-selected'] === 'true' || / on( |$)/.test(' ' + (attrVals.class || '')) || / on( |$)/.test(' ' + (pcls || '')), href: attrVals.href, trail });
    if (n.tag === 'input' || n.tag === 'select' || n.tag === 'textarea') out.inputs.push({ tag: n.tag, id: attrVals.id, aria: attrVals['aria-label'], val: attrVals.value, onInput: attrVals.onInput, onChange: attrVals.onChange, onKeyDown: attrVals.onKeyDown, onBlur: attrVals.onBlur, type: attrVals.type, checked: attrVals.checked, trail });
    n.kids.forEach(k => walk(k, scope, trail + '>' + n.tag, attrVals.class));
  }
  walk(root, vals, '');
  return out;
}
function mount(C) { const c = new C.Component({}); return c; }
function view(C, c) { const tpl = parseTemplate(C.html); const V = c.renderVals(); return Object.assign(render(tpl, V), { V }); }
module.exports = { load, parseTemplate, render, mount, view, SCR };
