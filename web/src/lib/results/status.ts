/** A test's outcome, as reports and run views name it. */
export type Status = 'passed' | 'failed' | 'blocked' | 'flaky' | 'skipped';

/** Every outcome, in the order views list them. */
export const statuses: readonly Status[] = ['passed', 'failed', 'blocked', 'flaky', 'skipped'];

/** Test counts by outcome. */
export type Counts = Readonly<Record<Status, number>>;

export const noCounts: Counts = { passed: 0, failed: 0, blocked: 0, flaky: 0, skipped: 0 };

/** How many tests have an outcome. */
export function doneCount(counts: Counts): number {
  return counts.passed + counts.failed + counts.blocked + counts.flaky + counts.skipped;
}

/** The `dot` and `pill` modifier class of each outcome (controls.css). */
export const statusClass: Readonly<Record<Status, string>> = {
  passed: 'pass',
  failed: 'fail',
  blocked: 'block',
  flaky: 'flaky',
  skipped: 'skip',
};

/** `12.0`, `9.5`, `1234`: a duration in ms, finer below 10 ms; empty when unknown. */
export function ms(value: number | null | undefined): string {
  if (value === null || value === undefined) return '';
  return value < 10 ? value.toFixed(1) : Math.round(value).toString();
}
