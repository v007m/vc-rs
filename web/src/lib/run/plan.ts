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

/** The plan's rows, all queued, in plan order. */
export function planRows(instances: readonly PlannedInstance[]): PlanRow[] {
  return instances.map(({ instance, label, test }) => ({
    instance,
    label: label || test,
    state: 'queued',
  }));
}

/** The plan with `instance` marked running, when it was queued; else the same plan. */
export function markRunning(plan: readonly PlanRow[], instance: number): readonly PlanRow[] {
  const at = plan.findIndex((row) => row.instance === instance);
  if (plan[at]?.state !== 'queued') return plan;
  return plan.with(at, { ...plan[at], state: 'running' });
}

/** The plan with the instance `test` ran as marked done; a test not in the plan changes nothing. */
export function markFinished(plan: readonly PlanRow[], test: Test): readonly PlanRow[] {
  const state = outcomeOf(test.status);
  const at = plan.findIndex((row) => row.label === test.label);
  const row = plan[at];
  if (!state || !row) return plan;
  return plan.with(at, { ...row, state, ms: test.durationMs });
}
