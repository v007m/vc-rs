import type { RunEvent } from '#lib/gen/vibranium/engine/v1/events_pb.ts';
import { RunStatus, type Totals } from '#lib/gen/vibranium/results/v1/results_pb.ts';
import { doneCount, noCounts, type Counts } from '#lib/results/status.ts';
import { describe, type EventLine } from './events.ts';
import {
  indexPlan,
  markFinished,
  markRunning,
  noPlanIndex,
  outcomeOf,
  planRows,
  type PlanIndex,
  type PlanRow,
} from './plan.ts';

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
  /** Where each planned instance's row is, so an event finds it without a search. */
  planIndex: PlanIndex;
  /** When the run started, in ms since the epoch. */
  startedMs?: bigint;
  counts: Counts;
  /** The last `eventCap` event lines, oldest first. */
  events: readonly EventLine[];
  /** How many older lines were let go to keep `events` at `eventCap`. */
  dropped: number;
  /** The history id, once the report names it. */
  runId?: bigint;
  exit?: number;
  /** Where the run's HTML report was written, once the report names it; text only (D320). */
  htmlReport?: string;
  /** Why the stream broke off, when phase is `error`. */
  error?: string;
}>;

const zero: Counts = noCounts;

/** How many event lines a view keeps, so a long run's memory stays bounded. */
export const eventCap = 2000;

/** A run that has not started. */
export const idleRun: RunView = {
  phase: 'idle',
  planned: 0,
  plan: [],
  planIndex: noPlanIndex,
  counts: zero,
  events: [],
  dropped: 0,
};

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
    kind.case === 'runPlanned'
      ? {
          ...view,
          plan: planRows(kind.value.instances),
          planIndex: indexPlan(kind.value.instances),
        }
      : view,
  actionStarted: (view, { kind }) => {
    const owner = kind.case === 'actionStarted' ? kind.value.owner?.kind : undefined;
    if (owner?.case !== 'instance') return view;
    const plan = markRunning(view.plan, view.planIndex, owner.value);
    return plan === view.plan ? view : { ...view, plan };
  },
  testFinished: (view, { kind }) => {
    if (kind.case !== 'testFinished') return view;
    const test = kind.value.test;
    const key = test && outcomeOf(test.status);
    if (!key) return view;
    const counts = { ...view.counts, [key]: view.counts[key] + 1 };
    return { ...view, counts, plan: markFinished(view.plan, view.planIndex, kind.value) };
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
    kind.case === 'report'
      ? { ...view, runId: kind.value.run, exit: kind.value.exit, htmlReport: kind.value.htmlReport }
      : view,
};

/** The view after one more event of the run's stream. */
export function apply(view: RunView, event: RunEvent): RunView {
  const step = event.kind.case && steps[event.kind.case];
  const next = step ? step(view, event) : view;
  const line = describe(event);
  return line ? withLine(next, line) : next;
}

/** `view` with `line` last, letting go of the oldest lines past `eventCap`. */
function withLine(view: RunView, line: EventLine): RunView {
  const over = Math.max(0, view.events.length + 1 - eventCap);
  const events = [...view.events.slice(over), line];
  return { ...view, events, dropped: view.dropped + over };
}

/** The view of a run whose stream broke off with `message`. */
export function broken(view: RunView, message: string): RunView {
  return { ...view, phase: 'error', error: message };
}
