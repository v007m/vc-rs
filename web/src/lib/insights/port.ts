import type { Client } from '@connectrpc/connect';
import { DefinitionKind, type EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
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
  /** The git branch the run was on, when known. */
  branch?: string;
}>;

/** One test over the window: results that ran, how many failed or were blocked, how many flaky. */
export type TestTrend = Readonly<{
  testId: string;
  results: number;
  failed: number;
  flaky: number;
  meanMs: number;
}>;

/**
 * A test whose written `skip` has an end date: its full ID, `skip.until` (`YYYY-MM-DD`), reason,
 * and whether the date has passed, so the test runs again and only the line is left.
 */
export type SkipLine = Readonly<{ id: string; until: string; reason?: string; ended: boolean }>;

/** Which runs Insights reads: under one profile, on one branch; every one when absent. */
export type Scope = Readonly<{ profile?: string; branch?: string }>;

/** What Insights asks of the history (G15). */
export interface InsightsPort {
  /** The newest `limit` runs of kind `run` in `scope`, newest first. */
  runs(limit: number, scope: Scope): Promise<RunSummary[]>;
  trends(window: number, scope: Scope): Promise<TestTrend[]>;
  /** The profiles the config defines. */
  profiles(): Promise<string[]>;
  /** The skips that have an end date, expired ones included. */
  skips(): Promise<SkipLine[]>;
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
  const branch = run.gitBranch === undefined ? {} : { branch: run.gitBranch };
  const finished =
    run.finishedMs === undefined ? {} : { durationMs: Number(run.finishedMs - run.startedMs) };
  return { ...base, ...branch, ...finished };
}

/** The port over the engine client's ListRuns, Trends, GetProject and ListDefinitions. */
export function enginePort(
  client: Pick<
    Client<typeof EngineService>,
    'listRuns' | 'trends' | 'getProject' | 'listDefinitions'
  >,
): InsightsPort {
  return {
    async runs(limit, scope) {
      const { runs } = await client.listRuns({ limit, kind: 'run', ...scope });
      return runs.map(summary);
    },
    async profiles() {
      return (await client.getProject({})).profiles;
    },
    async skips() {
      const { definitions } = await client.listDefinitions({
        kind: DefinitionKind.TESTS,
        skipped: true,
      });
      return definitions.flatMap(({ id, skipReason, skipUntil, skipExpired }) => {
        if (!skipUntil) return [];
        const reason = skipReason ? { reason: skipReason } : {};
        return [{ id, until: skipUntil, ...reason, ended: skipExpired }];
      });
    },
    async trends(window, scope) {
      const { trends } = await client.trends({ window, ...scope });
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
