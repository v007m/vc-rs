import type { Client } from '@connectrpc/connect';
import { checkLine, exchangeLine } from '#lib/api/results.ts';
import type { EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import {
  RunStatus,
  TestStatus,
  type Run,
  type Test,
} from '#lib/gen/vibranium/results/v1/results_pb.ts';
import type { Counts, Status } from '#lib/results/status.ts';
import type { CheckLine, ExchangeLine, TestRow } from '#lib/results/types.ts';

/** How a stored run ended; `unfinished` while it is going or when it never finished. */
export type RunOutcome = 'passed' | 'failed' | 'cancelled' | 'unfinished';

/** A run as the history keeps it (`ListRuns`, `GetRun`). */
export type RunLine = Readonly<{
  id: number;
  /** `run`, `load` and so on. */
  kind: string;
  startedMs: number;
  /** Absent while the run is going or when it never finished. */
  durationMs?: number;
  profiles: readonly string[];
  selector: string;
  branch?: string;
  commit?: string;
  outcome: RunOutcome;
  counts: Counts;
}>;

/** One stored test instance: its list row, why it did not pass, its checks and exchanges. */
export type TestDetail = Readonly<{
  row: TestRow;
  /** The test's full ID, for running it again. */
  id: string;
  attempts: number;
  failure?: string;
  blockedBy?: string;
  skipReason?: string;
  checks: readonly CheckLine[];
  exchanges: readonly ExchangeLine[];
}>;

export type StoredRun = Readonly<{ run: RunLine; tests: readonly TestDetail[] }>;

/** What the Runs screen asks of the history. Every value is already masked by the engine. */
export interface HistoryPort {
  /** The newest `limit` runs of every kind, newest first. */
  runs(limit: number): Promise<RunLine[]>;
  /** One run by its number, with its tests. */
  run(id: number): Promise<StoredRun>;
}

const outcomes: Readonly<Record<RunStatus, RunOutcome>> = {
  [RunStatus.UNSPECIFIED]: 'unfinished',
  [RunStatus.PASSED]: 'passed',
  [RunStatus.FAILED]: 'failed',
  [RunStatus.CANCELLED]: 'cancelled',
};

const statuses: Readonly<Record<TestStatus, Status>> = {
  [TestStatus.UNSPECIFIED]: 'skipped',
  [TestStatus.PASSED]: 'passed',
  [TestStatus.FAILED]: 'failed',
  [TestStatus.BLOCKED]: 'blocked',
  [TestStatus.FLAKY]: 'flaky',
  [TestStatus.SKIPPED]: 'skipped',
  // It ran and did not pass (D294); the allowed-failure badge comes with D248's web side.
  [TestStatus.ALLOWED_FAILURE]: 'failed',
};

function runLine(run: Run): RunLine {
  const { passed = 0, failed = 0, blocked = 0, flaky = 0, skipped = 0 } = run.totals ?? {};
  const finished =
    run.finishedMs === undefined ? {} : { durationMs: Number(run.finishedMs - run.startedMs) };
  return {
    id: Number(run.id),
    kind: run.kind,
    startedMs: Number(run.startedMs),
    ...finished,
    profiles: run.profiles,
    selector: run.selector,
    branch: run.gitBranch,
    commit: run.gitCommit,
    outcome: outcomes[run.status] ?? 'unfinished',
    counts: { passed, failed, blocked, flaky, skipped },
  };
}

function testDetail(test: Test): TestDetail {
  return {
    row: {
      key: test.label || test.id,
      label: test.label || test.id,
      file: test.id.split('#')[0] ?? '',
      status: statuses[test.status] ?? 'skipped',
      ms: test.durationMs,
      tags: [],
    },
    id: test.id,
    attempts: test.attempts,
    failure: test.failure?.message,
    blockedBy: test.blockedBy,
    skipReason: test.skipReason,
    checks: test.checks.map(checkLine),
    exchanges: test.exchanges.map(exchangeLine),
  };
}

/** The port over the engine client's ListRuns and GetRun. */
export function enginePort(
  client: Pick<Client<typeof EngineService>, 'listRuns' | 'getRun'>,
): HistoryPort {
  return {
    async runs(limit) {
      const { runs } = await client.listRuns({ limit });
      return runs.map(runLine);
    },
    async run(id) {
      const { run, tests } = await client.getRun({ id: BigInt(id) });
      if (!run) throw new Error(`Run #${id} is not in the history.`);
      return { run: runLine(run), tests: tests.map(testDetail) };
    },
  };
}
