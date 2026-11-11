import type { Client } from '@connectrpc/connect';
import type { EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import type { Run } from '#lib/gen/vibranium/results/v1/results_pb.ts';

/** A stored `vc run`, as Insights charts it; counts are disjoint (a flaky test is not also passed). */
export type RunSummary = Readonly<{
  id: number;
  startedMs: number;
  /** Absent while the run is going or when it never finished. */
  durationMs?: number;
  passed: number;
  failed: number;
  blocked: number;
  flaky: number;
}>;

/** One test over the window: results that ran, how many failed or were blocked, how many flaky. */
export type TestTrend = Readonly<{
  testId: string;
  results: number;
  failed: number;
  flaky: number;
  meanMs: number;
}>;

/** What Insights asks of the history (G15). */
export interface InsightsPort {
  /** The newest `limit` runs of kind `run`, newest first. */
  runs(limit: number): Promise<RunSummary[]>;
  trends(window: number): Promise<TestTrend[]>;
}

function summary(run: Run): RunSummary {
  const { passed = 0, failed = 0, blocked = 0, flaky = 0 } = run.totals ?? {};
  const base = {
    id: Number(run.id),
    startedMs: Number(run.startedMs),
    passed,
    failed,
    blocked,
    flaky,
  };
  return run.finishedMs === undefined
    ? base
    : { ...base, durationMs: Number(run.finishedMs - run.startedMs) };
}

/** The port over the engine client's ListRuns and Trends. */
export function enginePort(
  client: Pick<Client<typeof EngineService>, 'listRuns' | 'trends'>,
): InsightsPort {
  return {
    async runs(limit) {
      const { runs } = await client.listRuns({ limit, kind: 'run' });
      return runs.map(summary);
    },
    async trends(window) {
      const { trends } = await client.trends({ window });
      return trends.map(({ testId, results, failed, flaky, meanMs }) => ({
        testId,
        results,
        failed,
        flaky,
        meanMs,
      }));
    },
  };
}
