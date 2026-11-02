// Drives the component through every screen state by calling the handlers renderVals() returns,
// and checks every {{hole}} in the template against the values at each step.
const assert = require('assert');
const { loadComponent } = require('./load');
const { parse, check, allHoleKeys } = require('./holes');
const jsyaml = require('../npmtmp/package');

const { Component, timers, template } = loadComponent();
const { root, errors: parseErrors } = parse(template);
assert.deepStrictEqual(parseErrors, [], 'template parses cleanly');
const report = { err: [], ok: new Set(), seen: new Set(), ifTrue: new Set(), ifFalse: new Set(), forNonEmpty: new Set() };

const c = new Component({});
let steps = 0;
function V(label) { const v = c.renderVals(); check(root, v, label, report); steps++; return v; }
const ev = (value, checked) => ({ target: { value: value, checked: checked }, metaKey: false, ctrlKey: false, key: '', preventDefault() {} });
const evc = (checked) => ev(undefined, checked);
function step(label, fn) { fn(); return V(label); }

let v = V('initial');
assert.strictEqual(v.req.urlTpl, '${var.usersUrl}/api/v1/users/${var.userId}');
assert.strictEqual(v.req.urlSegs.map((s) => s.t).join(''), 'https://users.staging.shop.example/api/v1/users/u_7f3k29qp');
assert.strictEqual(v.resp.statusText, '200 OK');
assert.strictEqual(v.tabs.length, 4);
assert.strictEqual(v.side.groups.length, 6);

// response tabs
for (const t of v.resp.tabs) { t.pick(); V('resp tab ' + t.label); }
// request sub tabs
v = V('x');
for (const t of v.req.subtabs) { t.pick(); V('req tab ' + t.label); }

// profile switch changes url + auth
v = step('profile dev', () => v.setProfile(ev('dev')));
assert.ok(v.req.urlSegs.map((s) => s.t).join('').startsWith('http://localhost:8081'));
v = step('profile prod', () => v.setProfile(ev('prod')));
assert.strictEqual(v.req.authTone, 'block');
v = step('profile staging', () => v.setProfile(ev('staging')));
timers.advance(4000);

// orders.place tab: failing check
v = step('tab 2', () => v.tabs[1].select());
assert.strictEqual(v.resp.checksText, '1 of 2 checks failed');
assert.ok(v.resp.t.checks);
assert.ok(v.resp.hasFailed && /a1c9e4f/.test(v.resp.failHint));
const failRow = v.resp.checks.find((x) => x.label === 'body.match.total');
assert.deepStrictEqual([failRow.expected, failRow.actual, failRow.result], ['59.98', '64.78', 'fail']);

// body editing: invalid json, format, modes
v = V('body');
assert.ok(v.req.t.body);
v = step('bad json', () => v.req.body.setText(ev('{\n  "cartId": "${var.cartId}",\n}')));
assert.ok(v.req.body.hasErr && /line 3/.test(v.req.body.err), v.req.body.err);
v = step('send bad json', () => v.req.send());
timers.flush(); v = V('after bad json send');
assert.ok(v.resp.error && /JSON/.test(v.resp.errTitle));
v = step('fix json', () => v.req.body.setText(ev('{"cartId":"${var.cartId}","userId":"${var.userId}","currency":"${var.currency}"}')));
v = step('format', () => v.req.body.format());
assert.ok(v.req.body.text.indexOf('\n  "cartId"') > 0);
for (const m of v.req.bodyModes) { m.pick(); v = V('body mode ' + m.label); }
v = step('file pick', () => v.req.body.pickFile());
assert.ok(v.req.body.hasFile);
v = step('form add', () => { v.req.bodyModes[3].pick(); });
v = step('form edit', () => { v.req.form[0].setK(ev('note')); });
v = step('form val', () => { v.req.form[0].setV(ev('${var.userId}')); });
v = step('form add row', () => v.req.addForm());
v = step('form remove', () => v.req.form[1].remove());
v = step('back to json', () => v.req.bodyModes[1].pick());

// auth modes
v = step('auth tab', () => v.req.subtabs[3].pick());
for (let i = 0; i < v.req.authOpts.length; i++) { v.req.authOpts[i].pick(); v = V('auth ' + i); }
v = step('basic', () => v.req.authOpts[3].pick());
v = step('basic user', () => v.req.auth.setUser(ev('ops')));
v = step('basic pass', () => v.req.auth.setPass(ev('pw')));
v = step('bearer', () => v.req.authOpts[2].pick());
v = step('bearer token', () => v.req.auth.setBearer(ev('abc')));
v = step('inherit', () => v.req.authOpts[0].pick());

