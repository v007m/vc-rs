import type { PlannedInstance } from '#lib/gen/vibranium/engine/v1/events_pb.ts';
import { TestStatus, type Test } from '#lib/gen/vibranium/results/v1/results_pb.ts';
import type { Status } from '#lib/results/status.ts';

/** Where a planned test instance is: waiting, under way, or done with an outcome. */
export type PlanState = 'queued' | 'running' | Status;

/** One test instance of the run's plan, as the Runs screen lists it. */
export type PlanRow = Readonly<{
  instance: number;
  label: string;
  state: PlanState;
  ms?: number;
}>;

const outcomes: Partial<Record<TestStatus, Status>> = {
  [TestStatus.PASSED]: 'passed',
  [TestStatus.FAILED]: 'failed',
  [TestStatus.BLOCKED]: 'blocked',
  [TestStatus.FLAKY]: 'flaky',
  [TestStatus.SKIPPED]: 'skipped',
};

/** The outcome of a finished test, or `undefined` when it has none. */
export function outcomeOf(status: TestStatus): Status | undefined {
  return outcomes[status];
}

/**
 * Where each planned instance's row is: by its plan number, which actions name, and by what it
 * runs (test, example row, repeat), which is all a finished test says of its instance.
 */
export type PlanIndex = Readonly<{
  byInstance: ReadonlyMap<number, number>;
  byRun: ReadonlyMap<string, number>;
}>;

/** The index of an empty plan. */
export const noPlanIndex: PlanIndex = { byInstance: new Map(), byRun: new Map() };

/** One instance's test, example row and repeat, as one key. */
function runKey(test: string, row: number | undefined, repeat: number): string {
  return JSON.stringify([test, row ?? null, repeat]);
}

/** The plan's rows, all queued, in plan order. */
export function planRows(instances: readonly PlannedInstance[]): PlanRow[] {
  return instances.map(({ instance, label, test }) => ({
    instance,
    label: label || test,
    state: 'queued',
  }));
}

/** Where each of `instances` is in `planRows(instances)`. */
export function indexPlan(instances: readonly PlannedInstance[]): PlanIndex {
  const byInstance = new Map<number, number>();
  const byRun = new Map<string, number>();
  instances.forEach(({ instance, test, exampleRow, repeatIndex }, at) => {
    byInstance.set(instance, at);
    byRun.set(runKey(test, exampleRow, repeatIndex), at);
  });
  return { byInstance, byRun };
}

/** The plan with `instance` marked running, when it was queued; else the same plan. */
export function markRunning(
  plan: readonly PlanRow[],
  index: PlanIndex,
  instance: number,
): readonly PlanRow[] {
  const at = index.byInstance.get(instance) ?? -1;
  const row = plan[at];
  if (row?.state !== 'queued') return plan;
  return plan.with(at, { ...row, state: 'running' });
}

/** The plan with the instance `test` ran as marked done; a test not in the plan changes nothing. */
export function markFinished(
  plan: readonly PlanRow[],
  index: PlanIndex,
  test: Test,
): readonly PlanRow[] {
  const state = outcomeOf(test.status);
  const at = index.byRun.get(runKey(test.id, test.exampleRow, test.repeatIndex)) ?? -1;
  const row = plan[at];
  if (!state || !row) return plan;
  return plan.with(at, { ...row, state, ms: test.durationMs });
}
