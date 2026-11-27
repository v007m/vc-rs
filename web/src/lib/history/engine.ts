import type { Client } from '@connectrpc/connect';
import { unlessMissing } from '#lib/api/maybe.ts';
import type {
  CompareRunsResponse,
  EngineService,
  ShapeChange as WireShape,
  TestChange,
  TestRunResult,
} from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { TestStatus } from '#lib/gen/vibranium/results/v1/results_pb.ts';
import { statusOf } from './port.ts';
import {
  compareNotYet,
  historyNotYet,
  type HistoryPoint,
  type RunComparison,
  type RunsSeams,
  type ShapeChange,
  type StatusChange,
  type TimingChange,
} from './seams.ts';

export type RunsClient = Pick<Client<typeof EngineService>, 'compareRuns' | 'testHistory'>;

/** The share a test's time must move by to count as slower or faster (D637, D672). */
export const SLOWER_SHARE = 0.2;

function statusChange(change: TestChange): StatusChange {
  const ran = change.before !== TestStatus.UNSPECIFIED;
  return {
    id: change.label || change.testId,
    was: statusOf(change.before),
    now: statusOf(change.after),
    ...(ran ? {} : { why: 'Not in the base run' }),
  };
}

function timingChange(change: TestChange): TimingChange {
  return {
    id: change.label || change.testId,
    baseMs: change.beforeMs ?? 0,
    headMs: change.afterMs,
  };
}

/** One api and status's shape change as a row per field: added, removed or retyped. */
function shapeRows(change: WireShape): ShapeChange[] {
  const api = `${change.apiId} · ${change.status}`;
  return [
    ...change.added.map((path) => ({ api, path, change: 'added' as const, type: '' })),
    ...change.removed.map((path) => ({ api, path, change: 'removed' as const, type: '' })),
    ...change.retyped.map(({ path, before, after }) => ({
      api,
      path,
      change: 'type' as const,
      type: `${before} → ${after}`,
    })),
  ];
}

/** The engine's comparison as the Compare view reads it; newly blocked counts as newly failing. */
export function comparisonOf(response: CompareRunsResponse): RunComparison {
  return {
    newlyFailing: [...response.newlyFailing, ...response.newlyBlocked].map(statusChange),
    fixed: response.fixed.map(statusChange),
    timing: [...response.slower, ...response.faster].map(timingChange),
    shapes: response.shapes.flatMap(shapeRows),
  };
}

/** One test run as a point of the strip; a run that did not run the test is a gap. */
export function pointOf(result: TestRunResult): HistoryPoint {
  return {
    run: Number(result.run?.id ?? 0n),
    status: statusOf(result.status),
    ms: result.durationMs,
    startedMs: Number(result.run?.startedMs ?? 0n),
  };
}

/** The Runs space's seams over `CompareRuns` and `TestHistory` (D672, D673). */
export function engineRunsSeams(client: RunsClient): RunsSeams {
  return {
    compare: {
      compare: (base, head) =>
        unlessMissing(compareNotYet, async () =>
          comparisonOf(
            await client.compareRuns({
              a: BigInt(base),
              b: BigInt(head),
              slowerThreshold: SLOWER_SHARE,
            }),
          ),
        ),
    },
    tests: {
      history: (id, limit) =>
        unlessMissing(historyNotYet, async () => {
          const { runs } = await client.testHistory({ test: id, limit });
          return runs.map(pointOf);
        }),
    },
  };
}
