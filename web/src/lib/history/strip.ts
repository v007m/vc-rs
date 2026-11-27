import type { HistoryPoint } from './seams.ts';

/** How many results the per-test history strip shows (D638). */
export const STRIP = 30;

/** A test's history at a glance: its flake rate, pass rate and median time over `points`. */
export type StripSummary = Readonly<{
  results: number;
  flakeRate: number;
  passRate: number;
  medianMs: number;
}>;

/** The strip's summary; the flake rate counts flaky results, the pass rate flaky ones as passed. */
export function summarize(points: readonly HistoryPoint[]): StripSummary {
  const ran = points.filter((p) => p.status !== 'skipped');
  const share = (n: number) => (ran.length ? Math.round((n / ran.length) * 100) : 0);
  const sorted = ran.map((p) => p.ms).sort((a, b) => a - b);
  return {
    results: ran.length,
    flakeRate: share(ran.filter((p) => p.status === 'flaky').length),
    passRate: share(ran.filter((p) => p.status === 'passed' || p.status === 'flaky').length),
    medianMs: sorted[Math.floor(sorted.length / 2)] ?? 0,
  };
}

/**
 * The duration sparkline's path over `[w, h]`, oldest result on the left; a run that did not run
 * the test breaks the line rather than dipping to zero.
 */
export function sparkline(
  points: readonly HistoryPoint[],
  [w, h]: readonly [number, number],
): string {
  const shown = [...points].reverse();
  const top = Math.max(1, ...shown.map((p) => p.ms));
  const step = shown.length > 1 ? w / (shown.length - 1) : 0;
  let pen = 'M';
  return shown
    .flatMap((p, i) => {
      if (p.status === 'skipped') {
        pen = 'M';
        return [];
      }
      const at = `${pen}${(i * step).toFixed(1)},${(h - (p.ms / top) * (h - 2) - 1).toFixed(1)}`;
      pen = 'L';
      return [at];
    })
    .join(' ');
}
