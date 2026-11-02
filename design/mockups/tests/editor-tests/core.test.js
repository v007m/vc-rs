const H = require('./harness');
const yaml = require(H.SCR + '/npmtmp/package');
const C = H.load();
let pass = 0, fail = 0;
function ok(cond, name, extra) { if (cond) pass++; else { fail++; console.log('FAIL', name, extra !== undefined ? JSON.stringify(extra).slice(0, 400) : ''); } }
function eq(a, b, name) { ok(a === b, name, a === b ? undefined : { got: a, want: b }); }
const SEED = C.SEED || null;
const c0 = H.mount(C);
const files = {}; Object.keys(c0.state.files).forEach(p => files[p] = c0.state.files[p].text);
const parse = t => C.yParseSafe(t);
const ctx = C.buildCtx(files, parse);
const CO = 'services/orders/checkout.test.yaml', UC = 'services/users/crud.test.yaml';
function diags(p, text) { const k = C.fileKind(p); const f = Object.assign({}, files, { [p]: text }); const cx = C.buildCtx(f, parse); const P = k === 'schema' ? C.jsonParse(text) : k === 'dataset' ? C.csvParse(text) : C.yParseSafe(text); return C.diagnose(p, k, text, P, cx); }
function has(D, code, sev) { return D.find(d => d.code === code && (!sev || d.sev === sev)); }

// ---------- 1. parser parity and serializer identity ----------
for (const [p, t] of Object.entries(files)) {
  const k = C.fileKind(p); if (!C.isYamlKind(k)) continue;
  const r = parse(t);
  ok(r.ok && C.deepEq(r.value, yaml.load(t, { schema: yaml.CORE_SCHEMA })), 'parser parity ' + p);
  if (k !== 'openapi') eq(C.yDump(r.value, k, r.comments), t, 'dump(parse(seed)) is byte-identical: ' + p);
}
const extra = [
  'a: {x: 1, "y": [1, 2, "three"]}\nb: \'it\'\'s\'\nc: "tab\\there"\nd: |\n  line one\n  line two\ne: >-\n  folded\n  text\nf: ~\ng: -12.5\nh: 0x1F\ni: "2026-11-15"\n',
  '- a\n- - b\n  - c\n- k: v\n  l:\n    - 1\n# trailing comment\n',
  'key:\n- one\n- two\nother: x  # note\n',
  'url: http://x.example/a#frag\ntpl: ${var.x | default(\'y\')}\nq: "@example.com"\n'
];
extra.forEach((t, i) => { const r = parse(t); ok(r.ok && C.deepEq(r.value, yaml.load(t, { schema: yaml.CORE_SCHEMA })), 'parser parity extra ' + i, r.ok ? [r.value, yaml.load(t, { schema: yaml.CORE_SCHEMA })] : r.error); });
['a: b: c\n', 'a:\n  b: 1\n c: 2\n', 'a: [1, 2\n', 'a: "x\n', 'a: 1\na: 2\n', 'x:\n\t- 1\n'].forEach((t, i) => {
  const r = parse(t); let j = null; try { yaml.load(t); } catch (e) { j = e.mark.line + 1; }
  ok(!r.ok && j !== null, 'both parsers reject bad yaml ' + i, { mine: r.error, js: j });
});

