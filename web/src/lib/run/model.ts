import type { RunEvent } from '#lib/gen/vibranium/engine/v1/events_pb.ts';
import { RunStatus, type Totals } from '#lib/gen/vibranium/results/v1/results_pb.ts';
import { doneCount, noCounts, type Counts } from '#lib/results/status.ts';
import { describe, type EventLine } from './events.ts';
import { markFinished, markRunning, outcomeOf, planRows, type PlanRow } from './plan.ts';

export { doneCount, type Counts };

/** Where a run is: not started, streaming, finished with an outcome, or broken off. */
export type RunPhase = 'idle' | 'running' | 'passed' | 'failed' | 'cancelled' | 'error';

/** What the Runs screen shows of one run, built only from its event stream. */
export type RunView = Readonly<{
  phase: RunPhase;
  /** Test instances the run planned, once it started. */
  planned: number;
  /** The planned test instances in plan order, once the plan arrives. */
  plan: readonly PlanRow[];
  /** When the run started, in ms since the epoch. */
  startedMs?: bigint;
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
export const idleRun: RunView = { phase: 'idle', planned: 0, plan: [], counts: zero, events: [] };

/** The view of a run that has just been asked for, before its first event. */
export function startingRun(): RunView {
  return { ...idleRun, phase: 'running' };
}

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
    kind.case === 'runStarted'
      ? { ...view, planned: kind.value.instances, startedMs: kind.value.startedMs || undefined }
      : view,
  runPlanned: (view, { kind }) =>
    kind.case === 'runPlanned' ? { ...view, plan: planRows(kind.value.instances) } : view,
  actionStarted: (view, { kind }) => {
    const owner = kind.case === 'actionStarted' ? kind.value.owner?.kind : undefined;
    if (owner?.case !== 'instance') return view;
    const plan = markRunning(view.plan, owner.value);
    return plan === view.plan ? view : { ...view, plan };
  },
  testFinished: (view, { kind }) => {
    const test = kind.case === 'testFinished' ? kind.value.test : undefined;
    const key = test && outcomeOf(test.status);
    if (!test || !key) return view;
    const counts = { ...view.counts, [key]: view.counts[key] + 1 };
    return { ...view, counts, plan: markFinished(view.plan, test) };
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
