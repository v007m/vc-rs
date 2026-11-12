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

/** One api over the window: every call, and p50 and p95 over those that got a response. */
export type ApiTrend = Readonly<{
  apiId: string;
  method: string;
  calls: number;
  /** Absent when no call got a response. */
  p50Ms?: number;
  p95Ms?: number;
  /** The statuses its responses had, each once, lowest first. */
  statuses: readonly number[];
}>;

/** One api, or one operation only a spec snapshot has, with what tests and documents it (D300). */
export type CoveredApi = Readonly<{
  /** `users#get`; an operation only a snapshot has is named by its `operationId`. */
  apiId: string;
  method: string;
  /** As written, `/users/${var.userId}`; the spec's for an operation only a snapshot has. */
  path: string;
  /** Its api file, or the snapshot. */
  file: string;
  /** The tests that call it, skipped and `only`-filtered ones included (D301). */
  tests: number;
  skipped: number;
  /** `2xx`, `200`: the statuses its `expect`s and its snapshot's responses name. */
  documented: readonly string[];
  specOnly: boolean;
}>;

/** Every api and spec operation, and a note for each snapshot that is missing or unreadable. */
export type Coverage = Readonly<{ apis: readonly CoveredApi[]; notes: readonly string[] }>;

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
  /** Each api over the same runs, shared and lifecycle calls included, in api ID order. */
  apiTrends(window: number, scope: Scope): Promise<ApiTrend[]>;
  /** The skips that have an end date, expired ones included. */
  skips(): Promise<SkipLine[]>;
  /** What tests and documents each api; read from the project, not the history. */
  coverage(): Promise<Coverage>;
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

/** The port over the engine client's ListRuns, Trends, ApiTrends, ApiCoverage and ListDefinitions. */
export function enginePort(
  client: Pick<
    Client<typeof EngineService>,
    'listRuns' | 'trends' | 'apiTrends' | 'apiCoverage' | 'listDefinitions'
  >,
): InsightsPort {
  return {
    async apiTrends(window, scope) {
      const { trends } = await client.apiTrends({ window, ...scope });
      return trends.map(({ apiId, method, calls, p50Ms, p95Ms, statuses }) => ({
        apiId,
        method,
        calls,
        ...(p50Ms === undefined ? {} : { p50Ms }),
        ...(p95Ms === undefined ? {} : { p95Ms }),
        statuses,
      }));
    },
    async coverage() {
      const { apis, notes } = await client.apiCoverage({});
      const covered = apis.map((api) => ({
        apiId: api.apiId,
        method: api.method,
        path: api.path,
        file: api.file,
        tests: api.tests,
        skipped: api.skipped,
        documented: api.documented,
        specOnly: api.specOnly,
      }));
      return { apis: covered, notes };
    },
    async runs(limit, scope) {
      const { runs } = await client.listRuns({ limit, kind: 'run', ...scope });
      return runs.map(summary);
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
