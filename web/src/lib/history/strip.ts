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

/** The duration sparkline's path over `[w, h]`, oldest result on the left. */
export function sparkline(
  points: readonly HistoryPoint[],
  [w, h]: readonly [number, number],
): string {
  const ms = [...points].reverse().map((p) => p.ms);
  const top = Math.max(1, ...ms);
  const step = ms.length > 1 ? w / (ms.length - 1) : 0;
  return ms
    .map(
      (v, i) =>
        `${i ? 'L' : 'M'}${(i * step).toFixed(1)},${(h - (v / top) * (h - 2) - 1).toFixed(1)}`,
    )
    .join(' ');
}
