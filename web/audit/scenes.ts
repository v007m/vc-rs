import type { MessageInitShape } from '@bufbuild/protobuf';
import * as E from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import * as R from '#lib/gen/vibranium/results/v1/results_pb.ts';
import { now } from './fixtures.ts';

/** The fakes' richer answers: a compare, a test's history, a load run, diagnostics and git. */

type Changed = MessageInitShape<typeof E.TestChangeSchema>;
const FILE = 'services/users/crud.test.yaml';

const change = (
  label: string,
  before: R.TestStatus,
  after: R.TestStatus,
  ms: [number, number],
): Changed => ({
  testId: label,
  label,
  before,
  after,
  beforeMs: ms[0],
  afterMs: ms[1],
});

export const compareRuns = (): MessageInitShape<typeof E.CompareRunsResponseSchema> => {
  const { PASSED, FAILED, FLAKY, BLOCKED } = R.TestStatus;
  return {
    a: 17n,
    b: 18n,
    newlyFailing: [change('payments.charges#rejectsDeclinedCard', PASSED, FAILED, [190, 212])],
    newlyBlocked: [
      {
        testId: 'payments.refunds#refundsTwice',
        label: 'payments.refunds#refundsTwice',
        after: BLOCKED,
      },
    ],
    fixed: [change('users.crud#rejectsBadEmail', FAILED, FLAKY, [92, 88])],
    slower: [
      change('payments.refunds#refundsPartially', PASSED, PASSED, [96, 171]),
      change('users.crud#createsUser', PASSED, PASSED, [41, 63]),
    ],
    faster: [change('users.crud#listsUsers', PASSED, PASSED, [140, 41])],
    shapes: [
      {
        apiId: 'payments#charge',
        status: 200,
        added: ['$.risk_score'],
        removed: ['$.legacy_id'],
        retyped: [{ path: '$.amount', before: 'number', after: 'string' }],
      },
    ],
  };
};

function historyStatus(i: number): R.TestStatus {
  if (i === 0 || i === 7) return R.TestStatus.FAILED;
  if (i === 4) return R.TestStatus.UNSPECIFIED;
  return i === 11 ? R.TestStatus.FLAKY : R.TestStatus.PASSED;
}

export const testHistory = (): MessageInitShape<typeof E.TestHistoryResponseSchema> => ({
  runs: Array.from({ length: 24 }, (_, i) => ({
    run: { id: BigInt(40 - i), startedMs: now - BigInt(i) * 3_600_000n },
    status: historyStatus(i),
    durationMs: i === 4 ? 0 : 180 + ((i * 37) % 60),
    instances: [],
  })),
});

type LoadAnswer = MessageInitShape<typeof E.RunLoadResponseSchema>;

/** A ramp to 50 users over 60 s, p99 breaching 500 ms between 38 and 44 s. */
export function loadTicks(): LoadAnswer[] {
  return Array.from({ length: 60 }, (_, i) => {
    const s = i + 1;
    const users = Math.min(50, Math.round(s * 2.5));
    const spike = s >= 38 && s <= 44;
    const p99 = spike ? 540 + (s - 38) * 18 : 210 + Math.round(Math.sin(s / 4) * 30 + users * 2);
    const tick = {
      second: BigInt(s),
      users,
      rps: BigInt(Math.round(users * 4 + Math.sin(s / 3) * 6)),
      p50Ms: Math.round(p99 / 4),
      p90Ms: Math.round(p99 / 2.2),
      p95Ms: Math.round(p99 / 1.7),
      p99Ms: p99,
      errors: BigInt(spike ? 6 : s % 9 === 0 ? 1 : 0),
      breaching: spike ? ['p99'] : [],
      host: {
        cpuPercent: Math.round(30 + users * 0.9),
        memoryBytes: BigInt((380 + users * 3) * 1024 * 1024),
        connections: BigInt(users * 2),
        lateIterations: BigInt(spike ? 2 : 0),
      },
    };
    return { kind: { case: 'tick', value: tick } };
  });
}