// ---------- 2. form -> text -> model round trip ----------
function mutateAll(p, fn, label) {
  const t = files[p], k = C.fileKind(p), m = C.clone(parse(t).value); fn(m);
  const nt = C.yApplyModel(t, k, m), r = parse(nt);
  ok(r.ok && C.deepEq(r.value, m), 'round trip model ' + label);
  eq(C.yApplyModel(nt, k, r.value), nt, 'idempotent text ' + label);
  eq(C.yDump(parse(C.yDump(m, k)).value, k), C.yDump(m, k), 'stable serializer ' + label);
  ok(C.deepEq(yaml.load(nt, { schema: yaml.CORE_SCHEMA }), m), 'js-yaml agrees ' + label);
  return { nt, ops: C.diffStats(C.lineDiff(t, nt)) };
}
let x;
x = mutateAll(CO, m => { m.tests[0].name = 'A cart with two lines becomes a paid order'; }, 'rename test');
ok(x.ops.add === 1 && x.ops.del === 1, 'scalar edit touches one line', x.ops);
x = mutateAll(CO, m => { m.tests[0].expect.status = [200, 201]; }, 'status list');
ok(x.ops.add === 1 && x.ops.del === 1 && x.nt.indexOf('      status: [200, 201]\n') > 0, 'flow list edit in place', x.ops);
x = mutateAll(CO, m => { m.tests[0].retry = { attempts: 2, delay: '1s' }; }, 'add retry');
ok(x.ops.del === 0 && x.ops.add === 3, 'adding retry only adds 3 lines', x.ops);
ok(x.nt.indexOf('      custom:\n        - function: ../../hooks/checks.ts#currencyIsConsistent\n    retry:\n      attempts: 2\n      delay: 1s\n  - id: rejectsDeclinedCard') > 0, 'retry placed in house order after expect');
x = mutateAll(CO, m => { m.tests[0].steps.push({ id: 'cancel', call: 'orders.cancel' }); }, 'add step');
ok(x.ops.del === 0 && x.ops.add === 2, 'adding a step adds 2 lines', x.ops);
x = mutateAll(CO, m => { m.tests.splice(2, 1); }, 'delete test');
ok(x.ops.add === 0 && x.ops.del === 12, 'deleting a test removes only its 12 lines', x.ops);
x = mutateAll(CO, m => { const t = m.tests[0]; m.tests[0] = m.tests[1]; m.tests[1] = t; }, 'swap tests');
x = mutateAll(CO, m => { m.tests[1].vars = C.renameKey(m.tests[1].vars, 'paymentToken', 'cardToken'); }, 'rename var key');
ok(x.ops.add === 1 && x.ops.del === 1, 'renaming a key edits one line', x.ops);
x = mutateAll(CO, m => { delete m.tests[0].expect.cel; }, 'delete cel');
ok(x.ops.add === 0 && x.ops.del === 2, 'deleting cel removes 2 lines', x.ops);
x = mutateAll(CO, m => { m.afterAll = [{ command: './scripts/print-order-stats.sh' }]; }, 'add afterAll');
ok(x.nt.indexOf('\nafterAll:\n  - command: ./scripts/print-order-stats.sh\n\ntests:') > 0 && x.ops.del === 0, 'afterAll inserted before tests with blank line', x.ops);
ok(x.nt.indexOf('# shared by the file') > 0 && x.nt.indexOf('# every test gets its own buyer and cart') > 0, 'comments survive structural edits');
x = mutateAll(CO, m => { m.tests[0].steps[1].poll.until.body.match.status = { in: ['paid', 'settled'] }; }, 'matcher object');
x = mutateAll('vibranium.config.yaml', m => { m.profiles[1].vars.geoUrl = 'https://geo.staging.shop.example'; }, 'config matrix cell');
ok(x.ops.add === 1 && x.ops.del === 0, 'matrix cell adds one line', x.ops);
x = mutateAll('vibranium.config.yaml', m => { delete m.profiles[0].default; m.profiles[1] = C.insertKeyAfter(m.profiles[1], 'id', 'default', true); }, 'default radio');
x = mutateAll('load/checkout.load.yaml', m => { m.mix[0].weight = 4; m.stages.push({ duration: '30s', users: 0 }); }, 'load edits');
x = mutateAll('services/users/users.api.yaml', m => { m.apis[0].expect.headers = { 'content-type': { contains: 'json' } }; }, 'api header check');
x = mutateAll(UC, m => { m.tests[2].examples.push({ email: 'two@@example.com' }); }, 'examples row');
x = mutateAll(UC, m => { m.tests[0].body = 'raw\ntext body\n'; }, 'multiline string');
// user-formatted (non house style) text keeps its formatting on scalar edits
const odd = 'id:   orders.checkout   # file id\nname: Checkout\ntags: [ orders , checkout ]\ntests:\n- id: placesOrder\n  call: orders.place\n  expect: {status: 201}\n';
const om = C.clone(parse(odd).value); om.name = 'Checkout v2';
const ot = C.yApplyModel(odd, 'test', om);
eq(ot, odd.replace('name: Checkout', 'name: Checkout v2'), 'scalar edit keeps foreign formatting');
const om2 = C.clone(parse(odd).value); om2.tests[0].expect.status = 200;
ok(C.deepEq(parse(C.yApplyModel(odd, 'test', om2)).value, om2), 'edit inside a flow map is correct');

