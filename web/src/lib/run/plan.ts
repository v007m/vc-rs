import type { PlannedInstance, TestFinished } from '#lib/gen/vibranium/engine/v1/events_pb.ts';
import { TestStatus } from '#lib/gen/vibranium/results/v1/results_pb.ts';
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

/** Where each planned instance's row is, by its plan number, which actions and finished tests name. */
export type PlanIndex = ReadonlyMap<number, number>;

/** The index of an empty plan. */
export const noPlanIndex: PlanIndex = new Map();

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
  return new Map(instances.map(({ instance }, at) => [instance, at]));
}

/** The plan with `instance` marked running, when it was queued; else the same plan. */
export function markRunning(
  plan: readonly PlanRow[],
  index: PlanIndex,
  instance: number,
): readonly PlanRow[] {
  const at = index.get(instance) ?? -1;
  const row = plan[at];
  if (row?.state !== 'queued') return plan;
  return plan.with(at, { ...row, state: 'running' });
}

/**
 * The plan with the finished instance marked done; a test that was never planned (it has no
 * instance) or has no outcome changes nothing.
 */
export function markFinished(
  plan: readonly PlanRow[],
  index: PlanIndex,
  { test, instance }: TestFinished,
): readonly PlanRow[] {
  const state = test && outcomeOf(test.status);
  const at = instance === undefined ? -1 : (index.get(instance) ?? -1);
  const row = plan[at];
  if (!test || !state || !row) return plan;
  return plan.with(at, { ...row, state, ms: test.durationMs });
}
