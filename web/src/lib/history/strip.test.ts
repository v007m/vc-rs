import { describe, expect, it } from 'vitest';
import type { HistoryPoint } from './seams.ts';
import { sparkline, summarize } from './strip.ts';

const point = (run: number, status: HistoryPoint['status'], ms: number): HistoryPoint => ({
  run,
  status,
  ms,
  startedMs: run * 1000,
});

describe('the per-test history strip', () => {
  it('counts flaky results as passed and as flakes, and leaves skipped ones out', () => {
    const points = [
      point(4, 'passed', 40),
      point(3, 'flaky', 90),
      point(2, 'failed', 30),
      point(1, 'skipped', 0),
    ];
    expect(summarize(points)).toEqual({ results: 3, flakeRate: 33, passRate: 67, medianMs: 40 });
    expect(summarize([])).toEqual({ results: 0, flakeRate: 0, passRate: 0, medianMs: 0 });
  });

  it('draws the oldest result on the left and the slowest at the top', () => {
    const d = sparkline([point(2, 'passed', 100), point(1, 'passed', 50)], [10, 12]);
    expect(d).toBe('M0.0,6.0 L10.0,1.0');
  });
});
