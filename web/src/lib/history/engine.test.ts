import { describe, expect, it } from 'vitest';
import { TestStatus } from '#lib/gen/vibranium/results/v1/results_pb.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { engineRunsSeams } from './engine.ts';
import { compareNotYet, historyNotYet } from './seams.ts';

describe('engineRunsSeams', () => {
  it('reads CompareRuns as status, timing and shape changes', async () => {
    const seams = engineRunsSeams(
      fakeEngine({
        compareRuns: (req) => {
          expect([req.a, req.b, req.slowerThreshold]).toEqual([3n, 7n, 0.2]);
          return {
            newlyFailing: [
              {
                testId: 'a',
                label: 'users.crud#createsUser',
                before: TestStatus.PASSED,
                after: TestStatus.FAILED,
                afterMs: 12,
              },
            ],
            newlyBlocked: [
              { testId: 'b', label: 'users.crud#deletesUser', after: TestStatus.BLOCKED },
            ],
            fixed: [
              {
                testId: 'c',
                label: 'orders.list#pages',
                before: TestStatus.FAILED,
                after: TestStatus.PASSED,
              },
            ],
            slower: [{ testId: 'a', label: 'users.crud#createsUser', beforeMs: 100, afterMs: 212 }],
            shapes: [
              {
                apiId: 'users#get',
                status: 200,
                added: ['$.nickname'],
                removed: [],
                retyped: [{ path: '$.age', before: 'number', after: 'string' }],
              },
            ],
          };
        },
      }),
    );
    const result = await seams.compare.compare(3, 7);
    expect(result.available && result.value).toEqual({
      newlyFailing: [
        { id: 'users.crud#createsUser', was: 'passed', now: 'failed' },
        {
          id: 'users.crud#deletesUser',
          was: 'skipped',
          now: 'blocked',
          why: 'Not in the base run',
        },
      ],
      fixed: [{ id: 'orders.list#pages', was: 'failed', now: 'passed' }],
      timing: [{ id: 'users.crud#createsUser', baseMs: 100, headMs: 212 }],
      shapes: [
        { api: 'users#get · 200', path: '$.nickname', change: 'added', type: '' },
        { api: 'users#get · 200', path: '$.age', change: 'type', type: 'number → string' },
      ],
    });
  });

  it('reads TestHistory as strip points, a run without the test as a gap', async () => {
    const seams = engineRunsSeams(
      fakeEngine({
        testHistory: (req) => {
          expect([req.test, req.limit]).toEqual(['users.crud#createsUser', 30]);
          return {
            runs: [
              { run: { id: 9n, startedMs: 5n }, status: TestStatus.PASSED, durationMs: 40 },
              { run: { id: 8n, startedMs: 4n }, status: TestStatus.UNSPECIFIED },
            ],
          };
        },
      }),
    );
    const result = await seams.tests.history('users.crud#createsUser', 30);
    expect(result.available && result.value).toEqual([
      { run: 9, status: 'passed', ms: 40, startedMs: 5 },
      { run: 8, status: 'skipped', ms: 0, startedMs: 4 },
    ]);
  });

  it('says why when the engine is older than CompareRuns and TestHistory', async () => {
    const seams = engineRunsSeams(fakeEngine({}));
    expect(await seams.compare.compare(1, 2)).toEqual({ available: false, reason: compareNotYet });
    expect(await seams.tests.history('a#b', 30)).toEqual({
      available: false,
      reason: historyNotYet,
    });
  });
});