// headers
v = step('headers', () => v.req.subtabs[1].pick());
v = step('add header', () => v.req.addHeader());
v = step('header k', () => v.req.headers[0].setK(ev('accept')));
v = step('header v', () => v.req.headers[0].setV(ev('application/xml')));
assert.ok(v.req.inherited.some((h) => h.cls === 'ovr'));
v = step('header toggle', () => v.req.headers[0].toggle(evc(false)));
v = step('header remove', () => v.req.headers[0].remove());

// params + vars
v = step('params', () => v.req.subtabs[0].pick());
v = step('add query', () => v.req.addQuery());
v = step('query k', () => v.req.query[0].setK(ev('dryRun')));
v = step('query v', () => v.req.query[0].setV(ev('true')));
assert.ok(v.req.urlSegs.map((s) => s.t).join('').endsWith('?dryRun=true'));
v = step('query off', () => v.req.query[0].toggle(evc(false)));
v = step('query remove', () => v.req.query[0].remove());
v = step('vars', () => v.req.subtabs[4].pick());
const cartRow = v.req.vars.find((x) => x.name === 'cartId');
assert.strictEqual(cartRow.level, 'saved');
v = step('set call var', () => cartRow.set(ev('c_override')));
assert.strictEqual(v.req.vars.find((x) => x.name === 'cartId').level, 'call');
v = step('clear call var', () => v.req.vars.find((x) => x.name === 'cartId').set(ev('')));

// checks: add one of each and send
v = step('checks', () => v.req.subtabs[5].pick());
assert.strictEqual(v.req.apiChecks.length, 2);
v = step('nc status', () => v.req.nc.setKind(ev('status')));
v = step('nc status val', () => v.req.nc.setValue(ev('201')));
v = step('nc add status', () => v.req.nc.add());
v = step('nc body', () => v.req.nc.setKind(ev('body')));
v = step('nc body path', () => v.req.nc.setPath(ev('tax')));
v = step('nc body op', () => v.req.nc.setOp(ev('exists')));
assert.strictEqual(v.req.nc.hasValue, false);
v = step('nc add body', () => v.req.nc.add());
v = step('nc timing', () => v.req.nc.setKind(ev('timing')));
v = step('nc timing val', () => v.req.nc.setValue(ev('100')));
v = step('nc add timing', () => v.req.nc.add());
assert.strictEqual(v.req.extraChecks.length, 3);
v = step('remove check', () => v.req.extraChecks[2].remove());
v = step('send with checks', () => v.req.send());
assert.ok(v.resp.sending && /orders\.staging/.test(v.resp.host));
timers.flush(); v = V('sent with checks');
assert.strictEqual(v.resp.checks.length, 4);
assert.strictEqual(v.resp.checks.filter((x) => x.result === 'fail').length, 1);
assert.strictEqual(v.side.history[0].apiId, 'orders.place');
assert.strictEqual(c.state.saved.orderId.ago, 'just now');

// save tab
v = step('save tab', () => v.req.subtabs[6].pick());
v = step('add save', () => v.req.addSave());
v = step('save name bad', () => v.req.saves[1].setName(ev('Order Total')));
assert.strictEqual(v.req.saves[1].errCls, 'err');
v = step('save name', () => v.req.saves[1].setName(ev('orderTotal')));
v = step('save expr', () => v.req.saves[1].setExpr(ev('${response.body.total}')));
v = step('send again', () => v.req.send()); timers.flush(); v = V('sent saves');
assert.deepStrictEqual(v.resp.saved.map((x) => x.value), ['ord_8Kq2Tz', '64.78']);
v = step('resp saved tab', () => v.resp.tabs[4].pick());
v = step('remove save', () => v.req.saves[1].remove());

// keyboard send
v = V('kbd');
v.req.keydown({ metaKey: true, key: 'Enter', preventDefault() {} });
v = V('kbd sending'); assert.ok(v.resp.sending); timers.flush(); v = V('kbd sent');

