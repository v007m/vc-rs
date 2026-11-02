// Static check: every {{path}} in the template must be produced by renderVals().
// Loop aliases are mapped to their list's item shape; keys are unioned over many UI states.
const H = require('./harness');
const C = H.load();
const tpl = H.parseTemplate(C.html);
// 1. collect holes with their alias chain
const holes = [];
(function walk(n, aliases) {
  if (n.text !== undefined) { (n.text.match(/\{\{\s*([^}]+?)\s*\}\}/g) || []).forEach(h => holes.push({ p: h.replace(/[{}\s]/g, ''), aliases })); return; }
  let next = aliases;
  if (n.tag === 'sc-for') { const list = /\{\{\s*([^}]+?)\s*\}\}/.exec(n.attrs.list)[1]; holes.push({ p: list, aliases }); next = Object.assign({}, aliases, { [n.attrs.as]: list }); }
  for (const [k, v] of Object.entries(n.attrs)) if (typeof v === 'string' && k !== 'list') (v.match(/\{\{\s*([^}]+?)\s*\}\}/g) || []).forEach(h => holes.push({ p: h.replace(/[{}\s]/g, ''), aliases }));
  (n.kids || []).forEach(k => walk(k, next));
})(tpl, {});
// 2. union of shapes over states
const shapes = {};
function addShape(prefix, v) {
  if (v === null || v === undefined) return;
  if (Array.isArray(v)) { v.forEach(x => addShape(prefix + '[]', x)); shapes[prefix] = true; return; }
  shapes[prefix] = true;
  if (typeof v === 'object') Object.keys(v).forEach(k => { if ((k === 'model' || k === 'a' || k === 'api') && typeof v[k] === 'object') return; addShape(prefix ? prefix + '.' + k : k, v[k]); });
}
function snap(c) { addShape('', c.renderVals()); }
const c = H.mount(C);
for (const p of Object.keys(c.state.files)) {
  c.openFile(p);
  for (const v of ['form', 'source', 'split']) {
    c.setView(v);
    const k = C.fileKind(p), m = c.state.files[p].model;
    let sels = [undefined];
    if (k === 'test') sels = ['file', 'life'].concat((m.tests || []).map((t, i) => 'test:' + i));
    if (k === 'api') sels = ['file'].concat((m.apis || []).map((t, i) => 'api:' + i));
    if (k === 'config') sels = ['general', 'profiles', 'auth', 'defaults', 'output', 'hooks'];
    if (k === 'load') sels = ['scenario', 'mix', 'stages', 'thresholds'];
    for (const s of sels) { if (s) c.setSel(s); snap(c); }
  }
  c.toggleDiff(); snap(c); c.setState({ diffMode: 'uni' }); snap(c); c.setState({ diffMode: 'sbs' }); c.toggleDiff();
}
c.openFile('services/orders/checkout.test.yaml'); c.setSel('test:0'); c.renderVals(); c.fieldInput(C.fidOf(['tests', 0, 'steps', 0, 'call']), 'orders.', 7, 'call', ['tests', 0, 'steps', 0, 'call'], null); snap(c);
c.setView('source'); c.setState({ comp: { path: 'services/orders/checkout.test.yaml', items: [{ label: 'a', detail: 'b', method: 'GET', insert: 'a' }], sel: 0, from: 0, to: 0, line: 0, col: 0 } }); snap(c);
c.setState({ menu: 'project' }); snap(c); c.setState({ menu: 'user' }); snap(c);
c.runTest(); snap(c); C.flush(); snap(c);
c.setState({ blocked: { path: 'services/orders/checkout.test.yaml', n: 1, first: 'x' }, toast: { msg: 'm', sub: 's', kind: 'ok' } }); snap(c);
for (const k of ['test', 'api', 'testFile', 'load', 'schema', 'dataset', 'hook']) { c.openNew(k); c.dlgSet({ name: 'A b', service: '+new', file: '+new', hookFile: '+new', fromApi: 'orders.get' }); snap(c); }
c.setState({ dlg: null, confirm: { path: 'services/orders/checkout.test.yaml' } }); snap(c);
const c2 = H.mount(C); c2.setSel('test:1');
let V2 = c2.renderVals(); V2.fsecs.forEach(sc => sc.rows.forEach(r => { if (r.isText && r.lbl === 'Name') r.on({ target: { value: 'Changed name', selectionStart: 3 } }); }));
c2.toggleDiff(); snap(c2); c2.setState({ diffMode: 'uni' }); snap(c2); c2.toggleDiff();
V2 = c2.renderVals(); const vt = []; V2.fsecs.forEach(sc => sc.rows.forEach(r => { if (r.isTable && r.lbl === 'Vars') vt.push(r); }));
vt[0].items[0].cells[1].on({ target: { value: '${var.', selectionStart: 6 } }); snap(c2);
V2 = c2.renderVals(); V2.fsecs.forEach(sc => sc.rows.forEach(r => { if (r.isTable && r.lbl === 'Vars') r.add(); }));
V2 = c2.renderVals(); V2.fsecs.forEach(sc => sc.rows.forEach(r => { if (r.isTable && r.lbl === 'Vars' && r.items[1]) r.items[1].cells[0].on({ target: { value: 'paymentToken' } }); })); snap(c2);
c2.openNew('api'); c2.dlgSet({ name: 'Users list', service: 'users', apiPath: '/u' }); snap(c2);
// 3. compare
function shapeOf(h) {
  const segs = h.p.split('.'); let head = segs[0], rest = segs.slice(1), guard = 0;
  while (h.aliases[head] && guard++ < 10) { const list = h.aliases[head]; const ls = list.split('.'); head = ls[0]; rest = ls.slice(1).concat(['[]']).concat(rest); h = { p: '', aliases: h.aliases }; }
  return [head].concat(rest).join('.').replace(/\.\[\]/g, '[]');
}
const missing = [];
holes.forEach(h => {
  if (/^(true|false|\d+)$/.test(h.p) || h.p === '$index') return;
  const s = shapeOf(h);
  if (!shapes[s]) missing.push(h.p + '  ->  ' + s);
});
console.log('template holes', holes.length, 'distinct shapes produced', Object.keys(shapes).length);
console.log('unproduced:', [...new Set(missing)]);
process.exitCode = missing.length ? 1 : 0;
