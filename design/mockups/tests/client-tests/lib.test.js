// Unit tests for the pure logic shipped inside WebClient.dc.html (extracted, not copied).
const assert = require('assert');
const { loadComponent } = require('./load');
const jsyaml = require('../npmtmp/package');
const { Component } = loadComponent();
const L = Component.lib();

let n = 0;
function t(name, fn) { fn(); n++; console.log('ok  ' + name); }

const cat = L.initialCatalog(), cfg = L.initialConfig();
const env = (profile) => ({ catalog: cat, config: cfg, profile: profile });
const saved = { userId: 'u_7f3k29qp', cartId: 'c_51Hx9w', orderId: 'ord_8Kq2Tz' };

t('url resolves per profile', () => {
  const r = L.reqFromApi(L.findApi(cat, 'users.get'), 'x');
  assert.strictEqual(r.urlTpl, '${var.usersUrl}/api/v1/users/${var.userId}');
  assert.strictEqual(L.buildRequest(r, env('staging'), saved).url, 'https://users.staging.shop.example/api/v1/users/u_7f3k29qp');
  assert.strictEqual(L.buildRequest(r, env('dev'), saved).url, 'http://localhost:8081/api/v1/users/u_7f3k29qp');
  assert.strictEqual(L.buildRequest(r, env('prod'), saved).url, 'https://users.shop.example/api/v1/users/u_7f3k29qp');
});
t('missing vars are reported, not sent', () => {
  const r = L.reqFromApi(L.findApi(cat, 'users.get'), 'x');
  const b = L.buildRequest(r, env('staging'), {});
  assert.deepStrictEqual(b.missing, ['var.userId']);
  const resp = L.execRequest(r, env('staging'), {});
  assert.strictEqual(resp.error, 'missing');
});
t('var precedence call > saved > api > file > profile > config', () => {
  const r = L.reqFromApi(L.findApi(cat, 'users.create'), 'x');
  let rows = L.varRows(r, L.levelsFor(r, env('staging'), {}), { vars: L.mergeLevels(L.levelsFor(r, env('staging'), {})) });
  const role = rows.find((x) => x.name === 'role');
  assert.strictEqual(role.win.level, 'api');
  r.callVars = { role: 'admin' };
  const lv = L.levelsFor(r, env('staging'), {});
  rows = L.varRows(r, lv, { vars: L.mergeLevels(lv) });
  assert.strictEqual(rows.find((x) => x.name === 'role').win.level, 'call');
  assert.strictEqual(rows.find((x) => x.name === 'role').resolved, 'admin');
  assert.deepStrictEqual(rows.map((x) => x.name), ['usersUrl', 'name', 'email', 'role']);
  const lv2 = L.levelsFor(L.reqFromApi(L.findApi(cat, 'orders.place'), 'y'), env('staging'), { orderTotal: 1 });
  assert.strictEqual(L.mergeLevels(lv2).orderTotal, 1, 'saved beats file');
  assert.strictEqual(L.mergeLevels(L.levelsFor(L.reqFromApi(L.findApi(cat, 'orders.place'), 'y'), env('staging'), {})).orderTotal, 59.98, 'file var');
});
t('body templates keep types and fakes resolve', () => {
  const r = L.reqFromApi(L.findApi(cat, 'carts.addItem'), 'x');
  const b = L.buildRequest(r, env('staging'), saved);
  assert.deepStrictEqual(b.body, { sku: 'SKU-1', quantity: 1 });
  const c = L.buildRequest(L.reqFromApi(L.findApi(cat, 'users.create'), 'x'), env('staging'), {});
  assert.strictEqual(c.body.name, 'Amara Okafor');
});
t('pipes', () => {
  const ctx = { vars: { price: '24.99', name: '  Ana ' } };
  assert.strictEqual(L.resolveValue('${var.price | number}', ctx), 24.99);
  assert.strictEqual(L.resolveValue('${var.nope | default(\'x\')}', ctx), 'x');
  assert.strictEqual(L.resolveValue('${var.name | trim | lower}', ctx), 'ana');
});
t('headers inherit config, file, auth, body and can be overridden', () => {
  const r = L.reqFromApi(L.findApi(cat, 'orders.place'), 'x');
  let h = L.inheritedHeaders(r, env('staging'));
  assert.deepStrictEqual(h.map((x) => x.k), ['accept', 'user-agent', 'x-shop-region', 'authorization', 'content-type']);
  r.headers = [{ k: 'Accept', v: 'application/xml', on: true }];
  h = L.inheritedHeaders(r, env('staging'));
  assert.strictEqual(h[0].overridden, true);
  r.authMode = 'none';
  assert.ok(!L.inheritedHeaders(r, env('staging')).some((x) => x.k === 'authorization'));
});
t('orders.place fails the total check deterministically', () => {
  const resp = L.execRequest(L.reqFromApi(L.findApi(cat, 'orders.place'), 'x'), env('staging'), saved);
  assert.strictEqual(resp.status, 201);
  const c = resp.checks.find((x) => x.label === 'body.match.total');
  assert.strictEqual(c.pass, false);
  assert.strictEqual(c.expected, '59.98');
  assert.strictEqual(c.actual, '64.78');
  assert.strictEqual(resp.saved[0].value, 'ord_8Kq2Tz');
});
t('extra checks: status, body, timing', () => {
  const r = L.reqFromApi(L.findApi(cat, 'users.get'), 'x');
  r.extraChecks = [{ kind: 'status', value: '200' }, { kind: 'body', path: 'role', op: 'eq', value: 'member' }, { kind: 'body', path: 'status', op: 'eq', value: 'deactivated' }, { kind: 'timing', path: 'total', op: 'lt', value: '5' }];
  const resp = L.execRequest(r, env('staging'), saved);
  assert.deepStrictEqual(resp.checks.map((c) => c.pass), [true, true, true, false, false]);
});
t('blank request matched by URL shape', () => {
  const r = L.blankReq('x');
  r.urlTpl = 'https://users.staging.shop.example/api/v1/users/u_1';
  const resp = L.execRequest(r, env('staging'), {});
  assert.strictEqual(resp.apiId, 'users.get');
  r.urlTpl = 'https://users.staging.shop.example/api/v1/nothing';
  assert.strictEqual(L.execRequest(r, env('staging'), {}).status, 404);
});
t('invalid JSON body is caught with a line number', () => {
  const e = L.jsonError('{\n  "a": 1,\n  "b": }');
  assert.ok(/line 3/.test(e), e);
});
t('flow chains saved values between stages', () => {
  const flow = L.initialFlows()[0];
  const run = L.runFlow(flow, env('staging'));
  const r = (i) => run.results[flow.stages[i].uid];
  assert.deepStrictEqual(flow.stages.map((s, i) => r(i).status), ['passed', 'passed', 'passed', 'failed', 'skipped']);
  assert.deepStrictEqual(run.snapshots.f1b, { userId: 'u_7f3k29qp', cartId: 'c_51Hx9w' });
  assert.strictEqual(r(1).response.body.userId, 'u_7f3k29qp', 'stage 2 used userId from stage 1');
  assert.strictEqual(r(2).note, '×2 · SKU-1, SKU-2');
  assert.strictEqual(r(3).response.body.cartId, 'c_51Hx9w');
  flow.stopOnFail = false;
  const run2 = L.runFlow(flow, env('staging'));
  assert.strictEqual(run2.results.f1e.status, 'passed');
  assert.strictEqual(run2.results.f1e.note, '3 polls · status paid');
  const info = L.stageInfo(flow, cat);
  assert.deepStrictEqual(info[1].consumes.map((c) => c.name + ':' + c.from), ['userId:stage 1']);
  assert.deepStrictEqual(info[4].consumes.map((c) => c.name + ':' + c.from), ['orderId:stage 4']);
});
t('flow stage without its producer fails with a missing var', () => {
  const flow = { stages: [{ uid: 'a', api: 'carts.create', vars: {} }], stopOnFail: true };
  const run = L.runFlow(flow, env('staging'));
  assert.strictEqual(run.results.a.status, 'failed');
  assert.ok(/userId/.test(run.results.a.note));
});
t('single stage run uses the previous snapshot', () => {
  const flow = L.initialFlows()[0];
  const full = L.runFlow(flow, env('staging'));
  const one = L.runFlow(flow, env('staging'), 'f1e', { results: full.results, snapshots: full.snapshots });
  assert.strictEqual(one.results.f1e.status, 'passed');
});
t('flow var table', () => {
  const flow = L.initialFlows()[0];
  const run = L.runFlow(flow, env('staging'));
  run.shown = { f1a: true, f1b: true };
  const tbl = L.flowVarTable(flow, env('staging'), run);
  const row = (n) => tbl.find((x) => x.name === n);
  assert.deepStrictEqual(row('userId').cells.map((c) => c.val), ['u_7f3k29qp', 'u_7f3k29qp', '', '', '']);
  assert.ok(row('cartId').cells[1].sets);
  assert.strictEqual(row('sku').cells[2].val, 'SKU-1, SKU-2');
  assert.strictEqual(row('currency').cells[3].val, 'EUR');
});
t('HAR filter counts', () => {
  const E = L.harEntries();
  assert.deepStrictEqual(L.harCounts(E), { total: 46, static: 21, thirdParty: 6, preflight: 5, api: 14 });
  const f = { hideStatic: true, hideThird: true, hidePreflight: true, included: {} };
  assert.strictEqual(L.harVisible(E, f).length, 14);
  assert.strictEqual(L.harSelected(E, f, {}).length, 14);
  assert.strictEqual(L.harVisible(E, Object.assign({}, f, { hideStatic: false })).length, 35);
  assert.strictEqual(L.harSelected(E, Object.assign({}, f, { hideStatic: false }), {}).length, 14, 'unhidden statics are not selected');
  const ex = {}; ex[L.harVisible(E, f)[0].idx] = true;
  assert.strictEqual(L.harSelected(E, f, ex).length, 13);
});
t('api id derivation', () => {
  const d = L.deriveApiId;
  assert.strictEqual(d('POST', '/carts', 'carts'), 'create');
  assert.strictEqual(d('POST', '/carts/${var.cartId}/items', 'carts'), 'addItem');
  assert.strictEqual(d('GET', '/orders/${var.orderId}', 'orders'), 'get');
  assert.strictEqual(d('GET', '/products/${var.productId}', 'catalog'), 'getProduct');
  assert.strictEqual(d('GET', '/payment-methods', 'payments'), 'listPaymentMethods');
  assert.strictEqual(d('POST', '/coupons/validate', 'coupons'), 'validate');
  assert.strictEqual(d('POST', '/users/${var.userId}/deactivate', 'users'), 'deactivate');
  assert.strictEqual(d('DELETE', '/users/${var.userId}', 'users'), 'delete');
  assert.strictEqual(d('GET', '/users', 'users'), 'list');
  assert.strictEqual(d('GET', '/exports/${var.exportId}/status', 'users'), 'status');
  assert.strictEqual(d('POST', '/orders/${var.orderId}/notes', 'orders'), 'addNote');
  assert.strictEqual(d('GET', '/products/${var.sku}/stock-levels', 'catalog'), 'listStockLevels');
});
t('path param detection', () => {
  assert.deepStrictEqual(L.templatePath('/orders/ord_8Kq2Tz', {}), { path: '/orders/${var.orderId}', params: [{ name: 'orderId', recorded: 'ord_8Kq2Tz' }] });
  assert.strictEqual(L.templatePath('/carts/c_51Hx9w/items', {}).path, '/carts/${var.cartId}/items');
  assert.strictEqual(L.templatePath('/products/SKU-1', {}).path, '/products/${var.productId}');
  assert.strictEqual(L.templatePath('/v1/users/42', {}).path, '/v1/users/${var.userId}');
  assert.strictEqual(L.templatePath('/users/550e8400-e29b-41d4-a716-446655440000', {}).path, '/users/${var.userId}');
  assert.strictEqual(L.templatePath('/payment-methods', {}).path, '/payment-methods');
  assert.ok(!L.isIdLike('v1') && !L.isIdLike('items') && !L.isIdLike('payment-methods'));
});
t('HAR mapping: matches, merges, new files, chaining', () => {
  const E = L.harEntries();
  const sel = L.harSelected(E, { hideStatic: true, hideThird: true, hidePreflight: true, included: {} }, {});
  const g = L.harMap(sel, cat, {});
  const by = {};
  g.forEach((x) => x.apis.forEach((a) => { by[x.fileId + ':' + a.method + ' ' + a.recordedPath] = a; }));
  assert.strictEqual(g.find((x) => x.fileId === 'coupons').isNew, true);
  assert.strictEqual(g.find((x) => x.fileId === 'users').isNew, false);
  assert.strictEqual(by['users:POST /api/v1/users'].match, 'users.create');
  assert.strictEqual(by['users:POST /api/v1/users'].action, 'skip');
  assert.strictEqual(by['orders:GET /api/v1/orders/ord_8Kq2Tz'].count, 3);
  assert.strictEqual(by['orders:GET /api/v1/orders/ord_8Kq2Tz'].match, 'orders.get');
  assert.strictEqual(by['orders:POST /api/v1/orders'].action, 'update');
  assert.deepStrictEqual(by['orders:POST /api/v1/orders'].extraFields, ['couponCode']);
  assert.strictEqual(by['carts:POST /api/v1/carts/c_51Hx9w/items'].match, 'carts.addItem');
  assert.strictEqual(by['carts:POST /api/v1/carts/c_51Hx9w/items'].count, 2);
  const cv = by['coupons:POST /api/v1/coupons/validate'];
  assert.strictEqual(cv.id, 'validate');
  assert.deepStrictEqual(cv.vars, { code: 'WELCOME10' });
  assert.deepStrictEqual(cv.body, { code: '${var.code}', cartId: '${var.cartId}' });
  assert.strictEqual(by['payments:GET /api/v1/payment-methods'].id, 'listPaymentMethods');
  const total = g.reduce((s, x) => s + x.apis.length, 0);
  assert.strictEqual(total, 11);
  // editable ids with validation
  const g2 = L.harMap(sel, cat, { ids: { 'coupons:POST /coupons/validate': 'Validate-Coupon' } });
  const a2 = g2.find((x) => x.fileId === 'coupons').apis[0];
  assert.ok(a2.idError && /validateCoupon/.test(a2.idError), a2.idError);
  const g3 = L.harMap(sel, cat, { ids: { 'payments:GET /payment-methods': 'authorize' } });
  assert.ok(/already exists/.test(g3.find((x) => x.fileId === 'payments').apis[0].idError));
  const ch = L.chainSummary(g);
  assert.deepStrictEqual(ch.find((c) => c.name === 'orderId').to, ['payments.authorize', 'orders.get']);
});
t('HAR secrets', () => {
  const E = L.harEntries();
  const sel = L.harSelected(E, { hideStatic: true, hideThird: true, hidePreflight: true, included: {} }, {});
  const s = L.harSecrets(sel, 'PAYMENTS_KEY');
  assert.deepStrictEqual(s.map((x) => x.kind), ['header', 'cookie', 'query']);
  assert.ok(/\$\{secret\.PAYMENTS_KEY\}/.test(s[2].outcome));
});
t('HAR flow steps and files', () => {
  const E = L.harEntries();
  const sel = L.harSelected(E, { hideStatic: true, hideThird: true, hidePreflight: true, included: {} }, {});
  const g = L.harMap(sel, cat, {});
  const steps = L.harFlowSteps(g, sel, false);
  assert.deepStrictEqual(steps.map((s) => s.call), ['users.create', 'carts.create', 'carts.addItem', 'coupons.validate', 'orders.place', 'payments.authorize', 'orders.get']);
  assert.deepStrictEqual(steps[2].forEach, ['SKU-1', 'SKU-2']);
  assert.deepStrictEqual(steps[6].poll.until.body.match, { status: 'paid' });
  assert.strictEqual(L.harFlowSteps(g, sel, true).length, 11);
  const test = { id: 'recordedCheckout', name: 'Recorded checkout', tags: ['recorded'], steps: steps };
  const res = L.harFiles(g, cat, { config: cfg, secretName: 'PAYMENTS_KEY', hasKeySecret: true, flowTest: test });
  const paths = res.files.map((f) => f.path + ':' + f.status);
  assert.deepStrictEqual(paths.sort(), ['.env.schema:changed', 'services/coupons/coupons.api.yaml:new', 'services/orders/checkout.test.yaml:changed', 'services/orders/orders.api.yaml:changed', 'services/payments/payments.api.yaml:changed', 'vibranium.config.yaml:changed'].sort());
  for (const f of res.files) if (f.path.endsWith('.yaml')) jsyaml.load(f.after);
  const coupons = jsyaml.load(res.files.find((f) => f.path.includes('coupons')).after);
  assert.strictEqual(coupons.id, 'coupons');
  assert.strictEqual(coupons.baseUrl, '${var.couponsUrl}/api/v1');
  assert.strictEqual(coupons.apis[0].id, 'validate');
  const pay = jsyaml.load(res.files.find((f) => f.path.includes('payments')).after);
  assert.strictEqual(pay.apis[1].query.api_key, '${secret.PAYMENTS_KEY}');
  const checkout = jsyaml.load(res.files.find((f) => f.path.includes('checkout')).after);
  assert.strictEqual(checkout.tests[2].id, 'recordedCheckout');
  assert.strictEqual(checkout.tests[2].steps[6].call, 'orders.get');
  const orders = jsyaml.load(res.files.find((f) => f.path.includes('orders.api')).after);
  assert.strictEqual(orders.apis[0].body.couponCode, '${var.couponCode}');
  assert.ok(res.catalog.find((f) => f.id === 'coupons'));
  const stages = L.stepsToStages(steps, 'r');
  assert.strictEqual(stages[2].loop.items, 'SKU-1, SKU-2');
  assert.strictEqual(stages[6].poll.equals, 'paid');
  const fl = L.runFlow({ stages: stages, stopOnFail: false }, { catalog: res.catalog, config: Object.assign({}, cfg, { profiles: Object.assign({}, cfg.profiles, { staging: Object.assign({ couponsUrl: 'https://coupons.staging.shop.example' }, cfg.profiles.staging) }) }), profile: 'staging' });
  assert.strictEqual(fl.results.r3.status, 'passed', 'imported coupons.validate runs: ' + fl.results.r3.note);
});
t('YAML emitter round-trips through js-yaml', () => {
  const samples = [
    { id: 'x', s: '${var.a}', n: 1.5, b: true, nul: null, q: 'a: b', h: 'x #y', yes: 'yes', num: '42', date: '2026-10-23', empty: '', list: [200, 204, 404], strs: ['SKU-1', 'a, b'], nested: { deep: { more: [{ a: 1, b: { c: 'd' } }, 'z'] } }, emptyObj: {}, emptyArr: [], star: '*x', apos: "it's", multi: 'a\nb', dollar: '$file' },
    L.flowToTest(L.initialFlows()[0], 'checksOutTwoItemCart', 'Checks out a two-item cart'),
    L.flowToLifecycle(L.initialFlows()[0], 3, 'placesOrder', 'Places an order'),
    { apis: cat.map((f) => f.apis.map(L.apiToObj)) }
  ];
  for (const s of samples) assert.deepStrictEqual(jsyaml.load(L.toYaml(s)), JSON.parse(JSON.stringify(s)));
  for (const f of cat) assert.strictEqual(jsyaml.load(L.apiFileYaml(f)).id, f.id);
});
t('save as test YAML', () => {
  const r = L.reqFromApi(L.findApi(cat, 'users.create'), 'x');
  const resp = L.execRequest(r, env('staging'), {});
  const exp = L.suggestExpectations(resp, 'users.create');
  assert.deepStrictEqual(exp.map((e) => e.key), ['status', 'schema', 'match.role', 'match.status', 'match.verified', 'timing']);
  assert.strictEqual(L.deriveTestId('Rejects a declined card'), 'rejectsDeclinedCard');
  assert.strictEqual(L.deriveTestId('Creates a member'), 'createsMember');
  assert.strictEqual(L.thirdPerson('Create a user'), 'Creates a user');
  assert.strictEqual(L.thirdPerson('Search products'), 'Searches products');
  assert.strictEqual(L.thirdPerson('Deactivate a user'), 'Deactivates a user');
  assert.ok(L.testIdWarning('checkoutHappyPath'));
  assert.ok(!L.testIdWarning('createsMember'));
  const test = L.testFromRequest({ id: 'createsMember', name: 'Creates a member', apiId: 'users.create', vars: { role: 'member' }, expectations: exp, status: 201 });
  const y = L.testBlockYaml(test);
  const back = jsyaml.load('tests:\n' + y);
  assert.deepStrictEqual(back.tests[0], { id: 'createsMember', name: 'Creates a member', call: 'users.create', vars: { role: 'member' },
    expect: { status: 201, body: { schema: { $file: '../../schemas/users/user.json' }, match: { role: 'member', status: 'active' } }, timing: { total: { lt: resp.timing.total * 3 > 100 ? Math.ceil(resp.timing.total * 3 / 100) * 100 + 'ms' : '100ms' } } } });
  assert.strictEqual(L.testFilePath('users.crud'), 'services/users/crud.test.yaml');
  assert.strictEqual(L.testFilePath('payments.smoke'), 'services/payments/smoke.test.yaml');
});
t('save changes to api: reqToApi and diff', () => {
  const found = L.findApi(cat, 'users.create');
  const r = L.reqFromApi(found, 'x');
  assert.strictEqual(L.reqDirty(r, cat), false);
  r.method = 'PUT';
  r.headers = [{ k: 'x-trace', v: '${run.id}', on: true }];
  assert.strictEqual(L.reqDirty(r, cat), true);
  const res = L.reqToApi(r, found);
  assert.strictEqual(res.api.method, 'PUT');
  const f2 = L.clone(found.file); f2.apis[0] = res.api;
  const dv = L.diffView(L.apiFileYaml(found.file), L.apiFileYaml(f2));
  assert.strictEqual(dv.add, 3); assert.strictEqual(dv.del, 1);
  r.urlTpl = 'https://elsewhere.example/x';
  assert.ok(L.reqToApi(r, found).error);
});
t('cURL parse and round trip', () => {
  const p = L.parseCurl("curl -X POST 'https://orders.staging.shop.example/api/v1/orders/ord_8Kq2Tz/notes' \\\n  -H 'authorization: Bearer abc' \\\n  -H 'content-type: application/json' \\\n  --data-raw '{\"text\":\"Leave at the side door\"}'");
  assert.strictEqual(p.method, 'POST');
  assert.strictEqual(p.url, 'https://orders.staging.shop.example/api/v1/orders/ord_8Kq2Tz/notes');
  assert.deepStrictEqual(p.headers[0], ['authorization', 'Bearer abc']);
  assert.strictEqual(JSON.parse(p.body).text, 'Leave at the side door');
  assert.strictEqual(L.parseCurl('curl https://x.example/a').method, 'GET');
  assert.strictEqual(L.parseCurl('curl -d a=1 https://x.example/a').method, 'POST');
  assert.ok(L.parseCurl('wget x').error);
  const r = L.reqFromApi(L.findApi(cat, 'users.create'), 'x');
  const b = L.buildRequest(r, env('staging'), {});
  const c = L.curlFor(b);
  const back = L.parseCurl(c);
  assert.strictEqual(back.url, b.url);
  assert.deepStrictEqual(JSON.parse(back.body), b.body);
  assert.ok(back.headers.some((h) => h[0] === 'authorization' && h[1] === 'Bearer $TOKEN'));
  const q = L.parseCurl("curl 'https://x.example/a' --data-raw '{\"n\":\"it'\\''s\"}'");
  assert.strictEqual(JSON.parse(q.body).n, "it's");
});
t('OpenAPI and Postman mapping', () => {
  const rows = L.openapiMap(L.openapiOps(), cat, 'payments');
  assert.deepStrictEqual(rows.map((r) => r.id), ['listPaymentMethods', 'authorize', 'get', 'capture', 'addRefund']);
  assert.strictEqual(rows[1].match, 'payments.authorize');
  const pm = L.postmanMap(L.postmanItems(), cat);
  assert.deepStrictEqual(pm.map((r) => r.service + '.' + r.id + (r.match ? '=' : '+')), ['users.create=', 'users.get=', 'orders.list+', 'orders.cancel=', 'catalog.getProduct=', 'catalog.listStockLevels+']);
  assert.ok(!pm.some((r) => r.tpl.indexOf('{' + '{') >= 0));
});
t('JSON and YAML tokenizers', () => {
  const segs = L.tokenizeJson('  "id": "u_7f3k29qp",');
  assert.deepStrictEqual(segs.map((s) => s.cls), ['ws', 'y-key', 'y-punc', 'ws', 'y-str', 'y-punc']);
  const y = L.tokenizeYaml('    forEach: [SKU-1, SKU-2]');
  assert.ok(y.some((s) => s.cls === 'y-key' && s.t === 'forEach'));
  const z = L.tokenizeYaml('  - call: ${var.x}');
  assert.ok(z.some((s) => s.cls === 'y-tpl'));
  assert.strictEqual(L.tokenizeYaml('# c')[0].cls, 'y-com');
  const all = L.tokenizeYaml('      sku: ${item}').map((s) => s.t).join('');
  assert.strictEqual(all, '      sku: ${item}');
});
t('line diff', () => {
  const d = L.lineDiff('a\nb\nc', 'a\nx\nc\nd');
  assert.deepStrictEqual(d.map((l) => l.op + l.text), [' a', '-b', '+x', ' c', '+d']);
  const h = L.diffHunks(L.lineDiff(Array.from({ length: 20 }, (_, i) => 'l' + i).join('\n'), Array.from({ length: 20 }, (_, i) => (i === 10 ? 'X' : 'l' + i)).join('\n')), 2);
  assert.strictEqual(h[0].gap, true);
});
console.log('\n' + n + ' lib tests passed');
