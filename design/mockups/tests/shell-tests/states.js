// Renders named states of an artboard to shots/<Board>-<state>[~c].html (~c = sidebar collapsed).
const fs = require('fs'), path = require('path');
const { loadComponent } = require('./load');
const { page } = require('./render');
const ev = (value, checked) => ({ target: { value, checked }, preventDefault() {}, stopPropagation() {} });
const SCEN = {
  WebExplorer: [
    ['initial', () => {}],
    ['query-sel', (c, v) => { v().onQuery(ev('status:failed tag:smoke api:orders')); v().views[1].pick(); v().groups[0].rows[0].toggle(); v().groups[0].rows[1].toggle(); }],
    ['tree', (c, v) => { v().showTree(); }],
    ['newtest', (c, v) => { v().openNew(); v().onNtName(ev('Test rejects a declined card')); }],
    ['skip', (c, v) => { v().groups[0].rows[0].toggle(); v().openSkip(); }],
    ['empty', (c, v) => { v().onQuery(ev('name:"nothing like this" tag:unknownthing')); }],
  ],
  WebRun: [
    ['initial', () => {}],
    ['mid', (c, v, t) => { c.componentDidMount(); t.advance(9000); v().life[4].pick(); v().selectors[2].queue(); }],
    ['done', (c, v, t) => { c.componentDidMount(); t.advance(30000); v().rerunFailed(); }],
    ['paused', (c, v, t) => { c.componentDidMount(); t.advance(4000); v().togglePause(); t.advance(5000); }],
  ],
  WebInsights: [
    ['initial', () => {}],
    ['table-skip', (c, v) => { v().toggleTable(); v().flaky[0].openSkip(); v().flaky[1].quarantine(); }],
    ['gaps', (c, v) => { v().ranges[0].pick(); v().toggleGaps(); v().svcs[1].pick(); }],
  ],
};
const board = process.argv[2];
const only = process.argv.slice(3);
let extra = {};
try { extra = require('./settings-states.js'); } catch (e) { if (e.code !== 'MODULE_NOT_FOUND') throw e; }
const list = (SCEN[board] || []).concat(extra[board] || []);
for (const [name, fn] of list) {
  if (only.length && !only.includes(name)) continue;
  for (const collapsed of [false, true]) {
    const { Component, timers, html } = loadComponent(board + '.dc.html');
    const c = new Component({});
    const v = () => c.renderVals();
    fn(c, v, timers, ev);
    if (collapsed) c.setState({ sideCollapsed: true });
    fs.writeFileSync(path.join(__dirname, 'shots', board + '-' + name + (collapsed ? '~c' : '') + '.html'), page(html, c.renderVals()));
  }
}
console.log('rendered', board, list.length, 'states');
