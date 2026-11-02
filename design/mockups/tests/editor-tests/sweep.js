const H = require('./harness');
const C = H.load();
const tpl = H.parseTemplate(C.html);
let problems = 0, renders = 0;
function uniq(a) { return [...new Set(a.map(x => x.replace(/\[\d+\]/g, '[]')))]; }
function chk(c, label) {
  let r;
  try { const V = c.renderVals(); r = Object.assign(H.render(tpl, V), { V }); } catch (e) { console.log('CRASH', label, e.stack.split('\n').slice(0, 4).join(' | ')); problems++; return null; }
  renders++;
  const miss = uniq(r.missing), bad = uniq(r.badHandlers);
  if (miss.length || bad.length) { console.log('GAPS', label, miss.slice(0, 6), bad.slice(0, 6)); problems++; }
  return r;
}
const c = H.mount(C);
const files = Object.keys(c.state.files);
for (const p of files) {
  c.openFile(p);
  for (const v of ['form', 'source', 'split']) {
    c.setView(v);
    const k = C.fileKind(p);
    const m = c.state.files[p].model;
    let sels = [undefined];
    if (k === 'test') sels = ['file', 'life'].concat((m.tests || []).map((t, i) => 'test:' + i));
    if (k === 'api') sels = ['file'].concat((m.apis || []).map((t, i) => 'api:' + i));
    if (k === 'config') sels = ['general', 'profiles', 'auth', 'defaults', 'output', 'hooks'];
    if (k === 'load') sels = ['scenario', 'mix', 'stages', 'thresholds'];
    for (const s of sels) { if (s) c.setSel(s); chk(c, p + ' ' + v + ' ' + s); }
  }
  c.toggleDiff(); chk(c, p + ' diff'); c.setState({ diffMode: 'uni' }); chk(c, p + ' diff uni'); c.setState({ diffMode: 'sbs' }); c.toggleDiff();
}
// broken yaml in form and split
c.openFile('services/orders/checkout.test.yaml'); c.setView('form');
const t0 = c.state.files['services/orders/checkout.test.yaml'].text;
c.applyText('services/orders/checkout.test.yaml', t0.replace('  - id: placesOrder', '  - id: placesOrder\n   bad: [1,'));
let r = chk(c, 'broken form'); console.log('broken: parseErr', r && r.V.parseErr, 'line', r && r.V.parseErrLine, r && r.V.parseErrMsg);
c.setView('split'); chk(c, 'broken split');
c.toggleDiff(); r = chk(c, 'broken diff'); console.log('diff counts', r.V.diffAdd, r.V.diffDel); c.toggleDiff();
c.revert(); chk(c, 'reverted');
// dialogs
for (const k of ['test', 'api', 'testFile', 'load', 'schema', 'dataset', 'hook']) { c.openNew(k); chk(c, 'dlg ' + k); c.dlgSet({ name: 'Rejects a declined card', area: 'history', scenario: 'peak', apiPath: '/x' }); chk(c, 'dlg filled ' + k); }
c.setState({ dlg: null });
// confirm close dialog
c.openFile('services/orders/checkout.test.yaml');
c.applyText('services/orders/checkout.test.yaml', t0.replace('name: Checkout', 'name: Checkout flow'));
c.closeTab('services/orders/checkout.test.yaml'); r = chk(c, 'confirm'); console.log('confirm open', r.V.confirmOpen);
c.setState({ confirm: null });
// close all tabs
for (const p of c.state.open.slice()) c.closeTab(p, true);
chk(c, 'no file');
console.log('renders', renders, 'problems', problems);
