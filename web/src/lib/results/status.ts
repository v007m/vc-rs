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

/** The `st-dot` and `st-badge` modifier class of each outcome (strata/vibranium.css). */
export const statusClass: Readonly<Record<Status, string>> = {
  passed: 'vc-pass',
  failed: 'vc-fail',
  blocked: 'vc-blocked',
  flaky: 'vc-flaky',
  skipped: 'vc-skipped',
};

/** A mark for each outcome, shown beside its colour so a count never rests on colour alone (D640). */
export const statusGlyph: Readonly<Record<Status, string>> = {
  passed: '✓',
  failed: '✕',
  blocked: '⊘',
  flaky: '≈',
  skipped: '–',
};

/** A test's time with its unit, `212 ms`; a dash for a skipped test or an unknown time. */
export function testTime(status: Status, value: number | null | undefined): string {
  return status === 'skipped' || value === null || value === undefined ? '—' : `${ms(value)} ms`;
}

/** `12.0`, `9.5`, `1234`: a duration in ms, finer below 10 ms; empty when unknown. */
export function ms(value: number | null | undefined): string {
  if (value === null || value === undefined) return '';
  return value < 10 ? value.toFixed(1) : Math.round(value).toString();
}
