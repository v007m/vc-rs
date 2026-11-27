import { notYet, type Maybe } from '#lib/api/maybe.ts';
import type { Status } from '#lib/results/status.ts';

/** A test whose status changed between the base run and the head run. */
export type StatusChange = Readonly<{ id: string; was: Status; now: Status; why?: string }>;

/** A test's duration in the base run and the head run, in ms. */
export type TimingChange = Readonly<{ id: string; baseMs: number; headMs: number }>;

/** A response field whose presence or type differs between the two runs' stored bodies. */
export type ShapeChange = Readonly<{
  api: string;
  path: string;
  change: 'added' | 'removed' | 'type';
  type: string;
}>;

/** What `vc history compare <base> <head>` finds (ui.md §6). */
export type RunComparison = Readonly<{
  newlyFailing: readonly StatusChange[];
  fixed: readonly StatusChange[];
  timing: readonly TimingChange[];
  shapes: readonly ShapeChange[];
}>;

export interface ComparePort {
  compare(base: number, head: number): Promise<Maybe<RunComparison>>;
}

/** One stored result of a test, newest first in a history. */
export type HistoryPoint = Readonly<{ run: number; status: Status; ms: number; startedMs: number }>;

export interface TestHistoryPort {
  /** The test's newest `limit` results across the history. */
  history(id: string, limit: number): Promise<Maybe<readonly HistoryPoint[]>>;
}

/** The Runs space's seams over RPCs the engine has not got yet (D538, ui.md §10 row 9). */
export type RunsSeams = Readonly<{ compare: ComparePort; tests: TestHistoryPort }>;

export const compareNotYet = 'Comparing runs needs a newer engine.';
export const historyNotYet = 'The test’s history needs a newer engine.';

/** Today's engine: neither compare nor a test's history; each says why. */
export const noRunsSeams: RunsSeams = {
  compare: { compare: () => Promise.resolve(notYet(compareNotYet)) },
  tests: { history: () => Promise.resolve(notYet(historyNotYet)) },
};

/** The tests slower (`dir` 1) or faster (`dir` -1) by `percent` or more and `floorMs` or more. */
export function moved(
  timing: readonly TimingChange[],
  dir: 1 | -1,
  limits: Readonly<{ percent: number; floorMs: number }>,
): TimingChange[] {
  return timing.filter(({ baseMs, headMs }) => {
    const delta = (headMs - baseMs) * dir;
    return baseMs > 0 && delta >= limits.floorMs && (delta / baseMs) * 100 >= limits.percent;
  });
}

/** `head` against `base` as a signed whole percent. */
export function percentChange({ baseMs, headMs }: TimingChange): number {
  return baseMs > 0 ? Math.round(((headMs - baseMs) / baseMs) * 100) : 0;
}
