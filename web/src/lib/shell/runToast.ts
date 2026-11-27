import { doneCount } from '#lib/results/status.ts';
import type { RunView } from '#lib/run/model.ts';
import type { ToastTone } from '#lib/ui/toasts.svelte.ts';

/** The toast a run that just ended gives: how it went, and the history number to open. */
export type RunEnd = Readonly<{ text: string; tone: ToastTone; id?: number }>;

const tests = (n: number): string => `${n} ${n === 1 ? 'test' : 'tests'}`;

function words(view: RunView, name: string): string {
  const done = doneCount(view.counts);
  const { failed, blocked } = view.counts;
  switch (view.phase) {
    case 'passed':
      return `${name} passed: ${tests(done)}.`;
    case 'failed':
      return `${name} failed: ${failed} failed, ${blocked} blocked of ${tests(done)}.`;
    case 'cancelled':
      return `${name} was stopped after ${tests(done)}.`;
    default:
      return `${name} broke off: ${view.error ?? 'the engine stopped answering'}.`;
  }
}

/** What a run says as it ends, on whichever screen is open (D604). */
export function runEnded(view: RunView): RunEnd {
  const id = view.runId === undefined ? undefined : Number(view.runId);
  const text = words(view, id === undefined ? 'The run' : `Run #${id}`);
  const tone: ToastTone = view.phase === 'error' ? 'error' : 'done';
  return id === undefined ? { text, tone } : { text, tone, id };
}
