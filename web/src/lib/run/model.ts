import type { RunEvent } from '#lib/gen/vibranium/engine/v1/events_pb.ts';
import { RunStatus, TestStatus, type Totals } from '#lib/gen/vibranium/results/v1/results_pb.ts';
import { doneCount, noCounts, type Counts } from '#lib/results/status.ts';
import { describe, type EventLine } from './events.ts';

export { doneCount, type Counts };

/** Where a run is: not started, streaming, finished with an outcome, or broken off. */
export type RunPhase = 'idle' | 'running' | 'passed' | 'failed' | 'cancelled' | 'error';

/** What the Runs screen shows of one run, built only from its event stream. */
export type RunView = Readonly<{
  phase: RunPhase;
  /** Test instances the run planned, once it started. */
  planned: number;
  counts: Counts;
  /** Event lines, oldest first. */
  events: readonly EventLine[];
  /** The history id, once the report names it. */
  runId?: bigint;
  exit?: number;
  /** Why the stream broke off, when phase is `error`. */
  error?: string;
}>;

const zero: Counts = noCounts;

/** A run that has not started. */
export const idleRun: RunView = { phase: 'idle', planned: 0, counts: zero, events: [] };

/** The view of a run that has just been asked for, before its first event. */
export function startingRun(): RunView {
  return { ...idleRun, phase: 'running' };
}

const countKey: Partial<Record<TestStatus, keyof Counts>> = {
  [TestStatus.PASSED]: 'passed',
  [TestStatus.FAILED]: 'failed',
  [TestStatus.BLOCKED]: 'blocked',
  [TestStatus.FLAKY]: 'flaky',
  [TestStatus.SKIPPED]: 'skipped',
};

const finalPhase: Partial<Record<RunStatus, RunPhase>> = {
  [RunStatus.PASSED]: 'passed',
  [RunStatus.FAILED]: 'failed',
  [RunStatus.CANCELLED]: 'cancelled',
};

/** The counts of a `Totals` message, which may be absent. */
export function countsOf(totals: Totals | undefined): Counts {
  if (!totals) return zero;
  const { passed, failed, blocked, flaky, skipped } = totals;
  return { passed, failed, blocked, flaky, skipped };
}

type Step = (view: RunView, event: RunEvent) => RunView;

/** What each event kind changes besides the event list; kinds not listed change nothing. */
const steps: Partial<Record<NonNullable<RunEvent['kind']['case']>, Step>> = {
  runStarted: (view, { kind }) =>
    kind.case === 'runStarted' ? { ...view, planned: kind.value.instances } : view,
  testFinished: (view, { kind }) => {
    const key = kind.case === 'testFinished' && kind.value.test && countKey[kind.value.test.status];
    return key ? { ...view, counts: { ...view.counts, [key]: view.counts[key] + 1 } } : view;
  },
  runFinished: (view, { kind }) =>
    kind.case === 'runFinished'
      ? {
          ...view,
          counts: countsOf(kind.value.totals),
          phase: finalPhase[kind.value.status] ?? view.phase,
        }
      : view,
  report: (view, { kind }) =>
    kind.case === 'report' ? { ...view, runId: kind.value.run, exit: kind.value.exit } : view,
};

/** The view after one more event of the run's stream. */
export function apply(view: RunView, event: RunEvent): RunView {
  const step = event.kind.case && steps[event.kind.case];
  const next = step ? step(view, event) : view;
  const line = describe(event);
  return line ? { ...next, events: [...next.events, line] } : next;
}

/** The view of a run whose stream broke off with `message`. */
export function broken(view: RunView, message: string): RunView {
  return { ...view, phase: 'error', error: message };
}