// ---------- 3. diff ----------
const A = 'a\nb\nc\nd\ne\n';
let s = C.diffStats(C.lineDiff(A, 'a\nb\nX\nd\ne\n')); ok(s.add === 1 && s.del === 1, 'diff replace', s);
s = C.diffStats(C.lineDiff(A, 'a\nb\n1\n2\n3\nc\nd\ne\n')); ok(s.add === 3 && s.del === 0, 'diff insert 3', s);
s = C.diffStats(C.lineDiff(A, 'a\nd\ne\n')); ok(s.add === 0 && s.del === 2, 'diff delete 2', s);
s = C.diffStats(C.lineDiff(A, A)); ok(s.add === 0 && s.del === 0, 'diff none');
const ops = C.lineDiff(A, 'a\nb\nX\nd\ne\n');
ok(ops.find(o => o.t === '-' && o.a === 3 && o.text === 'c') && ops.find(o => o.t === '+' && o.b === 3 && o.text === 'X'), 'diff line numbers');
const sbs = C.diffSideBySide(ops); ok(sbs.some(r => r.l && r.r && r.l.text === 'c' && r.r.text === 'X'), 'side by side pairs change');

// ---------- 4. diagnostics fire, quick fixes apply ----------
const co = files[CO];
let D, d, t2;
D = diags(CO, co); eq(D.length, 0, 'checkout opens clean');
D = diags(UC, files[UC]); ok(D.length === 1 && D[0].code === 'skip-expired' && D[0].sev === 'warn', 'stale skip warning on users.crud', D);
eq(C.applyFix(files[UC], 'test', D[0].fixes[1]), files[UC].replace('until: 2026-10-20', 'until: 2026-11-06'), 'fix: extend skip');
t2 = C.applyFix(files[UC], 'test', D[0].fixes[0]);
ok(t2.indexOf('skip:') < 0 && C.diffStats(C.lineDiff(files[UC], t2)).del === 3, 'fix: remove skip removes 3 lines');
D = diags(CO, co.replace('      - id: place\n        call: orders.place', '      - id: place\n        call: orders.plce'));
d = has(D, 'unknown-api'); ok(d && /did you mean orders\.place/.test(d.msg), 'unknown api did-you-mean', D);
eq(C.applyFix(co.replace('call: orders.plce', 'call: orders.plce').replace('        call: orders.place\n      - id: fetch', '        call: orders.plce\n      - id: fetch'), 'test', d.fixes[0]), co, 'fix: unknown api');
D = diags(CO, co.replace('total: ${var.orderTotal}', 'total: ${var.orderTotl}'));
d = has(D, 'var-unset'); ok(d && /orderTotal/.test(d.msg) && d.line === co.split('\n').findIndex(l => l.includes('total: ${var.orderTotal}')), 'var typo did-you-mean on its line', d);
eq(C.applyFix(co.replace('total: ${var.orderTotal}', 'total: ${var.orderTotl}'), 'test', d.fixes[0]), co, 'fix: var typo');
D = diags(CO, co.replace('total: ${var.orderTotal}', 'total: ${var.discount}'));
d = has(D, 'var-unset'); ok(d && /never set/.test(d.msg) && d.fixes.some(f => f.type === 'addVar'), 'var never set with add-var fix', d);
t2 = C.applyFix(co.replace('total: ${var.orderTotal}', 'total: ${var.discount}'), 'test', d.fixes.find(f => f.type === 'addVar'));
ok(diags(CO, t2).length === 0 && /    vars:\n      discount: ""\n/.test(t2), 'add-var fix clears the error', diags(CO, t2));
D = diags(CO, co.replace('      - id: place\n        call: orders.place\n', ''));
d = D.find(x => x.code === 'var-unset' && /orders\.get needs/.test(x.msg)); ok(d && /orders\.place saves it/.test(d.msg), 'api needs var set by an earlier step', D);
D = diags(CO, co.replace('  - id: rejectsDeclinedCard', '  - id: placesOrder'));
d = has(D, 'dup-id'); ok(d, 'duplicate test id', D);
ok(diags(CO, C.applyFix(co.replace('  - id: rejectsDeclinedCard', '  - id: placesOrder'), 'test', d.fixes[0])).length === 0, 'fix: duplicate id');
D = diags(CO, co.replace('  - id: memberCannotRefund', '  - id: Member_cannot_refund'));
d = has(D, 'id-case'); ok(d && d.fixes[0].to === 'memberCannotRefund', 'id not camelCase', D);
eq(C.applyFix(co.replace('  - id: memberCannotRefund', '  - id: Member_cannot_refund'), 'test', d.fixes[0]), co, 'fix: rename id');
D = diags(CO, co.replace('      - id: place\n', '      - id: Place_it\n').replace('${var.orderTotal}', '${Place_it.body.total}'));
d = has(D, 'id-case'); t2 = C.applyFix(co.replace('      - id: place\n', '      - id: Place_it\n').replace('${var.orderTotal}', '${Place_it.body.total}'), 'test', d.fixes[0]);
ok(t2.indexOf('${placeIt.body.total}') > 0 && diags(CO, t2).length === 0, 'rename fix updates step references');
D = diags(CO, co.replace('      reason: "coupon service flaky on staging, see #212"\n', ''));
d = has(D, 'skip-reason'); ok(d && d.fixes[0].type === 'jump', 'skip without reason', D);
D = diags(CO, co.replace('until: 2026-11-15', 'until: 2026-10-01')); ok(has(D, 'skip-expired', 'warn'), 'skip expired');
D = diags(CO, co.replace('      status: 402', '      status: 4020'));
d = has(D, 'status'); ok(d && d.fixes[0].value === 402, 'invalid status with fix', D);
eq(C.applyFix(co.replace('      status: 402', '      status: 4020'), 'test', d.fixes[0]), co, 'fix: status');
['200x', 'ok', 700, [], '6xx'].forEach(v => ok(!C.validStatus(v), 'invalid status ' + JSON.stringify(v)));
[200, '2xx', [200, 204], { in: [200, 201] }].forEach(v => ok(C.validStatus(v), 'valid status ' + JSON.stringify(v)));
D = diags(CO, co.replace('../../schemas/orders/order.json', '../../schemas/order.json'));
d = has(D, 'schema'); ok(d && /schemas\/order\.json/.test(d.msg) && d.fixes.length, 'schema not found with fix', D);
eq(C.applyFix(co.replace('../../schemas/orders/order.json', '../../schemas/order.json'), 'test', d.fixes[0]), co, 'fix: schema path');
D = diags(CO, co.replace('../../schemas/orders/order.json', '../../openapi/orders.yaml#/components/schemas/Order')); eq(D.length, 0, 'snapshot schema ref accepted');
D = diags(CO, co.replace('../../schemas/orders/order.json', '../../openapi/orders.yaml#/components/schemas/Ordr')); ok(has(D, 'schema'), 'snapshot schema ref checked');
const putTxt = co.replace('  - id: memberCannotRefund', '  - id: removesCatalogProduct\n    name: Removing a seeded product\n    call: users.delete\n    vars:\n      userId: ${var.catalog.productIds[0]}\n  - id: memberCannotRefund');
D = diags(CO, putTxt); d = has(D, 'shared-write', 'warn'); ok(d && /DELETE/.test(d.msg), 'DELETE with beforeAll value warns', D);
D = diags(CO, co.replace('    expect:\n      status: 403', '    expct:\n      status: 403'));
d = has(D, 'unknown-key'); ok(d && d.fixes[0].to === 'expect', 'unknown key with rename', D);
eq(C.applyFix(co.replace('    expect:\n      status: 403', '    expct:\n      status: 403'), 'test', d.fixes[0]), co, 'fix: rename key');
D = diags(CO, co.replace('tags: [orders, checkout]', 'tags: [orders, Slow_Network]')); d = has(D, 'tag'); ok(d && d.fixes[0].value === 'slow-network', 'tag case');
D = diags(CO, co.replace('id: orders.checkout', 'id: orders.check')); ok(has(D, 'file-id', 'warn'), 'file id vs path');
D = diags(CO, co.replace("      sku: ${item}", "      sku: ${item}\n      size: ${item.size}").replace('    call: orders.refund', '    call: orders.refund\n    body:\n      reason: ${item}')); ok(has(D, 'tpl'), '${item} outside forEach');
D = diags(CO, co.replace('call: orders.refund', 'call: orders.refund\n    auth: partnerLogn').replace("    auth:\n      id: partnerLogin\n      vars:\n        username: ${secret.MEMBER_USER}\n        password: ${secret.MEMBER_PASSWORD}\n", '')); ok(has(D, 'auth'), 'unknown auth id');
D = diags(CO, 'id: orders.checkout\ntests:\n  - id: x\n    call: [unclosed\n'); d = has(D, 'syntax'); ok(d && d.line === 3, 'syntax error with line', D);
D = diags('services/users/users.api.yaml', files['services/users/users.api.yaml'].replace('  - id: startExport', '  - id: usersStartExport')); d = has(D, 'id-prefix'); ok(d && d.fixes[0].to === 'startExport', 'api service prefix', D);
D = diags('services/users/users.api.yaml', files['services/users/users.api.yaml'].replace('call: delete', 'call: delet')); ok(has(D, 'unknown-api'), 'cleanup unknown api');
D = diags('load/checkout.load.yaml', files['load/checkout.load.yaml'].replace('catalog.browse.searchesProducts', 'catalog.browse.searchProducts')); d = has(D, 'unknown-test'); ok(d && /searchesProducts/.test(d.msg), 'load unknown test');
D = diags('vibranium.config.yaml', files['vibranium.config.yaml'].replace('  - id: staging\n', '  - id: staging\n    default: true\n')); ok(has(D, 'profiles', 'error'), 'two default profiles');
D = diags('datasets/capitals.csv', files['datasets/capitals.csv'].replace('Paris,FR,2102650', 'Paris,FR')); d = has(D, 'csv'); ok(d && d.line === 3, 'ragged csv row line', d);
D = diags('schemas/orders/order.json', files['schemas/orders/order.json'].replace('"title": "Order",', '"title": "Order"')); d = has(D, 'syntax'); ok(d && d.line === 3, 'json error line (0-based)', d);

