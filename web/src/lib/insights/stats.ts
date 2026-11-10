import type { RunSummary, TestTrend } from './port.ts';

/** Tests that ran in `run`: every result but the skipped ones. */
export function ran(run: RunSummary): number {
  return run.passed + run.flaky + run.failed + run.blocked;
}

/** The share of `run`'s tests that passed, a flaky pass included, in whole percent; `undefined` when none ran. */
export function passRate(run: RunSummary): number | undefined {
  const total = ran(run);
  return total === 0 ? undefined : Math.round(((run.passed + run.flaky) * 100) / total);
}

export function median(values: readonly number[]): number | undefined {
  if (values.length === 0) return undefined;
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  const upper = sorted[mid] ?? 0;
  return sorted.length % 2 === 1 ? upper : ((sorted[mid - 1] ?? 0) + upper) / 2;
}

export function mean(values: readonly number[]): number | undefined {
  return values.length === 0 ? undefined : values.reduce((a, b) => a + b, 0) / values.length;
}

/** The tests that were flaky or failed in the window, worst first, at most `limit`. */
export function flakiest(trends: readonly TestTrend[], limit = 10): TestTrend[] {
  return trends
    .filter((t) => t.flaky + t.failed > 0)
    .sort((a, b) => b.flaky + b.failed - (a.flaky + a.failed) || a.testId.localeCompare(b.testId))
    .slice(0, limit);
}

/** A round axis top at or above `max`: 1, 2 or 5 times a power of ten. */
export function niceMax(max: number): number {
  if (max <= 0) return 1;
  const power = 10 ** Math.floor(Math.log10(max));
  return [1, 2, 5, 10].map((m) => m * power).find((top) => top >= max) ?? max;
}

/** `850 ms`, `12.4 s`, `3 min 5 s`. */
export function duration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  const s = Math.round(ms / 1000);
  return `${Math.floor(s / 60)} min ${s % 60} s`;
}

/** `Nov 10, 14:05`, in the viewer's time zone. */
export function when(ms: number): string {
  return new Date(ms).toLocaleString('en', {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
  });
}