const latency = (p50Ms: number, p99Ms: number, maxMs: number) => ({
  p50Ms,
  p90Ms: Math.round(p99Ms / 3.6),
  p95Ms: Math.round(p99Ms / 2.9),
  p99Ms,
  maxMs,
});

export function loadReport(): LoadAnswer {
  const report = {
    exit: 3,
    scenario: 'checkout.peak',
    name: 'Checkout at peak',
    ending: E.LoadEnding.FINISHED,
    seconds: 60n,
    iterations: 9_840n,
    errors: 49n,
    rps: 164,
    ...latency(62, 512, 1_204),
    breaches: [
      { metric: 'p99', actual: 512, unit: 'ms', limitJson: '500', shown: '512 ms, over 500 ms' },
    ],
    tests: [
      {
        test: 'checkout.flow#buysOneItem',
        iterations: 6_888n,
        errors: 31n,
        rps: 115,
        ...latency(58, 498, 1_204),
      },
      {
        test: 'checkout.flow#browsesCatalog',
        iterations: 2_952n,
        errors: 18n,
        rps: 49,
        ...latency(71, 540, 980),
      },
    ],
    errorKinds: [
      { api: 'checkout#pay', status: 502, count: 31n },
      { api: 'catalog#search', code: 'V5104', count: 18n },
    ],
    run: 19n,
    seed: 4_207n,
  };
  return { kind: { case: 'report', value: report } };
}

function spanOf(src: string, needle: string) {
  const start = src.indexOf(needle);
  const line = src.slice(0, start).split('\n').length;
  const column = start - src.lastIndexOf('\n', start - 1);
  const span = {
    file: FILE,
    startLine: line,
    startColumn: column,
    endLine: line,
    endColumn: column + needle.length,
  };
  return { start, end: start + needle.length, span };
}

/** The crud file's two problems while they are still in the text: a bad ID and an unknown api. */
export function diagnosticsOf(src: string): MessageInitShape<typeof E.DiagnosticSchema>[] {
  const id = spanOf(src, 'CreatesUser');
  const call = spanOf(src, 'users#createUser\n');
  const all: [typeof id, MessageInitShape<typeof E.DiagnosticSchema>][] = [
    [
      id,
      {
        code: 'V4001',
        severity: E.Severity.ERROR,
        message: 'test IDs are camelCase third-person verbs',
        primary: { span: id.span, text: 'not camelCase' },
        help: ['rename it to createsUser'],
        fixes: [
          {
            title: 'Rename to createsUser',
            certain: true,
            edits: [{ file: FILE, start: id.start, end: id.end, newText: 'createsUser' }],
          },
        ],
      },
    ],
    [
      call,
      {
        code: 'V2003',
        severity: E.Severity.WARNING,
        message: 'unknown api users#createUser; did you mean users#createsUser?',
        primary: {
          span: { ...call.span, endColumn: call.span.endColumn - 1 },
          text: 'no such api',
        },
        fixes: [
          {
            title: 'Use users#createsUser',
            certain: true,
            edits: [
              { file: FILE, start: call.start, end: call.end - 1, newText: 'users#createsUser' },
            ],
          },
        ],
      },
    ],
  ];
  return all.filter(([at]) => at.start >= 0).map(([, d]) => d);
}

const diff = `diff --git a/${FILE} b/${FILE}
@@ -4,7 +4,10 @@ tests:
   - id: createsUser
     name: Creates a user with the given email
-    call: users#createUser
+    call: users#createsUser
+    expect:
+      status: 201
`;

export const vcs = {
  status: (): MessageInitShape<typeof E.VcsStatusResponseSchema> => ({
    branch: 'feat/refunds',
    upstream: 'origin/feat/refunds',
    ahead: 2,
    behind: 1,
    files: [
      { path: FILE, change: E.VcsChange.MODIFIED, added: 4, removed: 1 },
      { path: 'services/payments/refunds/partial.test.yaml', change: E.VcsChange.ADDED, added: 38 },
      { path: 'services/orders/flow.test.yaml', change: E.VcsChange.DELETED, removed: 52 },
    ],
  }),
  diff: (): MessageInitShape<typeof E.VcsDiffResponseSchema> => ({ diff }),
};
