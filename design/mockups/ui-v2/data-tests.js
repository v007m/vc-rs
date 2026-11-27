/* Tests mode fake data (pass 2): the tests tree, per-test history, a run plan with
   its events and drill-down, the plan preview, and a load file with its series.
   Exposed as VC.testsData (TD). Deterministic: a seeded PRNG, so shots are stable. */
(function () {
  'use strict';
  const VC = window.VC;
  const TD = (VC.testsData = {});

  function prng(seed) {
    let a = seed >>> 0;
    return () => { a = (a + 0x6d2b79f5) >>> 0; let t = a; t = Math.imul(t ^ (t >>> 15), t | 1); t ^= t + Math.imul(t ^ (t >>> 7), t | 61); return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
  }
  TD.prng = prng;

  /* ---- files -------------------------------------------------------------- */
  TD.files = {
    'users.crud': { path: 'services/users/crud.test.yaml', name: 'User accounts', tags: ['users', 'smoke'], changed: true,
      lifecycle: { beforeAll: [], beforeEach: [], afterAll: [], cleanups: ['users#delete (from users#create)'] } },
    'users.auth': { path: 'services/users/auth.test.yaml', name: 'Tokens', tags: ['users', 'auth'], changed: false,
      lifecycle: { beforeAll: ['call: users#create · id admin'], beforeEach: [], afterAll: [], cleanups: ['users#delete (admin)'] } },
    'orders.checkout': { path: 'services/orders/checkout.test.yaml', name: 'Checkout', tags: ['orders', 'checkout'], changed: true,
      lifecycle: { beforeAll: ['call: catalog#seedProducts · id catalog · cached 3h'], beforeEach: ['call: users#create', 'call: carts#create', 'call: carts#addItem · forEach ${var.catalog.productIds}'],
        afterAll: ['command: ./scripts/print-order-stats.sh'], cleanups: ['carts#delete', 'users#delete'] } },
    'payments.refunds': { path: 'services/payments/refunds.test.yaml', name: 'Refunds', tags: ['payments'], changed: false,
      allowFailure: { reason: 'payments sandbox is down, see #340', until: '2026-12-01' },
      lifecycle: { beforeAll: [], beforeEach: ['call: orders#create'], afterAll: [], cleanups: ['orders#cancel'] } },
    'catalog.read': { path: 'services/catalog/read.test.yaml', name: 'Catalog reads', tags: ['catalog', 'smoke'], changed: false,
      lifecycle: { beforeAll: [], beforeEach: [], afterAll: [], cleanups: [] } },
    'rooms.lobby': { path: 'services/rooms/lobby.test.yaml', name: 'Lobby chat', tags: ['streams'], changed: false,
      lifecycle: { beforeAll: [], beforeEach: ['call: users#create'], afterAll: [], cleanups: ['users#delete'] } }
  };
  TD.beforeRun = ['auth: shopUser token (cached, 41 min left)'];

  /* ---- tests: [id, status, line, ms, apis, own tags, extra] ---------------- */
  const T = [
    ['users.crud', 'createsUser', 'pass', 6, 118, ['users#create'], []],
    ['users.crud', 'getsCreatedUser', 'pass', 14, 96, ['users#create', 'users#get'], [], { repeat: 5 }],
    ['users.crud', 'rejectsBadEmail', 'flaky', 34, 142, ['users#create'], ['negative'], { examples: 2 }],
    ['users.crud', 'renamesUser', 'pass', 48, 233, ['users#create', 'users#update', 'users#get'], []],
    ['users.crud', 'deletesUser', 'pass', 66, 88, ['users#create', 'users#delete'], []],
    ['users.auth', 'rejectsExpiredToken', 'pass', 9, 61, ['users#get'], ['negative']],
    ['users.auth', 'refreshesToken', 'skipped', 21, 0, ['users#get'], [], { skip: { reason: 'token service rollout, see #288', until: '2026-12-01' } }],
    ['orders.checkout', 'placesOrder', 'pass', 19, 412, ['orders#create', 'payments#charge'], ['smoke']],
    ['orders.checkout', 'rejectsDeclinedCard', 'fail', 34, 388, ['orders#create', 'payments#charge'], ['negative']],
    ['orders.checkout', 'refundsPartially', 'blocked', 47, 0, ['orders#create', 'payments#refund'], []],
    ['orders.checkout', 'appliesCoupon', 'allow', 64, 301, ['orders#create'], [], { examples: 3, allow: { reason: 'coupon service flaky on staging, see #212', until: '2026-12-07' } }],
    ['payments.refunds', 'refundsOrder', 'pass', 8, 274, ['payments#refund'], []],
    ['payments.refunds', 'listsRefunds', 'flaky', 15, 190, ['payments#list'], ['slow']],
    ['catalog.read', 'readsProduct', 'pass', 7, 54, ['catalog#product'], []],
    ['catalog.read', 'hidesCostPrice', 'pass', 19, 49, ['catalog#product'], ['security']],
    ['rooms.lobby', 'echoesChatMessage', 'skipped', 11, 0, ['rooms#join'], [], { skip: { reason: 'websocket mock moves to vc demo 2.1', until: '2026-12-15' } }]
  ];
  const NAMES = {
    createsUser: 'Creates a member account', getsCreatedUser: 'Returns the user that was just created', rejectsBadEmail: 'Rejects malformed email addresses',
    renamesUser: 'A renamed user reads back with the new name', deletesUser: 'Deletes a user and returns 204', rejectsExpiredToken: 'Rejects an expired bearer token',
    refreshesToken: 'Refreshes a token before it expires', placesOrder: 'Places an order with a saved card', rejectsDeclinedCard: 'Rejects a declined card with 402',
    refundsPartially: 'Refunds part of an order', appliesCoupon: 'Applies a percentage coupon', refundsOrder: 'Refunds a whole order', listsRefunds: 'Lists refunds newest first',
    readsProduct: 'Reads a product by id', hidesCostPrice: 'Never returns the cost price', echoesChatMessage: 'Echoes a chat message to the lobby'
  };
  function history(st, ms, rnd) {
    const out = [], durs = [];
    for (let i = 0; i < 30; i++) {
      let s = 'pass';
      const r = rnd();
      if (st === 'flaky') s = r < 0.17 ? 'flaky' : r < 0.24 ? 'fail' : 'pass';
      else if (st === 'fail') s = i > 25 ? 'fail' : r < 0.06 ? 'fail' : 'pass';
      else if (st === 'blocked') s = i > 27 ? 'blocked' : r < 0.05 ? 'fail' : 'pass';
      else if (st === 'allow') s = r < 0.35 ? 'allow' : 'pass';
      else if (st === 'skipped') s = i > 17 ? 'skipped' : r < 0.04 ? 'fail' : 'pass';
      else s = r < 0.03 ? 'fail' : 'pass';
      out.push(s);
      const base = ms || 120;
      durs.push(s === 'skipped' || s === 'blocked' ? null : Math.round(base * (0.75 + rnd() * 0.5 + (i > 22 && st === 'fail' ? 0.3 : 0))));
    }
    return { runs: out, durs };
  }
  TD.tests = {};
  TD.order = [];
  T.forEach(([file, id, st, line, ms, apis, tags, extra], i) => {
    const full = `${file}#${id}`;
    const h = history(st, ms, prng(i * 7919 + 17));
    const flakes = h.runs.filter((s) => s === 'flaky' || s === 'fail' || s === 'allow').length;
    TD.tests[full] = Object.assign({ full, file, id, name: NAMES[id], st, line, ms, apis, tags, history: h.runs, durs: h.durs, flake: Math.round((flakes / 30) * 100), lastRun: st === 'skipped' ? '—' : 'today 14:02' }, extra || {});
    TD.order.push(full);
  });
  TD.allTags = (t) => TD.files[t.file].tags.concat(t.tags);

  /* the sidebar tree (overrides pass 1's): services → files → tests, plus a load file */
  VC.data.testsTree = ['users', 'orders', 'payments', 'catalog', 'rooms'].map((svc) => ({
    service: svc,
    files: Object.keys(TD.files).filter((f) => f.startsWith(svc + '.')).map((f) => ({ id: f, tests: TD.order.filter((k) => TD.tests[k].file === f).map((k) => [TD.tests[k].id, TD.tests[k].st]) }))
  })).concat([{ service: 'load', files: [{ id: 'checkout.peak', load: true, tests: [['p99 < 800ms', 'fail'], ['errorRate < 1%', 'pass'], ['placesOrder p95', 'pass']] }] }]);

  TD.savedViews = [
    { name: 'Failing smoke', q: 'tag:smoke status:failed' },
    { name: 'Flaky this week', q: 'status:flaky' },
    { name: 'Checkout, not slow', q: 'api:orders -tag:slow' },
    { name: 'Changed on branch', q: 'changed:yes' }
  ];

  /* ---- editor: the open file as YAML -------------------------------------- */
  TD.yaml = `id: orders.checkout
name: Checkout
tags: [orders, checkout]

beforeAll:
  - id: catalog
    call: catalog#seedProducts
    cache: 3h

beforeEach:
  - call: users#create
  - call: carts#create
  - call: carts#addItem
    forEach: \${var.catalog.productIds}
    vars:
      sku: \${item}

tests:
  - id: placesOrder
    name: Places an order with a saved card
    tags: [smoke]
    call: orders#create
    vars:
      cardToken: \${var.savedCard}
    expect:
      status: 201
      body:
        match:
          state: { in: [placed, paid] }
          total: { gt: 0 }
    save:
      orderId: \${response.body.id}

  - id: rejectsDeclinedCard
    name: Rejects a declined card with 402
    call: payments#charge
    vars:
      cardToken: \${var.declinedCard}
    expect:
      status: 402
      body:
        match:
          error.code: card_declined
      timing:
        total: { lt: 800ms }

  - id: refundsPartially
    name: Refunds part of an order
    steps:
      - call: orders#create
        save:
          orderId: \${response.body.id}
      - call: payments#refund
        vars:
          amount: 1250
        expect:
          status: 201
      - call: orders#get
        expect:
          body:
            match:
              refundedTotal: 1250

  - id: appliesCoupon
    name: Applies a percentage coupon
    call: orders#create
    allowFailure:
      reason: coupon service flaky on staging, see #212
      until: 2026-12-07
    examples:
      - { coupon: TEN, percent: 10 }
      - { coupon: HALF, percent: 50 }
      - { coupon: GONE, percent: 0 }
    vars:
      coupon: \${example.coupon}
    expect:
      status: 201
      body:
        match:
          discountPercent: \${example.percent}

afterAll:
  - command: ./scripts/print-order-stats.sh`;
  TD.diag = { line: 24, col: 18, len: 19, code: 'V2101', word: '${var.savedCard}', msg: 'savedCard is never set', help: 'no profile, file, api or earlier step sets savedCard. Did you mean declinedCard or cardToken?', fix: 'set savedCard in vibranium.config.yaml vars, or replace it with ${var.declinedCard}' };
  TD.hover = { word: 'declinedCard', type: 'string', value: 'tok_chargeDeclined', src: 'profile', at: 'profiles.local.vars · vibranium.config.yaml:31', doc: 'A test card token the payments sandbox always declines.' };
  TD.complete = [
    ['var', '${var.declinedCard}', 'profile · string'], ['var', '${var.cardToken}', 'api var · payments#charge'], ['var', '${var.catalog.productIds}', 'beforeAll catalog · list'],
    ['matcher', 'in', 'value is one of a list'], ['matcher', 'between', 'inclusive range'], ['id', 'payments#charge', 'POST /payments/charges'], ['secret', '${secret.STRIPE_KEY}', '•••• · .env']
  ];
  TD.explain = { code: 'V2101', title: 'A var is used but never set', body: ['vc check follows every ${var.name} to the layer that sets it: the profile, the config\'s vars, the api, the file, the test, or an earlier step\'s save.', 'When nothing sets it, the template would render as an error at run time, so the check stops it before the run.', 'A function: or test: action that saves names vc cannot read ahead of time can declare them with provides: [name].'],
    fix: 'Set the var in one of those layers, or fix the name. The quick fix offers the closest names that are set.' };
  TD.templates = [['test', 'Test file', 'vc new test orders.returns#acceptsReturn --from orders#create'], ['api', 'Api file', 'vc new api orders#cancel --method POST --path /orders/{id}/cancel'], ['service', 'Service', 'vc new service shipping --url https://shipping.shop.example'], ['load', 'Load file', 'vc new load checkout.weekend --mix orders.checkout#placesOrder']];

  /* ---- plan preview (vc plan --explain orders.checkout) -------------------- */
  TD.plan = {
    line: '4 tests (4 instances) in 1 file · lifecycle: beforeAll catalog (cached, 3h left) · beforeEach ×3 actions · profile local',
    tree: [['beforeRun', 'auth shopUser · token cached, 41 min left'], ['beforeAll', 'catalog · catalog#seedProducts · cached, 3h left'],
      ['test', 'orders.checkout#placesOrder'], ['test', 'orders.checkout#rejectsDeclinedCard'], ['test', 'orders.checkout#refundsPartially'], ['test', 'orders.checkout#appliesCoupon · allowFailure until 2026-12-07'],
      ['afterAll', 'command ./scripts/print-order-stats.sh'], ['cleanup', 'carts#delete · users#delete, newest first']],
    merged: [['status', '402', 'test', 'checkout.test.yaml:40'], ['headers.content-type', '{ contains: json }', 'api', 'payments.api.yaml:12'], ['body.$.error.code', 'card_declined', 'test', 'checkout.test.yaml:43'], ['timing.total', '{ lt: 800ms }', 'test', 'checkout.test.yaml:45'], ['status 201', 'replaced by the test', 'api', 'payments.api.yaml:10']]
  };

  /* ---- a live run: instances in plan order -------------------------------- */
  const I = (id, st, ms, label) => ({ id, st, ms, label: label || id });
  TD.run = {
    selector: 'tag:smoke OR changed', cmd: 'vc run -t smoke --changed origin/master --profile local --seed 42', seed: 42,
    instances: [
      I('users.crud#createsUser', 'pass', 118), I('users.crud#getsCreatedUser', 'pass', 96, 'users.crud#getsCreatedUser [repeat 1]'), I('users.crud#getsCreatedUser', 'pass', 102, 'users.crud#getsCreatedUser [repeat 2]'),
      I('users.crud#getsCreatedUser', 'pass', 91, 'users.crud#getsCreatedUser [repeat 3]'), I('users.crud#getsCreatedUser', 'pass', 99, 'users.crud#getsCreatedUser [repeat 4]'), I('users.crud#getsCreatedUser', 'pass', 94, 'users.crud#getsCreatedUser [repeat 5]'),
      I('users.crud#rejectsBadEmail', 'pass', 141, 'users.crud#rejectsBadEmail › no-at-sign'), I('users.crud#rejectsBadEmail', 'flaky', 388, 'users.crud#rejectsBadEmail [row 2]'),
      I('users.crud#renamesUser', 'pass', 233), I('users.crud#deletesUser', 'pass', 88), I('orders.checkout#placesOrder', 'pass', 412), I('orders.checkout#rejectsDeclinedCard', 'fail', 388),
      I('orders.checkout#refundsPartially', 'blocked', 0), I('orders.checkout#appliesCoupon', 'allow', 301), I('catalog.read#readsProduct', 'pass', 54), I('catalog.read#hidesCostPrice', 'pass', 49),
      I('users.auth#refreshesToken', 'skipped', 0)
    ],
    prev: { pass: 15, fail: 0, blocked: 0, skipped: 1, flaky: 1, ms: 4210 }
  };
  TD.drill = {
    id: 'orders.checkout#rejectsDeclinedCard', at: 'services/orders/checkout.test.yaml:34',
    exchanges: [
      { phase: 'beforeEach', m: 'POST', path: '/users', s: 201, ms: 41 }, { phase: 'beforeEach', m: 'POST', path: '/carts', s: 201, ms: 22 },
      { phase: 'beforeEach', m: 'POST', path: '/carts/c_91x/items ×3', s: 201, ms: 63 }, { phase: 'call', m: 'POST', path: '/payments/charges', s: 402, ms: 188, main: true },
      { phase: 'cleanup', m: 'DELETE', path: '/carts/c_91x', s: 204, ms: 18 }, { phase: 'cleanup', m: 'DELETE', path: '/users/u_3kd8', s: 204, ms: 21 }
    ],
    checks: [
      { st: 'pass', what: 'status', exp: '402', got: '402', from: 'test', at: 'checkout.test.yaml:40' },
      { st: 'pass', what: 'headers.content-type', exp: '{ contains: json }', got: 'application/json', from: 'api', at: 'payments.api.yaml:12' },
      { st: 'fail', what: 'body.$.error.code', exp: 'card_declined', got: '"insufficient_funds"', code: 'V5120', from: 'test', at: 'checkout.test.yaml:43' },
      { st: 'pass', what: 'timing.total', exp: '{ lt: 800ms }', got: '188 ms', from: 'test', at: 'checkout.test.yaml:45' }
    ],
    span: [[41, '      body:'], [42, '        match:'], [43, '          error.code: card_declined'], [44, '      timing:']],
    diff: [['@', '@@ body.error @@'], [' ', '{'], [' ', '  "error": {'], ['-', '    "code": "card_declined",'], ['+', '    "code": "insufficient_funds",'], [' ', '    "message": "Your card was declined.",'], ['+', '    "decline_code": "insufficient_funds",'], [' ', '    "chargeId": "ch_5Tq2"'], [' ', '  }'], [' ', '}']],
    headers: [['authorization', 'Bearer ••••', 'secret'], ['idempotency-key', 'ik_8f2c-01', 'api'], ['x-request-source', 'vibranium-tests', 'file']]
  };

  /* ---- load: checkout.peak ------------------------------------------------- */
  const stages = [{ duration: 60, users: 500 }, { duration: 300, users: 5000 }, { duration: 60, users: 0 }];
  TD.load = {
    id: 'checkout.peak', path: 'load/checkout/peak.load.yaml', name: 'Black Friday checkout mix', checks: 'api', thinkTime: { min: '500ms', max: '2s' },
    stages, abortOn: ['errorRate'],
    thresholds: [{ metric: 'p99', op: 'lt', v: 800, unit: 'ms' }, { metric: 'errorRate', op: 'lt', v: 1, unit: '%' }, { metric: 'p95', op: 'lt', v: 1200, unit: 'ms', test: 'orders.checkout#placesOrder' }],
    mix: [{ test: 'users.crud#getsCreatedUser', weight: 6 }, { test: 'catalog.read#readsProduct', weight: 3 }, { test: 'orders.checkout#placesOrder', weight: 1, p95: '1200ms' }]
  };
  /* one point per 5 s over 7 min = 84 ticks */
  (function series() {
    const rnd = prng(4242), N = 84, pts = [];
    for (let i = 0; i <= N; i++) {
      const t = i * 5;
      const users = t <= 60 ? (t / 60) * 500 : t <= 360 ? 500 + ((t - 60) / 300) * 4500 : Math.max(0, 5000 * (1 - (t - 360) / 60));
      const load = users / 5000;
      const n = () => 0.92 + rnd() * 0.16;
      const p50 = Math.round((70 + 160 * load * load) * n());
      const p95 = Math.round((150 + 520 * Math.pow(load, 2.2)) * n());
      const spike = t > 300 && t < 345 ? 1.22 : 1;
      const p99 = Math.round((230 + 760 * Math.pow(load, 2.4) * spike) * n());
      const rps = Math.round(users / 1.43 * (1 - 0.06 * load) * n());
      const err = +(Math.max(0, 0.05 + 0.75 * Math.pow(load, 3) * spike * n())).toFixed(2);
      pts.push({ t, users: Math.round(users), rps, p50, p95, p99, err, cpu: Math.round(18 + 58 * load * n()), mem: Math.round(410 + 820 * load * n()), conns: Math.round(users * 0.62), dropped: t > 290 && t < 350 ? Math.round(rnd() * 40 + 10) : 0 });
    }
    TD.load.series = pts;
  })();
  TD.load.histogram = [[25, 1840], [50, 22110], [100, 61200], [150, 48800], [200, 31300], [300, 22800], [400, 12900], [600, 7400], [800, 3550], [1200, 1410], [2000, 380], [5000, 41]];
  TD.load.scenarios = [
    { test: 'users.crud#getsCreatedUser', weight: 6, iters: 128410, p50: 88, p95: 402, p99: 771, err: 0.21, verdict: 'pass' },
    { test: 'catalog.read#readsProduct', weight: 3, iters: 64190, p50: 51, p95: 233, p99: 520, err: 0.04, verdict: 'pass' },
    { test: 'orders.checkout#placesOrder', weight: 1, iters: 21330, p50: 302, p95: 1088, p99: 1904, err: 1.7, verdict: 'fail' }
  ];
  TD.load.breaches = [
    { metric: 'p99', at: '5:10–6:05', value: '1044 ms peak', limit: 'lt 800ms', scope: 'scenario', final: 'breached p99 at 862 ms (lt 800 ms)', st: 'fail' },
    { metric: 'errorRate', at: '5:40 rolling', value: '0.87% peak', limit: 'lt 1%', scope: 'scenario', final: 'held at 0.31% (lt 1%) · abortOn not triggered', st: 'pass' },
    { metric: 'p95', at: '4:50–6:10', value: '1310 ms peak', limit: 'lt 1200ms', scope: 'orders.checkout#placesOrder', final: 'held at 1088 ms (lt 1200 ms)', st: 'pass' }
  ];
  TD.load.previous = [
    { run: 'today 14:20', branch: 's/checkout-v2', p99: 862, rps: 2930, err: 0.31, verdict: 'fail', cur: true },
    { run: 'Fri 11:02', branch: 'master', p99: 714, rps: 2988, err: 0.22, verdict: 'pass' },
    { run: 'Wed 16:40', branch: 'master', p99: 688, rps: 3012, err: 0.19, verdict: 'pass' },
    { run: 'Mon 09:15', branch: 'master', p99: 741, rps: 2901, err: 0.27, verdict: 'pass' }
  ];

  /* ---- load tests listed in the Tests space (the first one has the full design) */
  TD.loadList = [
    { id: 'checkout.peak', name: 'Black Friday checkout mix', path: 'load/checkout/peak.load.yaml', st: 'fail', last: 'p99 862 ms', tags: ['checkout', 'peak'] },
    { id: 'catalog.browse', name: 'Catalog browse soak', path: 'load/catalog/browse.load.yaml', st: 'pass', last: 'p99 412 ms', tags: ['catalog', 'soak'], sample: true }
  ];
  TD.quick = { users: 1, secs: 10, rps: 6.8, p95: 214, p50: 96, err: 0, iters: 68 };

  /* ---- a check run (Run on a test card): verifies one test, not saved to Runs */
  const OK = (s) => [{ st: 'pass', what: 'status', exp: String(s), got: String(s) }, { st: 'pass', what: 'headers.content-type', exp: '{ contains: json }', got: 'application/json' }];
  TD.checkRun = function (full) {
    const t = TD.tests[full];
    if (full === TD.drill.id) {
      const m = TD.drill.exchanges.find((x) => x.main);
      return { st: 'fail', ms: 353, checks: TD.drill.checks, req: `${m.m} ${m.path}`, status: m.s, body: TD.drill.diff.filter(([k]) => k !== '-' && k !== '@').map(([, l]) => l) };
    }
    if (t.st === 'blocked') return { st: 'blocked', ms: 64, checks: [{ st: 'fail', what: 'beforeEach carts#addItem', exp: '201', got: '409' }], req: 'POST /carts/c_91x/items', status: 409, body: ['{', '  "error": "item already in cart"', '}'] };
    const a = (VC.data.apis || {})[t.apis[t.apis.length - 1]] || { m: 'GET', path: '/' };
    const s = a.m === 'POST' ? 201 : a.m === 'DELETE' ? 204 : 200;
    const ms = Math.round((t.ms || 120) * 1.04);
    return { st: 'pass', ms, checks: OK(s).concat({ st: 'pass', what: 'timing.total', exp: '{ lt: 800ms }', got: `${ms} ms` }), req: `${a.m} ${a.path}`, status: s, body: s === 204 ? [] : ['{', `  "id": "${t.file.split('.')[0].slice(0, 1)}_3kd8",`, '  "ok": true', '}'] };
  };

  /* ---- git: the working tree of the test files (the Changes pane) --------- */
  TD.git = {
    branch: 's/checkout-v2', upstream: 'origin/s/checkout-v2', ahead: 0, behind: 1, base: 'origin/master',
    files: [
      { path: 'services/orders/checkout.test.yaml', st: 'M', staged: true,
        lines: [['@', '@@ -34,9 +34,10 @@ tests:'], [' ', '  - id: rejectsDeclinedCard'], [' ', '    name: Rejects a declined card with 402'], [' ', '    call: payments#charge'], ['-', '    tags: [negative]'], ['+', '    tags: [negative, needs-triage]'], [' ', '    expect:'], ['+', '      timing: { total: { lt: 800ms } }']] },
      { path: 'services/users/crud.test.yaml', st: 'M', staged: true,
        lines: [['@', '@@ -14,4 +14,4 @@ tests:'], [' ', '  - id: getsCreatedUser'], ['-', '    repeat: 3'], ['+', '    repeat: 5'], [' ', '    call: users#get']] },
      { path: 'services/orders/coupons.test.yaml', st: 'A', staged: true, isNew: true,
        lines: [['@', '@@ -0,0 +1,9 @@'], ['+', 'name: Coupons'], ['+', 'tests:'], ['+', '  - id: rejectsExpiredCoupon'], ['+', '    name: Rejects a coupon past its end date'], ['+', '    call: orders#create'], ['+', '    with: { body: { coupon: SPRING24 } }'], ['+', '    expect:'], ['+', '      status: 422'], ['+', '      body: { match: { error.code: coupon_expired } }']] },
      { path: 'load/catalog/browse-old.load.yaml', st: 'D', staged: true,
        lines: [['@', '@@ -1,6 +0,0 @@'], ['-', 'name: Catalog browse (old)'], ['-', 'stages:'], ['-', '  - { duration: 5m, users: 200 }'], ['-', 'mix:'], ['-', '  - { test: catalog.read#readsProduct, weight: 1 }'], ['-', 'thresholds: { p99: { lt: 600ms } }']] }
    ]
  };
})();
