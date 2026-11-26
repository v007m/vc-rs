import type { MessageInitShape } from '@bufbuild/protobuf';
import { describe, expect, it } from 'vitest';
import type { ListRunsRequest } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import {
  RunStatus,
  TestStatus,
  type Run,
  type RunSchema,
} from '#lib/gen/vibranium/results/v1/results_pb.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { enginePort } from './port.ts';

const RUNS: MessageInitShape<typeof RunSchema>[] = [
  {
    id: 7n,
    kind: 'run',
    startedMs: 1_790_000_000_000n,
    finishedMs: 1_790_000_001_500n,
    profiles: ['staging'],
    selector: 'tag:smoke',
    gitBranch: 'main',
    gitCommit: '7d41e0a9c2',
    status: RunStatus.FAILED,
    totals: { passed: 1, failed: 1, blocked: 1 },
  },
  { id: 6n, kind: 'load', startedMs: 1_789_000_000_000n, status: RunStatus.PASSED },
];

describe('enginePort', () => {
  it('reads a stored run with its tests, and refuses one the history does not have', async () => {
    const run: Run | undefined = undefined;
    const port = enginePort(
      fakeEngine({ listRuns: () => ({ runs: RUNS }), getRun: () => ({ run, tests: [] }) }),
    );
    const [newest, load] = await port.runs(20);
    expect(newest).toMatchObject({
      id: 7,
      durationMs: 1500,
      outcome: 'failed',
      counts: { passed: 1, failed: 1, blocked: 1, flaky: 0, skipped: 0 },
      commit: '7d41e0a9c2',
    });
    expect(load).toMatchObject({ id: 6, kind: 'load', outcome: 'passed' });
    expect(load?.durationMs).toBeUndefined();
    await expect(port.run(9)).rejects.toThrow('Run #9 is not in the history.');
  });

  it('shows an allowed failure as a test that ran and did not pass', async () => {
    const tests = [{ id: 'users.crud#a', status: TestStatus.ALLOWED_FAILURE }];
    const port = enginePort(fakeEngine({ getRun: () => ({ run: RUNS[0], tests }) }));
    const {
      tests: [allowed],
    } = await port.run(7);
    expect(allowed?.row.status).toBe('failed');
  });

  it('asks the engine for the runs of one kind, profile and branch', async () => {
    const asked: ListRunsRequest[] = [];
    const port = enginePort(
      fakeEngine({
        listRuns: (request) => {
          asked.push(request);
          return { runs: [] };
        },
      }),
    );
    await port.runs(200, { kind: 'load', profile: 'staging', branch: 'main' });
    await port.runs(12);
    expect(asked.map(({ limit, kind, profile, branch }) => [limit, kind, profile, branch])).toEqual(
      [
        [200, 'load', 'staging', 'main'],
        [12, undefined, undefined, undefined],
      ],
    );
  });
});