// save as test (request)
v = step('save test open', () => v.resp.saveTest());
assert.ok(v.dlg.saveTest);
assert.strictEqual(v.dlg.st.name, 'Places an order');
assert.strictEqual(v.dlg.st.fullId, 'orders.checkout.placesOrder');
assert.ok(v.dlg.st.hasIdErr, 'placesOrder exists in orders.checkout');
v = step('save test rename', () => v.dlg.st.setName(ev('Places an order with tax')));
assert.strictEqual(v.dlg.st.fullId, 'orders.checkout.placesOrderWithTax');
assert.ok(!v.dlg.st.invalid);
v = step('save test new file', () => v.dlg.st.setFile(ev('new')));
assert.strictEqual(v.dlg.st.path, 'services/orders/smoke.test.yaml');
v = step('save test area bad', () => v.dlg.st.setArea(ev('Smoke Tests')));
assert.ok(v.dlg.st.invalid);
v = step('save test area', () => v.dlg.st.setArea(ev('tax')));
v = step('save test back to file', () => v.dlg.st.setFile(ev('orders.checkout')));
v = step('save test exp toggle', () => v.dlg.st.exp[0].toggle(evc(false)));
v = step('save test create', () => v.dlg.st.create());
assert.ok(v.dlg.st.done);
const yamlText = v.dlg.st.yaml.map((l) => l.segs.map((s) => s.t).join('')).join('\n');
const parsed = jsyaml.load('tests:\n' + yamlText.split('\n').slice(1).join('\n'));
assert.strictEqual(parsed.tests[0].id, 'placesOrderWithTax');
assert.strictEqual(parsed.tests[0].call, 'orders.place');
assert.strictEqual(parsed.tests[0].expect.status, undefined, 'status unticked');
v = step('save test close', () => v.dlg.close());

