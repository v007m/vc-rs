import type { ApiTrend, RunSummary, SkipLine, TestTrend } from './port.ts';

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

/** The apis with a p95, slowest p95 first, at most `limit`. */
export function slowest(trends: readonly ApiTrend[], limit = 8): ApiTrend[] {
  return trends
    .filter((t) => t.p95Ms !== undefined)
    .sort((a, b) => (b.p95Ms ?? 0) - (a.p95Ms ?? 0) || a.apiId.localeCompare(b.apiId))
    .slice(0, limit);
}

/** A response time: `410 ms`, `1.24 s`. */
export function latency(ms: number): string {
  return ms < 1000 ? `${Math.round(ms)} ms` : `${(ms / 1000).toFixed(2)} s`;
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

/** A UTC day, which has no daylight-saving change. */
const DAY_MS = 86_400_000;

/** The local calendar day at `ms`, `YYYY-MM-DD`, as `skip.until` is written. */
export function localDay(ms: number): string {
  const date = new Date(ms);
  const two = (n: number) => String(n).padStart(2, '0');
  return `${date.getFullYear()}-${two(date.getMonth() + 1)}-${two(date.getDate())}`;
}

/** The skips that end within `days` days of `today`, or have ended, soonest first. */
export function expiring(skips: readonly SkipLine[], today: string, days = 14): SkipLine[] {
  const lastDay = new Date(Date.parse(`${today}T00:00:00Z`) + days * DAY_MS)
    .toISOString()
    .slice(0, 10);
  return skips
    .filter((skip) => skip.ended || skip.until <= lastDay)
    .sort((a, b) => a.until.localeCompare(b.until) || a.id.localeCompare(b.id));
}