// ---------- 5. completion ----------
let comp;
const tc = co.replace('      - id: place\n        call: orders.place', '      - id: place\n        call: ord');
comp = C.yamlCompletion(tc, tc.indexOf('call: ord') + 9, { fpath: CO, kind: 'test', ctx, model: parse(co).value, index: parse(co).index });
ok(comp && comp.items[0].label.indexOf('orders.') === 0 && comp.items.some(i => i.label === 'orders.place' && i.detail === 'POST /orders'), 'call: completion with method and path', comp && comp.items.map(i => i.label));
const tv = co.replace('total: ${var.orderTotal}', 'total: ${var.');
const cpos = tv.indexOf('total: ${var.') + 13;
comp = C.yamlCompletion(tv, cpos, { fpath: CO, kind: 'test', ctx, model: parse(co).value, index: parse(co).index });
const lbl = comp ? comp.items.map(i => i.label + ' | ' + i.detail) : [];
ok(lbl.some(l => /^orderTotal \| saved by step place \(orders\.place\)/.test(l)) && /^order(Id|Total)/.test(lbl[0]), 'var completion: nearest first, saved by step', lbl);
const all = C.scopeItems(C.scopeForPath('test', parse(co).value, ctx, ['tests', 0, 'expect', 'body', 'match', 'total']), ctx).map(i => i.label + ' | ' + i.detail);
ok(all.some(l => l === 'cartId | saved by carts.create (beforeEach)') && all.some(l => l === 'catalog.productIds | saved by catalog.seedProducts, shared') && all.some(l => l === 'ordersUrl | config vars'), 'var scope lists sources', all);
comp = C.tplCompletion('x ${', { scope: [], ctx, ids: ['place'] }); ok(comp && comp.items.some(i => i.label === 'var.') && comp.items.some(i => i.label === 'place.'), 'namespace completion');
const tm = co.replace('          items:\n            length: 2', '          items:\n            len');
comp = C.yamlCompletion(tm, tm.indexOf('            len') + 15, { fpath: CO, kind: 'test', ctx, model: parse(co).value, index: parse(co).index });
ok(comp && comp.items[0].label === 'length' && comp.items[0].insert === 'length: ', 'matcher completion under match', comp && comp.items);
const tk = co.replace('    name: A declined card returns 402', '    name: A declined card returns 402\n    ret');
comp = C.yamlCompletion(tk, tk.indexOf('    ret') + 7, { fpath: CO, kind: 'test', ctx, model: parse(co).value, index: parse(co).index });
ok(comp && comp.items.some(i => i.label === 'retry' && i.insert === 'retry: '), 'test key completion', comp && comp.items);
comp = C.fieldCompletion('${var.ord', 9, 'tpl', { ctx, scope: C.scopeForPath('test', parse(co).value, ctx, ['tests', 0, 'expect']) });
ok(comp && comp.items.some(i => i.label === 'orderId') && comp.from === 6, 'form field var completion', comp);

