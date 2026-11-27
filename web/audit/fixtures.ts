import type { MessageInitShape } from '@bufbuild/protobuf';
import * as E from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import * as R from '#lib/gen/vibranium/results/v1/results_pb.ts';

/** A small shop project: the data every audited screen shows, with each status in it. */

const now = 1_795_700_000_000n;
const enc = new TextEncoder();

const tests = [
  ['users.crud', 'listsUsers', 'Lists users', ['smoke'], 'GET', '/users'],
  ['users.crud', 'createsUser', 'Creates a user with the given email', ['smoke'], 'POST', '/users'],
  ['users.crud', 'rejectsBadEmail', 'Rejects a bad email', [], 'POST', '/users'],
  [
    'payments.refunds',
    'refundsPartially',
    'Refunds part of a charge',
    ['payments'],
    'POST',
    '/refunds',
  ],
  [
    'payments.charges',
    'rejectsDeclinedCard',
    'Rejects a declined card',
    ['payments'],
    'POST',
    '/charges',
  ],
  ['orders.flow', 'placesAnOrder', 'Places an order end to end', ['e2e'], 'POST', '/orders'],
] as const;

const service = (file: string): string => file.split('.')[0] ?? file;
const area = (file: string): string => file.split('.')[1] ?? file;

export const testDefinitions = tests.map(([file, id, name, tags, method, path]) => ({
  id: `${file}#${id}`,
  name,
  path: `services/${service(file)}/${area(file)}.test.yaml`,
  tags: [...tags],
  calls: [{ api: `${service(file)}.${id}`, method, path }],
  steps: 2,
}));

export const apiDefinitions = tests.map(([file, id, name, , method, path]) => ({
  id: `${service(file)}#${id}`,
  name,
  path: `services/${service(file)}/${service(file)}.api.yaml`,
  calls: [{ api: `${service(file)}#${id}`, method, path }],
}));

export const fileDefinitions = [
  ...[...new Set(testDefinitions.map((d) => d.path))].map((path) => ({ id: path, path })),
  { id: 'checkout.peak', path: 'services/checkout/peak.load.yaml' },
  { id: 'search.soak', path: 'services/search/soak.load.yaml' },
];

const shapes = [
  ['tag:smoke', 'staging', 'main', 1],
  ['users.crud', 'dev', 'feat/refunds', 0],
  ['tag:payments', 'staging', 'feat/refunds', 2],
  ['checkout.peak', 'prod', 'main', 0],
  ['tag:smoke', 'staging', 'main', 0],
  ['orders.flow', 'dev', 'main', 0],
  ['tag:smoke', 'staging', 'main', 1],
  ['payments.charges', 'staging', 'main', 0],
] as const;

export const runs: MessageInitShape<typeof R.RunSchema>[] = shapes.map(
  ([selector, profile, branch, failed], i) => ({
    id: BigInt(18 - i),
    kind: i === 3 ? 'load' : 'run',
    startedMs: now - BigInt(i) * 5_400_000n,
    finishedMs: now - BigInt(i) * 5_400_000n + BigInt(1_200 + i * 340),
    profiles: [profile],
    selector,
    gitBranch: branch,
    gitCommit: ['7d41e0a9c2', '3be19f04aa', 'c0ffee4211'][i % 3] ?? '',
    vcVersion: '2.0.0-dev',
    status: failed ? R.RunStatus.FAILED : R.RunStatus.PASSED,
    totals: { passed: 12 - failed - (i % 2), failed, skipped: 1, flaky: i % 2 },
  }),
);

const adhoc = [
  ['POST', 'http://localhost:7421/users', 201, 0],
  ['GET', 'http://localhost:7421/users/u_1', 200, 1],
  ['GET', 'http://localhost:7421/orders?limit=5', 404, 2],
  ['DELETE', 'http://localhost:7421/users/u_9', 204, 30],
] as const;

export const adhocRuns: MessageInitShape<typeof R.RunSchema>[] = adhoc.map(
  ([method, url, status, hours], i) => ({
    id: BigInt(40 - i),
    kind: 'adhoc',
    startedMs: now - BigInt(hours) * 3_600_000n - BigInt(i) * 60_000n,
    finishedMs: now - BigInt(hours) * 3_600_000n,
    profiles: ['staging'],
    selector: url,
    vcVersion: '2.0.0-dev',
    status: status < 400 ? R.RunStatus.PASSED : R.RunStatus.FAILED,
    call: { method, url, status },
  }),
);

const json = (value: unknown): Uint8Array => enc.encode(JSON.stringify(value));

