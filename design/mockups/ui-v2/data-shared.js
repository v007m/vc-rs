/* Vibranium UI v2 mockups: fake data for the shared screens (Runs, Insights, Agents, Settings).
   Kept under VC.data.shared so it never collides with Tests-mode data. Secrets carry only a
   name and a source, never a value; agent tokens appear only as placeholders. */
(function () {
  'use strict';
  const VC = window.VC;
  const t = (p, f, b, s, fl, al) => ({ pass: p, fail: f, blocked: b, skipped: s, flaky: fl, allow: al });

  /* ---- runs (history.db) ------------------------------------------------------ */
  const runs = [
    { n: 482, kind: 'test', st: 'fail', when: 'Today 14:02', ago: '4 min ago', dur: '41.2s', profile: 'local', layers: ['eu'], branch: 's/checkout-v2', commit: 'a41c9e2', by: 'you', sel: 'orders users', tot: t(38, 2, 1, 3, 1, 1) },
    { n: 481, kind: 'test', st: 'pass', when: 'Today 13:40', ago: '26 min ago', dur: '38.9s', profile: 'local', layers: ['eu'], branch: 's/checkout-v2', commit: '7d02bb1', by: 'you', sel: 'orders users', tot: t(41, 0, 0, 3, 1, 1) },
    { n: 480, kind: 'test', st: 'pass', when: 'Today 13:12', ago: '54 min ago', dur: '12.4s', profile: 'local', layers: [], branch: 's/checkout-v2', commit: '7d02bb1', by: 'claude-code', sel: 'tag:smoke', tot: t(9, 0, 0, 0, 0, 0) },
    { n: 479, kind: 'adhoc', st: 'fail', when: 'Today 13:58', ago: '8 min ago', dur: '311ms', profile: 'local', layers: [], branch: 's/checkout-v2', commit: '7d02bb1', by: 'you', sel: 'payments#charge', code: 402 },
    { n: 478, kind: 'load', st: 'fail', when: 'Today 11:30', ago: '2 h ago', dur: '7m 00s', profile: 'staging', layers: [], branch: 'master', commit: 'e19f04a', by: 'ci', sel: 'checkout.peak', sched: 'weekly-load', load: { iters: 666055, errs: 4120, breach: 'p99 lt 800ms · got 862 ms' } },
    { n: 477, kind: 'test', st: 'pass', when: 'Today 10:05', ago: '4 h ago', dur: '1m 12s', profile: 'staging', layers: [], branch: 'master', commit: 'e19f04a', by: 'ci', sel: 'all', sched: 'nightly-staging', tot: t(118, 0, 0, 6, 2, 1) },
    { n: 476, kind: 'test', st: 'fail', when: 'Yesterday 18:40', ago: '20 h ago', dur: '44.0s', profile: 'local', layers: ['eu'], branch: 's/checkout-v2', commit: '3b7a0c5', by: 'cursor', sel: 'orders.checkout', tot: t(7, 1, 1, 0, 0, 0) },
    { n: 475, kind: 'test', st: 'blocked', when: 'Yesterday 16:22', ago: '22 h ago', dur: '3.1s', profile: 'dev', layers: [], branch: 'master', commit: 'c02e771', by: 'ci', sel: 'payments', tot: t(0, 0, 6, 0, 0, 0) },
    { n: 474, kind: 'test', st: 'pass', when: 'Yesterday 09:15', ago: '1 d ago', dur: '1m 09s', profile: 'staging', layers: [], branch: 'master', commit: 'c02e771', by: 'ci', sel: 'all', tot: t(119, 0, 0, 6, 1, 1) },
    { n: 473, kind: 'test', st: 'pass', when: 'Sat 21 Nov 22:00', ago: '2 d ago', dur: '58.2s', profile: 'prod', layers: [], branch: 'master', commit: 'c02e771', by: 'ci', sel: 'tag:smoke', sched: 'prod-smoke', tot: t(14, 0, 0, 0, 0, 0) },
    { n: 472, kind: 'load', st: 'pass', when: 'Fri 20 Nov 17:30', ago: '3 d ago', dur: '7m 00s', profile: 'staging', layers: [], branch: 'master', commit: '90d1a3e', by: 'you', sel: 'checkout.peak', load: { iters: 671900, errs: 310, breach: null } },
    { n: 471, kind: 'test', st: 'fail', when: 'Fri 20 Nov 11:15', ago: '3 d ago', dur: '40.3s', profile: 'local', layers: [], branch: 's/refunds', commit: '5e6f2aa', by: 'you', sel: 'payments.refunds', tot: t(4, 2, 0, 0, 0, 0) }
  ];

  /* ---- schedules (vibranium.config.yaml › schedules:) ---------------------------
     The vc server runs them while `vc ui` or `vc serve` is up; each run lands in history.
     runner 'ci' means the schedule was exported to a GitHub Actions cron workflow. */
  const schedules = [
    { id: 'nightly-staging', cron: '0 2 * * 1-5', words: 'Weekdays 02:00', sel: 'all', profile: 'staging', runner: 'server', next: 'Tomorrow 02:00', last: { n: 477, st: 'pass', ago: '4 h ago' }, on: true, notify: 'slack #qa-alerts' },
    { id: 'prod-smoke', cron: '0 */6 * * *', words: 'Every 6 hours', sel: 'tag:smoke', profile: 'prod', runner: 'ci', next: 'Today 18:00', last: { n: 473, st: 'pass', ago: '2 d ago' }, on: true, notify: 'pagerduty on failure' },
    { id: 'weekly-load', cron: '30 11 * * 4', words: 'Thursdays 11:30', sel: 'checkout.peak', profile: 'staging', runner: 'server', next: 'Thu 3 Dec 11:30', last: { n: 478, st: 'fail', ago: '2 h ago' }, on: true, notify: 'slack #qa-alerts' },
    { id: 'morning-orders', cron: '0 9 * * 1-5', words: 'Weekdays 09:00', sel: 'orders users', profile: 'dev', runner: 'server', next: 'Tomorrow 09:00', last: null, on: false, notify: 'none' }
  ];

  /* ---- one stored run, the same shape a live run streams ---------------------- */
  const rx = (m, url, s, ms, note) => ({ m, url, s, ms, note });
  const runDetail = {
    482: {
      files: [
        { id: 'users.crud', ms: 6120, tests: [['createsUser', 'pass', 188], ['getsCreatedUser', 'pass', 142], ['rejectsBadEmail', 'flaky', 961], ['updatesRole', 'pass', 120], ['deletesUser', 'pass', 64]] },
        { id: 'users.auth', ms: 2210, tests: [['rejectsExpiredToken', 'pass', 77], ['refreshesToken', 'skipped', 0]] },
        { id: 'orders.checkout', ms: 9840, tests: [['placesOrder', 'pass', 402], ['appliesCoupon', 'pass', 233], ['rejectsDeclinedCard', 'fail', 1240], ['refundsPartially', 'blocked', 0], ['listsOpenOrders', 'pass', 77]] },
        { id: 'orders.cancel', ms: 3010, tests: [['cancelsOpenOrder', 'pass', 210], ['rejectsShippedCancel', 'fail', 188]] },
        { id: 'catalog.read', ms: 1980, tests: [['readsProduct', 'allow', 96], ['hidesCostPrice', 'pass', 88]] },
        { id: 'rooms.lobby', ms: 0, tests: [['echoesChatMessage', 'skipped', 0], ['kicksIdleClient', 'skipped', 0]] }
      ],
      tests: {
        'orders.checkout#rejectsDeclinedCard': {
          code: 'V5101', title: 'status: expected 402, got 500',
          checks: [
            { st: 'fail', what: 'status', exp: '402', got: '500' },
            { st: 'fail', what: 'body.error.code', exp: '"card_declined"', got: '"internal"' },
            { st: 'pass', what: 'headers.content-type', exp: 'startsWith("application/json")', got: 'application/json; charset=utf-8' },
            { st: 'skipped', what: 'body.order.status', exp: '"payment_failed"', got: 'not reached' }
          ],
          span: { file: 'services/orders/checkout.test.yaml', line: 41, lines: [
            [38, '  - id: rejectsDeclinedCard'], [39, '    call: payments#charge'], [40, '    vars: { card: "4000 0000 0000 0002" }'],
            [41, '    expect:'], [42, '      status: 402', 'hit'], [43, '      body.error.code: card_declined']] },
          help: 'the sandbox returns 500 when the card token has expired. fix: refresh ${var.card} in orders.checkout beforeAll, or mark the test allowFailure until the sandbox is fixed.',
          exchanges: [
            rx('POST', '${baseUrl}/users', 201, 182, 'beforeAll · buyer'),
            rx('POST', '${baseUrl}/orders', 201, 233, 'step 1 · placesOrder'),
            rx('POST', '${baseUrl}/payments/charges', 500, 611, 'attempt 1 of 2'),
            rx('POST', '${baseUrl}/payments/charges', 500, 598, 'attempt 2 of 2 · final'),
            rx('DELETE', '${baseUrl}/orders/o_51e2', 204, 41, 'cleanup')
          ],
          reqHeaders: [['content-type', 'application/json'], ['authorization', 'Bearer ••••'], ['idempotency-key', 'idem_7c2f'], ['x-client', 'vibranium']],
          diff: [['@', '@@ body @@'], [' ', '{'], ['-', '  "error": { "code": "card_declined", "decline": "generic" },'], ['+', '  "error": { "code": "internal", "trace": "tr_9a1c" },'], [' ', '  "order": {'], ['-', '    "status": "payment_failed"'], ['+', '    "status": "pending"'], [' ', '  }'], [' ', '}']]
        },
        'orders.cancel#rejectsShippedCancel': {
          code: 'V5101', title: 'body.error.code: expected "already_shipped", got "not_found"',
          checks: [
            { st: 'pass', what: 'status', exp: '409', got: '409' },
            { st: 'fail', what: 'body.error.code', exp: '"already_shipped"', got: '"not_found"' }
          ],
          span: { file: 'services/orders/cancel.test.yaml', line: 22, lines: [[20, '  - id: rejectsShippedCancel'], [21, '    call: orders#cancel'], [22, '    expect:'], [23, '      status: 409'], [24, '      body.error.code: already_shipped', 'hit']] },
          help: 'the order id came from beforeAll, which ran before the ship step finished. fix: add `after: shipsOrder` to the test.',
          exchanges: [rx('POST', '${baseUrl}/orders', 201, 140, 'beforeAll'), rx('POST', '${baseUrl}/orders/o_51e3/cancel', 409, 48, 'final')],
          reqHeaders: [['authorization', 'Bearer ••••'], ['x-client', 'vibranium']],
          diff: [['@', '@@ body @@'], [' ', '{ "error": {'], ['-', '  "code": "already_shipped"'], ['+', '  "code": "not_found"'], [' ', '} }']]
        },
        'orders.checkout#refundsPartially': { code: 'V3101', title: 'blocked: needs placesOrder#chargeId, which rejectsDeclinedCard did not save', checks: [], blockedBy: 'orders.checkout#rejectsDeclinedCard' }
      }
    }
  };

  /* ---- compare 481 → 482 ------------------------------------------------------ */
  const compare = {
    base: 481, head: 482,
    newlyFailing: [
      { id: 'orders.checkout#rejectsDeclinedCard', was: 'pass', now: 'fail', why: 'status 402 → 500' },
      { id: 'orders.cancel#rejectsShippedCancel', was: 'pass', now: 'fail', why: 'body.error.code changed' },
      { id: 'orders.checkout#refundsPartially', was: 'pass', now: 'blocked', why: 'needs rejectsDeclinedCard' }
    ],
    fixed: [{ id: 'users.crud#updatesRole', was: 'fail', now: 'pass', why: 'role enum now accepts "auditor"' }],
    timing: [
      { id: 'orders.checkout#rejectsDeclinedCard', a: 410, b: 1240 }, { id: 'users.crud#rejectsBadEmail', a: 512, b: 961 },
      { id: 'orders.checkout#placesOrder', a: 318, b: 402 }, { id: 'catalog.read#readsProduct', a: 88, b: 96 },
      { id: 'users.crud#createsUser', a: 240, b: 188 }, { id: 'orders.checkout#listsOpenOrders', a: 131, b: 77 }, { id: 'users.auth#rejectsExpiredToken', a: 92, b: 77 }
    ],
    shapes: [
      { api: 'orders#get', path: 'body.items[].discount', change: 'added', type: 'number', seen: 'placesOrder, listsOpenOrders' },
      { api: 'payments#charge', path: 'body.error.decline', change: 'removed', type: 'string', seen: 'rejectsDeclinedCard' },
      { api: 'payments#charge', path: 'body.error.trace', change: 'added', type: 'string', seen: 'rejectsDeclinedCard' },
      { api: 'users#get', path: 'body.role', change: 'type', type: 'string → string | null', seen: 'getsCreatedUser' }
    ]
  };

  /* ---- insights (staging · master · 30 days unless noted) ---------------------- */
  const days = Array.from({ length: 30 }, (_, i) => { const d = new Date(Date.UTC(2026, 9, 25 + i)); return d.toISOString().slice(5, 10); });
  const passRate = [96.1, 95.8, 96.4, 94.0, 95.2, 93.1, 94.8, 95.5, 96.0, 92.4, 91.8, 93.6, 94.9, 95.1, 96.2, 95.7, 94.3, 95.0, 96.6, 97.1, 96.8, 95.9, 93.2, 94.4, 95.8, 96.3, 97.0, 96.5, 95.2, 97.4];
  const duration = [74, 72, 75, 78, 76, 81, 79, 77, 74, 88, 92, 85, 80, 78, 76, 75, 77, 74, 72, 71, 73, 70, 76, 74, 72, 71, 69, 70, 72, 69];
  const insights = {
    days, passRate, duration,
    kpi: [
      { k: 'Pass rate', v: '97.4%', d: '+1.8 pts', good: true, vs: 'vs previous 30 days', spark: null },
      { k: 'Median run', v: '69 s', d: '−6 s', good: true, vs: 'vs previous 30 days', spark: duration },
      { k: 'Flaky tests', v: '4', d: '+1', good: false, vs: 'vs previous 30 days', spark: [2, 2, 3, 3, 2, 3, 3, 4, 4, 3, 4, 4] },
      { k: 'Api coverage', v: '71%', d: '10 of 14 apis', good: null, vs: 'have at least one test', spark: null }
    ],
    flaky: [
      { id: 'users.crud#rejectsBadEmail', rate: 18, runs: 50, strip: 'ppppfppppppppfpppppfpppp', last: 'passed on retry · #482' },
      { id: 'orders.checkout#placesOrder', rate: 9, runs: 46, strip: 'pppppppppfpppppppppppfpp', last: 'timeout on attempt 1 · #474' },
      { id: 'catalog.read#readsProduct', rate: 6, runs: 50, strip: 'pppppppppppppfpppppppppp', last: 'GraphQL errors + data · #470' },
      { id: 'rooms.lobby#echoesChatMessage', rate: 4, runs: 24, strip: 'ppppppppppppppppppfppppp', last: 'ws closed 1006 · #466' }
    ],
    slow: [
      { api: 'payments#charge', m: 'POST', p50: 284, p95: 611 }, { api: 'orders#create', m: 'POST', p50: 212, p95: 402 },
      { api: 'catalog#products', m: 'GQL', p50: 148, p95: 377 }, { api: 'users#create', m: 'POST', p50: 131, p95: 240 },
      { api: 'orders#list', m: 'GET', p50: 77, p95: 190 }, { api: 'users#get', m: 'GET', p50: 61, p95: 142 }
    ],
    gaps: [
      { api: 'payments#refund', m: 'POST', why: 'no test calls it', risk: 'writes money' },
      { api: 'users#update', m: 'PATCH', why: 'only called from a cleanup', risk: '' },
      { api: 'orders#get', m: 'GET', why: 'no test calls it', risk: '' },
      { api: 'catalog#products', m: 'GQL', why: 'every caller is skipped', risk: '' }
    ],
    skips: [
      { id: 'users.auth#refreshesToken', kind: 'skip', reason: 'identity sandbox rotates keys, see #198', until: '2026-11-25', left: 2 },
      { id: 'payments.refunds#refundsTwice', kind: 'allowFailure', reason: 'payments sandbox is down, see #340', until: '2026-11-28', left: 5 },
      { id: 'rooms.lobby#echoesChatMessage', kind: 'skip', reason: 'ws gateway migration', until: '2026-11-20', left: -3 }
    ],
    services: [
      { id: 'users', tests: 7, rate: 96.4, p95: 240, flaky: 1, trend: [95, 96, 94, 97, 96, 95, 97, 96, 96, 97, 96, 96] },
      { id: 'orders', tests: 9, rate: 91.2, p95: 402, flaky: 1, trend: [96, 95, 94, 93, 95, 92, 93, 91, 92, 90, 91, 91] },
      { id: 'payments', tests: 6, rate: 88.0, p95: 611, flaky: 0, trend: [92, 90, 91, 89, 90, 86, 85, 88, 87, 88, 89, 88] },
      { id: 'catalog', tests: 4, rate: 99.0, p95: 377, flaky: 1, trend: [99, 98, 99, 99, 100, 99, 98, 99, 99, 99, 100, 99] },
      { id: 'rooms', tests: 2, rate: 100, p95: 88, flaky: 1, trend: [100, 100, 100, 96, 100, 100, 100, 100, 100, 100, 100, 100] }
    ]
  };

  /* ---- agents (MCP) ---------------------------------------------------------- */
  const agents = {
    server: { http: 'http://127.0.0.1:7420/mcp', since: '09:12', clients: [{ name: 'claude-code', ver: '2.1', via: 'stdio · vc mcp', last: '2 min ago' }, { name: 'cursor', ver: '1.9', via: 'http · /mcp', last: '18 min ago' }] },
    queue: [
      { id: 'q1', agent: 'claude-code', tool: 'write_definition', profile: 'local', at: '14:05', age: '1 min', what: 'services/orders/checkout.test.yaml', why: 'writeDefinitions: ask', digest: '9f2c41e', diff: [['@', '@@ -36,6 +36,10 @@'], [' ', '  - id: rejectsDeclinedCard'], [' ', '    call: payments#charge'], ['+', '    allowFailure:'], ['+', '      reason: "sandbox returns 500 for expired card tokens, see #352"'], ['+', '      until: 2026-12-01'], [' ', '    vars: { card: "4000 0000 0000 0002" }']] },
      { id: 'q2', agent: 'cursor', tool: 'call_api', profile: 'staging', at: '13:59', age: '7 min', what: 'POST payments#refund', why: 'approve: [POST] on staging', body: '{ "chargeId": "ch_3PqZ", "amount": 1200 }' },
      { id: 'q3', agent: 'claude-code', tool: 'run_tests', profile: 'local', at: '13:51', age: '15 min', what: 'orders · 11 tests', why: 'the plan runs a command action', cmd: 'beforeRun: docker compose up -d --wait' }
    ],
    tools: [
      ['list_definitions', 'read'], ['search', 'read'], ['get_definition', 'read'], ['check', 'read'], ['query_history', 'read'], ['explain_failure', 'read'],
      ['run_tests', 'run'], ['call_api', 'call'], ['write_definition', 'write']
    ],
    methods: ['GET', 'POST', 'PUT', 'PATCH', 'DELETE'],
    policy: {
      local: { run: true, m: { GET: 'allow', POST: 'allow', PUT: 'allow', PATCH: 'allow', DELETE: 'allow' }, write: 'ask' },
      dev: { run: true, m: { GET: 'allow', POST: 'allow', PUT: 'ask', PATCH: 'ask', DELETE: 'ask' }, write: 'ask' },
      staging: { run: true, m: { GET: 'allow', POST: 'ask', PUT: 'ask', PATCH: 'ask', DELETE: 'off' }, write: 'off' },
      prod: { run: false, m: { GET: 'off', POST: 'off', PUT: 'off', PATCH: 'off', DELETE: 'off' }, write: 'off', readsOnly: true }
    },
    log: [
      { t: '14:05', agent: 'claude-code', tool: 'write_definition', arg: 'services/orders/checkout.test.yaml', res: 'waiting', profile: 'local' },
      { t: '14:03', agent: 'claude-code', tool: 'explain_failure', arg: 'orders.checkout#rejectsDeclinedCard · run 482', res: 'ok', profile: 'local' },
      { t: '13:59', agent: 'cursor', tool: 'call_api', arg: 'POST payments#refund', res: 'waiting', profile: 'staging' },
      { t: '13:12', agent: 'claude-code', tool: 'run_tests', arg: 'tag:smoke → run 480 · 9 passed', res: 'ok', profile: 'local', run: 480 },
      { t: '13:08', agent: 'claude-code', tool: 'write_definition', arg: 'services/users/crud.test.yaml', res: 'approved', profile: 'local', by: 'you', diff: [['@', '@@ -51,3 +51,8 @@'], [' ', '  - id: deletesUser'], [' ', '    call: users#delete'], ['+', '  - id: rejectsDuplicateEmail'], ['+', '    call: users#create'], ['+', '    vars: { email: "${var.buyer.email}" }'], ['+', '    expect:'], ['+', '      status: 409']] },
      { t: '12:40', agent: 'cursor', tool: 'call_api', arg: 'DELETE users#delete', res: 'refused', profile: 'staging', note: 'DELETE is off for staging' },
      { t: '11:02', agent: 'cursor', tool: 'query_history', arg: 'flaky · last 50 runs', res: 'ok', profile: 'staging' }
    ]
  };

  /* ---- agent usage, last 7 days (today, Thursday, is the last point) ----------------- */
  const usage = {
    days: ['Fri', 'Sat', 'Sun', 'Mon', 'Tue', 'Wed', 'Thu'],
    agents: [
      { id: 'claude-code', req: [42, 8, 3, 61, 74, 58, 49], tools: [118, 21, 9, 170, 205, 161, 132], runs: [4, 1, 0, 6, 7, 5, 4], tokens: '1.9M', time: '3 h 12 min' },
      { id: 'cursor', req: [18, 2, 0, 22, 31, 27, 14], tools: [40, 4, 0, 51, 66, 58, 29], runs: [1, 0, 0, 2, 2, 1, 1], tokens: '640k', time: '1 h 05 min' }
    ],
    byTool: [
      { agent: 'claude-code', tool: 'explain_failure', calls: 212, ok: 212, refused: 0, asked: 0 },
      { agent: 'claude-code', tool: 'run_tests', calls: 27, ok: 27, refused: 0, asked: 0 },
      { agent: 'claude-code', tool: 'write_definition', calls: 19, ok: 14, refused: 1, asked: 19 },
      { agent: 'claude-code', tool: 'get_definition', calls: 486, ok: 486, refused: 0, asked: 0 },
      { agent: 'cursor', tool: 'call_api', calls: 96, ok: 88, refused: 6, asked: 11 },
      { agent: 'cursor', tool: 'query_history', calls: 61, ok: 61, refused: 0, asked: 0 },
      { agent: 'cursor', tool: 'run_tests', calls: 7, ok: 7, refused: 0, asked: 0 }
    ]
  };

  /* ---- settings ---------------------------------------------------------------- */
  const settings = {
    digest: 'b3f1c27',
    general: { project: 'shop', syntaxVersion: 2, hooksThreads: 1 },
    paths: { include: 'services/**\ndatasets/**', exclude: 'node_modules\n.vibranium\nservices/**/legacy/**' },
    defaults: { timeout: '10s', concurrency: 32, perHost: 16, retryAttempts: 0, retryDelay: '1s', headers: [['accept', 'application/json'], ['x-client', 'vibranium']] },
    profiles: [
      { id: 'local', name: 'Local mock', def: true, vars: 3, agents: 'run · all methods' },
      { id: 'dev', name: 'Shared dev', def: false, vars: 3, agents: 'run · GET POST' },
      { id: 'staging', name: 'Staging', def: false, vars: 4, agents: 'run · GET' },
      { id: 'prod', name: 'Production', def: false, vars: 4, agents: 'reads only' }
    ],
    vars: [
      { k: 'baseUrl', base: 'http://localhost:7421', local: null, dev: 'https://dev.shop.internal', staging: 'https://staging.shop.example', prod: 'https://api.shop.example' },
      { k: 'tenantId', base: 't_shop_01', local: null, dev: null, staging: 't_stg_01', prod: 't_live_01' },
      { k: 'identityUrl', base: 'http://localhost:7421/id', local: null, dev: 'https://id.dev.shop.internal', staging: 'https://id.staging.shop.example', prod: 'https://id.shop.example' },
      { k: 'currency', base: 'USD', local: null, dev: null, staging: null, prod: 'USD' }
    ],
    http: { attempts: 2, on: 'connect, timeout, 502, 503, 504', methods: 'GET, HEAD, OPTIONS, PUT, DELETE', base: '100ms', max: '2s', jitter: true },
    tls: { ca: 'certs/shop-ca.pem', clients: [{ host: '*.partners.shop.example', cert: 'certs/partner.crt', key: 'PARTNER_KEY', src: 'keychain', exp: '2027-02-11' }] },
    auth: [
      { id: 'shopAuth', type: 'oauth2ClientCredentials', ico: 'shield', fields: [['tokenUrl', '${var.identityUrl}/oauth/token'], ['clientId', 'shop-tests'], ['clientSecret', null, 'OAUTH_CLIENT_SECRET', 'keychain'], ['scopes', 'users:read users:write catalog:read']] },
      { id: 'awsGateway', type: 'awsSigV4', ico: 'aws', fields: [['region', 'eu-west-1'], ['service', 'execute-api'], ['accessKeyId', null, 'AWS_ACCESS_KEY_ID', 'env'], ['secretAccessKey', null, 'AWS_SECRET_ACCESS_KEY', 'env']] },
      { id: 'partnerMtls', type: 'mtls', ico: 'cert', fields: [['clientCert', 'partner (http.tls.clients[0])']] }
    ],
    secrets: [
      { name: 'API_TOKEN', src: 'keychain', set: true, used: 4 },
      { name: 'OAUTH_CLIENT_SECRET', src: 'keychain', set: true, used: 1 },
      { name: 'AWS_ACCESS_KEY_ID', src: 'env', set: true, used: 1 },
      { name: 'AWS_SECRET_ACCESS_KEY', src: 'env', set: true, used: 1 },
      { name: 'PARTNER_KEY', src: 'keychain', set: true, used: 1 },
      { name: 'SLACK_WEBHOOK', src: '.env', set: true, used: 1 },
      { name: 'PD_ROUTING_KEY', src: 'env', set: false, used: 1, optional: true }
    ],
    reports: { dir: '.vibranium/reports', formats: ['ctrf', 'junit', 'html'], layout: 'folder', bodies: 'failures', maxBodySize: '64kb', keep: 50 },
    alerts: [
      { on: ['failure', 'recovery', 'flaky'], profiles: ['staging', 'prod'], tags: [], ch: [['slack', '#qa-alerts', 'SLACK_WEBHOOK']] },
      { on: ['threshold'], profiles: ['staging'], tags: ['load'], ch: [['pagerduty', 'checkout on-call', 'PD_ROUTING_KEY'], ['webhook', 'https://hooks.shop.example/vc', null]] }
    ],
    plugins: [
      { id: 'htmlReport', kind: 'reporter', source: 'npm:@acme/vibranium-html@^1', resolved: '1.2.0', perms: ['network: reports.acme.io', 'secrets: reportToken', 'files: reports/'], ok: true },
      { id: 'acmeAuth', kind: 'auth', source: 'npm:@acme/vibranium-auth@2', resolved: '2.0.4', perms: ['network: id.acme.io'], ok: true },
      { id: 'grpcExec', kind: 'executor', source: 'path:plugins/grpc', resolved: 'blake3-41c0…', perms: ['network: *.shop.internal'], ok: false, note: 'V5118 · files changed since vc plugin add' }
    ],
    user: { profile: 'local', timeout: '', concurrency: '8', reportsDir: '', formats: '', allowUnconfined: false, vars: [['buyerEmail', 'sarath+buyer@shop.test']] }
  };

  agents.usage = usage;
  VC.data.shared = { runs, runDetail, compare, insights, agents, settings, schedules };
})();