// ---------- 6. naming conventions ----------
const nw = { kind: 'test', name: 'Rejects a declined card', idEdited: false, file: CO, service: 'orders' };
let plan = C.planNew(nw, c0.state.files, ctx);
ok(plan.ok === false && plan.errors.some(e => /already exists/.test(e)), 'rejectsDeclinedCard already exists', plan.errors);
plan = C.planNew(Object.assign({}, nw, { name: 'Rejects an expired coupon' }), c0.state.files, ctx); ok(plan.ok && plan.id === 'rejectsExpiredCoupon' && plan.full === 'orders.checkout.rejectsExpiredCoupon', 'derived test id', plan);
plan = C.planNew(Object.assign({}, nw, { id: 'Test_1', idEdited: true }), c0.state.files, ctx); ok(!plan.ok && plan.sugg === 'test1', 'Test_1 refused with suggestion test1', plan);
plan = C.planNew({ kind: 'api', name: 'Start an export', service: 'users', method: 'POST', apiPath: '/x', idEdited: false }, c0.state.files, ctx); ok(!plan.ok && /startExport already exists/.test(plan.errors.join()), 'api id clash', plan.errors);
plan = C.planNew({ kind: 'api', name: 'List invoices', service: '+new', newService: 'Invoices', method: 'GET', apiPath: '/invoices', idEdited: false }, c0.state.files, ctx); ok(plan.ok && plan.path === 'services/invoices/invoices.api.yaml' && plan.full === 'invoices.listInvoices', 'new service api path', plan);
plan = C.planNew({ kind: 'testFile', service: 'orders', area: 'History', name: 'Order history' }, c0.state.files, ctx); ok(plan.ok && plan.path === 'services/orders/history.test.yaml' && plan.id === 'orders.history', 'test file path and id', plan);
plan = C.planNew({ kind: 'load', area: 'Checkout', scenario: 'peak', name: 'x' }, c0.state.files, ctx); ok(!plan.ok && /already exists/.test(plan.errors.join()), 'load path exists', plan);
plan = C.planNew({ kind: 'load', area: 'search', scenario: 'Spike test', name: 'x' }, c0.state.files, ctx); ok(plan.ok && plan.path === 'load/search.load.yaml' && plan.id === 'search.spikeTest', 'load id', plan);
plan = C.planNew({ kind: 'schema', service: 'orders', name: 'Line item' }, c0.state.files, ctx); ok(plan.ok && plan.path === 'schemas/orders/line-item.json', 'schema path', plan);
plan = C.planNew({ kind: 'dataset', name: 'World cities', columns: 'name, lat' }, c0.state.files, ctx); ok(plan.ok && plan.path === 'datasets/world-cities.csv' && plan.id === 'worldCities', 'dataset path and id', plan);
plan = C.planNew({ kind: 'hook', name: 'Totals add up', hookFile: 'hooks/checks.ts' }, c0.state.files, ctx); ok(plan.ok && plan.id === 'totalsAddUp', 'hook export id', plan);
const hp = C.happyPathTest('getsOrder', 'Returns a placed order', 'orders.get', ctx);
ok(hp.before && hp.before[0].call === 'orders.place' && hp.call === 'orders.get' && hp.expect.status === 200, 'happy path adds the saving api in before', hp);

console.log('core: pass', pass, 'fail', fail);
process.exitCode = fail ? 1 : 0;