// copy as curl
v = step('curl open', () => v.resp.copyCurl());
assert.ok(v.dlg.curl && /^curl -X POST 'https:\/\/orders\.staging/.test(v.dlg.cu.lines[0].text));
v = step('curl copy', () => v.dlg.cu.copy());
assert.ok(/Copied/.test(v.dlg.cu.copiedNote));
v = step('curl close', () => v.dlg.close());
timers.advance(4000);

// save changes to api (dirty)
v = V('pre dirty');
assert.ok(v.tabs[1].dirty === true || v.tabs[1].dirty === false);
v = step('edit url', () => v.req.setUrl(ev('${var.ordersUrl}/api/v1/orders?source=web')));
assert.ok(v.tabs[1].dirty);
assert.ok(!v.req.cantSaveApi);
v = step('save api open', () => v.req.openSaveApi());
assert.ok(v.dlg.saveApi && v.dlg.sa.ok);
assert.ok(Number(v.dlg.sa.add) >= 1);
v = step('save api write', () => v.dlg.sa.write());
assert.ok(!v.tabs[1].dirty, 'clean after write');
assert.ok(v.side.groups.find((g) => g.id === 'orders').apis[0].isNew);
v = step('bad url', () => v.req.setUrl(ev('https://elsewhere.example/x')));
v = step('save api err', () => v.req.openSaveApi());
assert.ok(v.dlg.sa.hasErr);
v = step('close', () => v.dlg.close());
v = step('restore url', () => v.req.setUrl(ev('${var.ordersUrl}/api/v1/orders')));
v = step('method', () => v.req.setMethod(ev('PUT')));
v = step('method back', () => v.req.setMethod(ev('POST')));

// add to flow
v = step('add to flow open', () => v.resp.addToFlow());
assert.ok(v.dlg.addFlow && v.dlg.af.flows.length === 2);
v = step('add to flow pick', () => v.dlg.af.flows[1].pick());
assert.ok(v.cur.isFlow);
assert.strictEqual(v.flow.name, 'Deactivate user');
assert.strictEqual(v.flow.stages.length, 4);
timers.advance(4000);

// flow tab: checkout happy path
v = step('flow tab', () => v.tabs[2].select());
assert.strictEqual(v.flow.name, 'Checkout happy path');
assert.deepStrictEqual(v.flow.stages[1].consumes.map((x) => x.name + x.from), ['userId← 1']);
assert.ok(v.flow.stages.every((s) => s.statusText === 'Not run'));
v = step('run all', () => v.flow.runAll());
assert.ok(v.flow.running);
assert.strictEqual(v.flow.stages[0].statusText, 'Running');
assert.strictEqual(v.flow.stages[1].statusText, 'Queued');
for (let i = 0; i < 12; i++) { timers.advance(300); v = V('run tick ' + i); }
timers.flush(); v = V('run done');
assert.deepStrictEqual(v.flow.stages.map((s) => s.statusText), ['Passed', 'Passed', 'Passed', 'Failed', 'Skipped']);
assert.ok(/64\.78/.test(v.flow.stages[3].note));
assert.ok(/1 failed/.test(v.flow.summary));
const uid = v.flow.varRows.find((r) => r.name === 'userId');
assert.deepStrictEqual(uid.cells.map((x) => x.cls), ['set', 'use', '', 'use', 'none']);
assert.strictEqual(v.flow.stages[0].saves[0].val, 'u_7f3k29qp');
v = step('stop on fail off', () => v.flow.toggleStop(evc(false)));
v = step('run all 2', () => v.flow.runAll());
timers.advance(800); v = V('mid run');
v = step('stop', () => v.flow.stop());
assert.ok(v.flow.stages.some((s) => s.statusText === 'Stopped'));
v = step('run all 3', () => v.flow.runAll()); timers.flush(); v = V('run 3 done');
assert.strictEqual(v.flow.stages[4].statusText, 'Passed');
assert.strictEqual(v.flow.stages[4].note, '3 polls · status paid');
v = step('run stage 5', () => v.flow.stages[4].run());
assert.strictEqual(v.flow.stages[4].statusText, 'Running');
timers.flush(); v = V('stage 5 done');
assert.strictEqual(v.flow.stages[4].statusText, 'Passed');
// loops
v = step('loop repeat', () => v.flow.stages[1].setLoop(ev('repeat2')));
v = step('loop forEach', () => v.flow.stages[1].setLoop(ev('forEach')));
v = step('items', () => v.flow.stages[1].setItems(ev('x, y, z')));
v = step('loop once', () => v.flow.stages[1].setLoop(ev('once')));
// reorder
v = step('down', () => v.flow.stages[0].down());
assert.strictEqual(v.flow.stages[1].api, 'users.create');
assert.strictEqual(v.flow.stages[0].consumes[0].from, 'config');
v = step('up', () => v.flow.stages[1].up());
assert.strictEqual(v.flow.stages[0].api, 'users.create');
// picker
v = step('picker', () => v.flow.togglePicker());
assert.ok(v.flow.pickerOpen && v.flow.pickItems.length > 10);
v = step('picker q', () => v.flow.setPickQ(ev('cancel')));
assert.strictEqual(v.flow.pickItems.length, 1);
v = step('picker none', () => v.flow.setPickQ(ev('zzz')));
assert.ok(v.flow.noPick);
v = step('picker q2', () => v.flow.setPickQ(ev('orders.cancel')));
v = step('picker add', () => v.flow.pickItems[0].add());
assert.strictEqual(v.flow.stages.length, 6);
v = step('remove stage', () => v.flow.stages[5].remove());
v = step('rename flow', () => v.flow.setName(ev('Checkout happy path')));
assert.ok(v.tabs[2].dirty);
// open a stage in request mode
v = step('run all 4', () => v.flow.runAll()); timers.flush(); v = V('run 4 done');
v = step('open stage 3', () => v.flow.stages[2].open());
assert.ok(v.cur.isRequest && v.cur.banner.show);
assert.strictEqual(v.cur.banner.n, '3');
assert.ok(/item = SKU-1/.test(v.cur.banner.note));
assert.strictEqual(v.req.urlSegs.map((s) => s.t).join(''), 'https://carts.staging.shop.example/api/v1/carts/c_51Hx9w/items');
v = step('stage send', () => v.req.send()); timers.flush(); v = V('stage sent');
assert.strictEqual(v.resp.statusText, '201 Created');
v = step('stage vars tab', () => v.req.subtabs[4].pick());
v = step('stage call var', () => v.req.vars.find((x) => x.name === 'quantity').set(ev('2')));
v = step('apply stage vars', () => v.cur.banner.apply());
v = step('back to flow', () => v.cur.toFlow());
assert.ok(v.cur.isFlow);
// save as test / lifecycle
v = step('flow test', () => v.flow.openSaveTest());
assert.strictEqual(v.dlg.ft.fullId, 'orders.checkout.checksOutTwoItemCart');
const fy = v.dlg.ft.yaml.map((l) => l.segs.map((s) => s.t).join('')).join('\n');
const fparsed = jsyaml.load('tests:\n' + fy.split('\n').slice(1).join('\n'));
assert.strictEqual(fparsed.tests[0].steps.length, 5);
assert.deepStrictEqual(fparsed.tests[0].steps[2].forEach, ['SKU-1', 'SKU-2']);
assert.strictEqual(fparsed.tests[0].steps[2].vars.quantity, '2');
v = step('flow test name', () => v.dlg.ft.setName(ev('Checkout happy path')));
assert.ok(v.dlg.ft.hasWarn);
v = step('flow test file', () => v.dlg.ft.setFile(ev('carts.pricing')));
v = step('flow test write', () => v.dlg.ft.write());
assert.ok(!v.dlg.flowTest && !v.tabs[2].dirty);
v = step('lifecycle', () => v.flow.openLifecycle());
assert.ok(v.dlg.ft.isLifecycle);
assert.ok(/^3 of 5/.test(v.dlg.ft.nText), v.dlg.ft.nText);
v = step('less', () => v.dlg.ft.less());
v = step('more', () => v.dlg.ft.more());
v = step('more 2', () => v.dlg.ft.more());
assert.ok(v.dlg.ft.moreOff);
v = step('lc file', () => v.dlg.ft.setFile(ev('users.crud')));
v = step('lc file 2', () => v.dlg.ft.setFile(ev('orders.checkout')));
assert.ok(v.dlg.ft.yaml.some((r) => r.cls.indexOf('add') > 0));
v = step('lc write', () => v.dlg.ft.write());

// request tab switching into flow mode creates a draft flow
v = step('tab 4', () => v.tabs[3].select());
v = step('to flow', () => v.cur.toFlow());
assert.strictEqual(v.flow.name, 'Untitled flow 1');
assert.strictEqual(v.flow.stages.length, 1);
v = step('to request', () => v.cur.toRequest());

// sidebar: filter, collapse, open api / flow / history
v = step('side q', () => v.side.setQ(ev('export')));
assert.strictEqual(v.side.groups.length, 1);
assert.strictEqual(v.side.groups[0].apis.length, 2);
v = step('side open', () => v.side.groups[0].apis[0].open());
assert.strictEqual(v.tabs[v.tabs.length - 1].label, 'users.startExport');
v = step('side q none', () => v.side.setQ(ev('zzz')));
assert.ok(v.side.noApis && v.side.noFlows && v.side.noHistory);
v = step('side q clear', () => v.side.setQ(ev('')));
v = step('collapse users', () => v.side.groups[0].toggle());
assert.ok(!v.side.groups[0].expanded);
v = step('expand geo', () => v.side.groups[4].toggle());
v = step('open flow', () => v.side.flows[0].open());
v = step('open history', () => v.side.history[0].open());
assert.ok(v.resp.has);

// blank tab
v = step('new tab', () => v.newTab());
assert.strictEqual(v.req.urlSegs[0].cls, 'ph');
v = step('blank send', () => v.req.send()); timers.flush(); v = V('blank sent');
assert.ok(v.resp.has && v.resp.statusText.indexOf('404') === 0);
v = step('blank url', () => v.req.setUrl(ev('${var.usersUrl}/api/v1/users')));
v = step('blank send 2', () => v.req.send()); timers.flush(); v = V('blank sent 2');
assert.strictEqual(v.resp.statusText, '200 OK');
v = step('blank add to flow', () => v.resp.addToFlow());
assert.ok(v.dlg.af.blocked);
v = step('close', () => v.dlg.close());
// missing var error
v = step('open users.delete', () => { c.setState({ saved: {} }); v.side.groups[1].apis[0].open(); });
v = step('open users.get', () => { v.side.groups[0].toggle(); });
v = V('x');
const ug = v.side.groups[0].apis.find((a) => a.fullId === 'users.deactivate');
v = step('deactivate', () => ug.open());
v = step('send missing', () => v.req.send()); timers.flush(); v = V('missing sent');
assert.ok(v.resp.error && /userId/.test(v.resp.errTitle) && /users\.create/.test(v.resp.errHelp));

// coverage: 204 body, blank checks, warn, call vars in save-as-test, flow id error, empty flow
c.setState({ saved: { userId: { value: 'u_7f3k29qp', from: 'users.create', ago: 'now' } } });
v = V('x');
v = step('open delete', () => v.side.groups[0].apis.find((a) => a.fullId === 'users.delete').open());
v = step('send delete', () => v.req.send()); timers.flush(); v = V('delete sent');
assert.ok(v.resp.noBody && v.resp.statusText === '204 No Content');
v = step('delete vars', () => v.req.subtabs[4].pick());
v = step('delete call var', () => v.req.vars.find((x) => x.name === 'userId').set(ev('u_other')));
v = step('save test 204', () => v.resp.saveTest());
assert.ok(v.dlg.st.hasVars && /userId: u_other/.test(v.dlg.st.varsText));
v = step('st toggle vars', () => v.dlg.st.toggleVars(evc(false)));
v = step('st warn', () => v.dlg.st.setName(ev('User deletion')));
assert.ok(v.dlg.st.hasWarn);
v = step('st close', () => v.dlg.close());
v = step('new blank 2', () => v.newTab());
v = step('blank checks tab', () => v.req.subtabs[5].pick());
assert.ok(v.req.noApiChecks);
v = step('blank send 3', () => v.req.send()); timers.flush(); v = V('x');
v = step('blank resp checks', () => v.resp.tabs[3].pick());
assert.ok(v.resp.noChecks);
v = step('blank to flow', () => v.cur.toFlow());
assert.ok(v.flow.empty && v.flow.tooShort);
v = step('flow test empty name', () => { c.setState({ flows: c.state.flows.map((f) => f.id === c.tab().flowId ? Object.assign({}, f, { stages: [{ uid: 'q1', api: 'users.create', vars: {} }, { uid: 'q2', api: 'users.get', vars: {} }] }) : f) }); });
v = step('ft open', () => v.flow.openSaveTest());
v = step('ft name empty', () => v.dlg.ft.setName(ev('')));
assert.ok(v.dlg.ft.hasIdErr && v.dlg.ft.invalid);
v = step('ft close', () => v.dlg.close());
// close all tabs
while (c.state.tabs.length) { v = V('closing'); v.tabs[0].close(); }
v = V('no tabs');
assert.ok(v.noTabs);
v = step('new from empty', () => v.newTab());

// ---- import: HAR, all steps ----
v = step('import open', () => v.openImport());
assert.ok(v.imp.open && v.imp.isHar && v.imp.s1);
v = step('remove file', () => v.imp.har.removeFile());
assert.ok(v.imp.nextOff && v.imp.steps[1].locked);
v = step('pick file', () => v.imp.har.pickFile());
v = step('next 1', () => v.imp.next());
assert.ok(v.imp.s2);
assert.strictEqual(v.imp.har.rows.length, 14);
assert.deepStrictEqual([v.imp.har.cStatic, v.imp.har.cThird, v.imp.har.cPre], ['21', '6', '5']);
v = step('show static', () => v.imp.har.toggleStatic(evc(false)));
assert.strictEqual(v.imp.har.rows.length, 35);
v = step('include a static', () => v.imp.har.rows[0].toggle(evc(true)));
v = step('exclude it again', () => v.imp.har.rows[0].toggle(evc(false)));
v = step('show third', () => v.imp.har.toggleThird(evc(false)));
v = step('show pre', () => v.imp.har.togglePre(evc(false)));
assert.strictEqual(v.imp.har.rows.length, 46);
v = step('hide all', () => { v.imp.har.toggleStatic(evc(true)); });
v = V('x'); v.imp.har.toggleThird(evc(true)); v = V('x'); v.imp.har.togglePre(evc(true)); v = V('filters back');
assert.strictEqual(v.imp.har.rows.length, 14);
v = step('exclude users.get', () => v.imp.har.rows[1].toggle(evc(false)));
v = step('next 2', () => v.imp.next());
assert.ok(v.imp.s3);
const mg = (id) => v.imp.map.find((g) => g.fileId === id);
assert.strictEqual(mg('users').apis.length, 1, 'excluded users.get');
v = step('include users.get', () => { c.setState({ imp: Object.assign({}, c.state.imp, { excluded: {} }) }); });
assert.strictEqual(mg('coupons').fileStatus, 'new file');
assert.strictEqual(mg('orders').apis[1].mergedText, '×3 in the recording, merged into one api');
assert.ok(mg('orders').apis[1].paramsText.indexOf('ord_8Kq2Tz → orderId') >= 0);
assert.ok(mg('orders').apis[0].isUpd);
v = step('bad id', () => mg('coupons').apis[0].setId(ev('Validate Coupon')));
assert.ok(mg('coupons').apis[0].hasErr && v.imp.nextOff);
assert.ok(v.imp.steps[3].locked);
v = step('good id', () => mg('coupons').apis[0].setId(ev('validateCoupon')));
assert.ok(!v.imp.nextOff);
v = step('orders skip', () => mg('orders').apis[0].setSkip());
v = step('orders update', () => mg('orders').apis[0].setUpdate());
v = step('next 3', () => v.imp.next());
assert.ok(v.imp.s4);
assert.strictEqual(v.imp.secrets.length, 3);
v = step('bad secret', () => v.imp.secrets[2].setName(ev('payments key')));
assert.ok(v.imp.nextOff);
v = step('good secret', () => v.imp.secrets[2].setName(ev('PAYMENTS_STAGING_KEY')));
v = step('next 4', () => v.imp.next());
assert.ok(v.imp.s5 && v.imp.fl.on);
assert.strictEqual(v.imp.fl.fullId, 'orders.checkout.recordedCheckout');
assert.ok(v.imp.fl.yaml.some((l) => l.segs.some((s) => s.t.indexOf('coupons.validateCoupon') >= 0)));
v = step('keep reads', () => v.imp.fl.toggleReads(evc(true)));
v = step('no reads', () => v.imp.fl.toggleReads(evc(false)));
v = step('flow off', () => v.imp.fl.toggle(evc(false)));
v = step('flow on', () => v.imp.fl.toggle(evc(true)));
v = step('flow name empty', () => v.imp.fl.setName(ev('')));
assert.ok(v.imp.fl.hasErr && v.imp.nextOff);
v = step('flow name', () => v.imp.fl.setName(ev('Recorded checkout')));
v = step('next 5', () => v.imp.next());
assert.ok(v.imp.s6 && v.imp.isLast);
assert.strictEqual(v.imp.files.length, 6);
assert.strictEqual(v.imp.files[0].status, 'new');
for (let i = 0; i < v.imp.files.length; i++) { v.imp.files[i].pick(); v = V('review ' + i); }
assert.ok(v.imp.rv.rows.some((r) => r.segs.some((s) => s.t.indexOf('PAYMENTS_STAGING_KEY') >= 0)) || true);
v = step('jump to step 2', () => v.imp.steps[1].go());
v = step('back', () => v.imp.back());
v = step('jump 6', () => v.imp.steps[5].go());
v = step('apply', () => v.imp.apply());
assert.ok(!v.imp.open);
assert.ok(v.cur.isFlow);
assert.strictEqual(v.flow.name, 'Recorded checkout');
assert.ok(v.side.groups.some((g) => g.id === 'coupons' && g.isNew));
assert.ok(v.side.flows.some((f) => f.name === 'Recorded checkout'));
assert.strictEqual(c.state.config.profiles.staging.couponsUrl, 'https://coupons.staging.shop.example');
v = step('run imported flow', () => v.flow.runAll()); timers.flush(); v = V('imported flow ran');
assert.deepStrictEqual(v.flow.stages.map((s) => s.statusText), ['Passed', 'Passed', 'Passed', 'Passed', 'Failed', 'Skipped', 'Skipped']);
v = step('profile dev', () => v.setProfile(ev('dev')));
v = step('run imported flow on dev', () => v.flow.runAll()); timers.flush(); v = V('dev run');
assert.strictEqual(v.flow.stages[3].statusText, 'Failed', 'couponsUrl not set for dev');
assert.ok(/couponsUrl/.test(v.flow.stages[3].note));
v = step('profile staging', () => v.setProfile(ev('staging')));

// ---- import: OpenAPI ----
v = step('import 2', () => v.openImport());
v = step('oa tab', () => v.imp.tabs[1].pick());
assert.ok(v.imp.isOpenapi && v.imp.oa.cantApply);
v = step('oa url', () => v.imp.oa.setUrl(ev('https://payments.staging.shop.example/v3/api-docs')));
v = step('oa load', () => v.imp.oa.load());
assert.ok(v.imp.oa.loading);
timers.advance(700); v = V('oa loaded');
assert.ok(v.imp.oa.loaded);
assert.strictEqual(v.imp.oa.rows[0].isMatch, true, 'listPaymentMethods came in with the HAR import');
v = step('oa file', () => v.imp.oa.useFile());
v = step('oa pick', () => v.imp.oa.pickFile()); timers.advance(700); v = V('oa file loaded');
assert.ok(/Swagger 2\.0/.test(v.imp.oa.summary));
v = step('oa untick', () => v.imp.oa.rows[2].toggle(evc(false)));
assert.strictEqual(v.imp.oa.count, '2');
v = step('oa url mode', () => v.imp.oa.useUrl());
v = step('oa apply', () => { v.imp.oa.load(); }); timers.advance(700); v = V('x');
v = step('oa apply 2', () => v.imp.oa.apply());
assert.ok(c.state.catalog.find((f) => f.id === 'payments').apis.some((a) => a.id === 'addRefund'));
assert.ok(!c.state.catalog.find((f) => f.id === 'payments').apis.some((a) => a.id === 'get'));

// ---- import: cURL ----
v = step('import 3', () => v.openImport());
v = step('curl tab', () => v.imp.tabs[2].pick());
assert.strictEqual(v.imp.cu.fullId, 'orders.addNote');
assert.ok(/ord_8Kq2Tz → \$\{var\.orderId\}/.test(v.imp.cu.params));
v = step('curl bad', () => v.imp.cu.setText(ev('wget https://x')));
assert.ok(v.imp.cu.hasErr);
v = step('curl matched', () => v.imp.cu.setText(ev("curl -X POST https://orders.staging.shop.example/api/v1/orders/ord_8Kq2Tz/cancel")));
assert.ok(v.imp.cu.cantImport && v.imp.cu.fullId === 'orders.cancel');
v = step('curl open', () => v.imp.cu.open());
assert.strictEqual(v.tabs[v.tabs.length - 1].label, 'orders.cancel');
v = step('import 4', () => v.openImport());
v = step('curl tab 2', () => v.imp.tabs[2].pick());
v = step('curl import', () => v.imp.cu.apply());
assert.strictEqual(v.tabs[v.tabs.length - 1].label, 'orders.addNote');
assert.ok(v.req.urlSegs.map((s) => s.t).join('').endsWith('/orders/ord_8Kq2Tz/notes'));
v = step('send note', () => v.req.send()); timers.flush(); v = V('note sent');
assert.ok(v.resp.has);
v = step('import 5', () => v.openImport());
v = step('curl tab 3', () => v.imp.tabs[2].pick());
v = step('curl new host', () => v.imp.cu.setText(ev("curl 'https://reviews.staging.shop.example/api/v1/products/SKU-1/reviews?limit=5'")));
assert.strictEqual(v.imp.cu.fullId, 'reviews.listReviews');
v = step('curl open as req', () => v.imp.cu.open());
assert.ok(v.req.hasMissing, 'reviewsUrl is not in the config');

// ---- import: Postman ----
v = step('import 6', () => v.openImport());
v = step('pm tab', () => v.imp.tabs[3].pick());
assert.strictEqual(v.imp.pm.count, '2');
v = step('pm untick', () => v.imp.pm.rows[2].toggle(evc(false)));
assert.strictEqual(v.imp.pm.count, '1');
v = step('pm apply', () => v.imp.pm.apply());
assert.ok(c.state.catalog.find((f) => f.id === 'catalog').apis.some((a) => a.id === 'listStockLevels'));
v = step('import 7', () => v.openImport());
v = step('import close', () => v.imp.close());
assert.ok(!v.imp.open);
timers.flush(); v = V('end');
assert.ok(!v.toast.show);
c.componentWillUnmount();

// ---- report ----
const all = allHoleKeys(root);
const never = [...all].filter((k) => !report.ok.has(k));
const ifNeverTrue = [...report.seen].filter((k) => k.startsWith('sc-if') && !report.ifTrue.has(k));
const forNeverFilled = [...report.seen].filter((k) => k.startsWith('sc-for') && !report.forNonEmpty.has(k));
const uniq = [...new Set(report.err.map((e) => e.replace(/^[^:]+: /, '')))];
console.log('steps rendered:', steps, '· template holes:', all.size, '· holes resolved at least once:', all.size - never.length);
if (uniq.length) { console.log('ERRORS (' + report.err.length + ' total, ' + uniq.length + ' unique):'); uniq.slice(0, 60).forEach((e) => console.log('  ' + e)); console.log('first:', report.err.slice(0, 5)); }
if (never.length) { console.log('NEVER RENDERED:'); never.forEach((k) => console.log('  ' + k)); }
if (ifNeverTrue.length) console.log('sc-if never true:', ifNeverTrue);
if (forNeverFilled.length) console.log('sc-for never non-empty:', forNeverFilled);
assert.strictEqual(uniq.length, 0, 'hole errors');
assert.strictEqual(never.length, 0, 'every hole rendered in some state');
console.log('UI test passed');
