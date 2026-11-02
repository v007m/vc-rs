// Statically renders the editor artboard in a set of UI states for screenshots and the layout audit.
// Each state is written twice: <name>.html (sidebar expanded) and <name>-c.html (sidebar collapsed).
//   node states.js            -> editor-tests/shots/*.html, then: python3 ../client-tests/shoot.py shots
const fs = require('fs'), path = require('path');
const H = require('./harness');
const { pageFrom } = require('../client-tests/render');
const C = H.load();
const out = path.join(__dirname, 'shots');
fs.mkdirSync(out, { recursive: true });
for (const f of fs.readdirSync(out)) if (/\.html$/.test(f)) fs.unlinkSync(path.join(out, f));
const save = (c, name) => {
  fs.writeFileSync(path.join(out, name + '.html'), pageFrom(C.html, c.renderVals()));
  const was = c.state.sideCollapsed; c.setState({ sideCollapsed: true });
  fs.writeFileSync(path.join(out, name + '-c.html'), pageFrom(C.html, c.renderVals()));
  c.setState({ sideCollapsed: was });
};
const CO = 'services/orders/checkout.test.yaml';
let c = H.mount(C);
save(c, '01-test-form');
c.setView('split'); save(c, '02-test-split');
c.setView('source'); save(c, '03-test-source');
c.setView('form'); c.setSel('life'); save(c, '04-test-lifecycle');
c.setSel('file'); save(c, '05-test-file');
c.openFile('services/users/users.api.yaml'); c.setView('form'); c.setSel('api:0'); save(c, '06-api-form');
c.openFile('vibranium.config.yaml'); c.setView('form'); c.setSel('profiles'); save(c, '07-config-profiles');
c.setSel('auth'); save(c, '08-config-auth');
c.openFile('load/checkout.load.yaml'); c.setView('form'); c.setSel('stages'); save(c, '09-load-stages');
c.setSel('mix'); save(c, '10-load-mix');
c.openFile('datasets/capitals.csv'); c.setView('form'); save(c, '11-dataset-grid');
c.openFile(CO); c.setView('form'); c.setSel('test:0');
const t0 = c.state.files[CO].text;
c.applyText(CO, t0.replace('name: Checkout', 'name: Checkout flow for a signed-in shopper').replace('every: 1s', 'every: 2s'));
c.toggleDiff(); save(c, '12-diff-sbs');
c.setState({ diffMode: 'uni' }); save(c, '13-diff-uni'); c.setState({ diffMode: 'sbs' }); c.toggleDiff();
c.runTest(); C.flush(); save(c, '14-run-result');
c.applyText(CO, t0.replace('  - id: placesOrder', '  - id: placesOrder\n   bad: [1,')); save(c, '15-parse-error');
c.revert();
c.setState({ blocked: { path: CO, n: 2, first: 'tests[0].steps[1].call: orders.fetch is not an api in services/orders/orders.api.yaml' }, toast: { msg: 'Saved services/orders/checkout.test.yaml', sub: 'vc check passes · 3 tests, 2 lifecycle actions', kind: 'ok' } });
save(c, '16-blocked-toast');
c.setState({ blocked: null, toast: null, run: null });
c.openNew('test'); c.dlgSet({ name: 'Rejects a declined card' }); save(c, '17-new-test');
c.openNew('api'); c.dlgSet({ name: 'Start an export', service: 'users', apiPath: '/exports' }); save(c, '18-new-api');
c.setState({ dlg: null });
c.applyText(CO, t0.replace('name: Checkout', 'name: Checkout flow'));
c.closeTab(CO); save(c, '19-confirm-close');
c.setState({ confirm: null }); c.revert();
for (const p of c.state.open.slice()) c.closeTab(p, true);
save(c, '20-no-file');
console.log('states written to', out);