export function exchange(status: number, ms: number): MessageInitShape<typeof R.ExchangeSchema> {
  return {
    action: 'call',
    method: 'POST',
    url: 'https://staging.shop.dev/charges',
    status,
    timing: { dnsMs: 3, connectMs: 8, tlsMs: 12, firstByteMs: ms - 26, downloadMs: 3, totalMs: ms },
    requestHeaders: [{ name: 'content-type', value: 'application/json' }],
    responseHeaders: [{ name: 'content-type', value: 'application/json' }],
    requestBody: json({ card: '4000 0000 0000 0002', amount: 4250 }),
    responseBody: json({ id: 'ch_91', status: 'succeeded', amount: 4250 }),
    wireSize: 388n,
  };
}

const storedTests = [
  ['payments.charges#rejectsDeclinedCard', R.TestStatus.FAILED, 212],
  ['users.crud#listsUsers', R.TestStatus.PASSED, 41],
  ['users.crud#createsUser', R.TestStatus.PASSED, 63],
  ['users.crud#rejectsBadEmail', R.TestStatus.FLAKY, 88],
  ['payments.refunds#refundsPartially', R.TestStatus.PASSED, 140],
  ['orders.flow#placesAnOrder', R.TestStatus.SKIPPED, 0],
] as const;

function checksOf(id: string, failed: boolean): MessageInitShape<typeof R.CheckSchema>[] {
  return [
    {
      kind: R.CheckKind.STATUS,
      origin: id,
      passed: !failed,
      expected: failed ? '402' : '200',
      actual: '200',
    },
    {
      kind: R.CheckKind.EQUALS,
      target: '$.status',
      origin: id,
      passed: !failed,
      expected: '"card_declined"',
      actual: failed ? '"succeeded"' : '"card_declined"',
    },
  ];
}

export function storedRun(id: bigint): MessageInitShape<typeof E.GetRunResponseSchema> {
  return {
    run: runs.find((r) => r.id === id) ?? runs[0],
    tests: storedTests.map(([test, status, ms]) => {
      const skipped = status === R.TestStatus.SKIPPED;
      const failed = status === R.TestStatus.FAILED;
      return {
        id: test,
        label: test,
        status,
        attempts: status === R.TestStatus.FLAKY ? 2 : 1,
        durationMs: ms,
        ...(failed && {
          failure: { action: 'charge', code: 'V5101', message: 'expected status 402, got 200' },
        }),
        ...(skipped && { skipReason: 'tagged wip' }),
        checks: skipped ? [] : checksOf(test, failed),
        exchanges: skipped ? [] : [exchange(200, ms)],
      };
    }),
  };
}

export const crudYaml = `name: Users CRUD
tags: [smoke]
tests:
  - id: listsUsers
    name: Lists users
    call: users#listUsers
    expect:
      status: 200
  - id: CreatesUser
    name: Creates a user with the given email
    call: users#createUser
    with:
      body:
        email: \${vars.email}
        name: Grace Hopper
    expect:
      status: 201
      body.email: \${vars.email}
    save:
      userId: $.id
`;

export const userBody = {
  id: 'u_1',
  email: 'grace@example.com',
  name: 'Grace Hopper',
  roles: ['admin', 'qa'],
  createdAt: '2026-11-20T09:14:03Z',
  plan: { tier: 'team', seats: 12 },
};

export { enc, json, now };

export const configPath = 'vibranium.config.yaml';
export const configYaml = `project: shop
defaults:
  timeout: 10s
  concurrency: 8
vars:
  baseUrl: https://staging.shop.dev
  adminEmail: grace@example.com
profiles:
  dev: {}
  staging: {}
  prod: {}
`;

const fromFile = (key: string, value: string) => ({
  key,
  layers: [{ value, source: configPath }],
});

/** What ExplainConfig answers: each key the config file sets. */
export const configValues = [
  fromFile('defaults.timeout', '10s'),
  fromFile('defaults.concurrency', '8'),
  fromFile('vars.baseUrl', 'https://staging.shop.dev'),
  fromFile('vars.adminEmail', 'grace@example.com'),
];

/** What ApiCoverage answers: every api, one of them untested. */
export const coverage = apiDefinitions.map(({ id, path, calls }, i) => ({
  apiId: id,
  method: calls[0]?.method ?? 'GET',
  path: calls[0]?.path ?? '/',
  file: path,
  tests: i === 2 ? 0 : 1 + (i % 3),
  skipped: i === 4 ? 1 : 0,
}));

/** What Trends answers: each test over the window, one flaky and one failing. */
export const trends = testDefinitions.map(({ id }, i) => ({
  testId: id,
  results: 8,
  failed: i === 4 ? 3 : 0,
  flaky: i === 1 ? 1 : 0,
  meanMs: 40 + i * 37,
}));

/** What ApiTrends answers: each api's calls, latency and statuses seen. */
export const apiTrends = apiDefinitions.map(({ id, calls }, i) => ({
  apiId: id,
  method: calls[0]?.method ?? 'GET',
  calls: 8 + i * 3,
  p50Ms: 30 + i * 22,
  p95Ms: 80 + i * 61,
  statuses: i === 4 ? [402, 201] : [calls[0]?.method === 'POST' ? 201 : 200],
}));
