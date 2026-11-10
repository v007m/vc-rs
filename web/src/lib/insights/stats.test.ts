import { describe, expect, it } from 'vitest';
import { duration, flakiest, median, niceMax, passRate } from './stats.ts';

const run = { id: 1, startedMs: 0, passed: 7, failed: 1, blocked: 1, flaky: 1 };

describe('stats', () => {
  it('counts a flaky pass as passed and leaves skipped tests out', () => {
    expect(passRate(run)).toBe(80);
    expect(passRate({ ...run, passed: 0, failed: 0, blocked: 0, flaky: 0 })).toBeUndefined();
  });

  it('takes the median of odd and even counts', () => {
    expect(median([3, 1, 2])).toBe(2);
    expect(median([4, 1, 3, 2])).toBe(2.5);
    expect(median([])).toBeUndefined();
  });

  it('ranks tests by flaky and failed results, dropping clean ones', () => {
    const t = (testId: string, failed: number, flaky: number) => ({
      testId,
      results: 10,
      failed,
      flaky,
      meanMs: 5,
    });
    const ranked = flakiest([
      t('a#ok', 0, 0),
      t('b#one', 0, 1),
      t('c#three', 1, 2),
      t('a#one', 1, 0),
    ]);
    expect(ranked.map((x) => x.testId)).toEqual(['c#three', 'a#one', 'b#one']);
  });

  it('rounds axis tops and formats durations', () => {
    expect([niceMax(0), niceMax(7), niceMax(13), niceMax(4200)]).toEqual([1, 10, 20, 5000]);
    expect([duration(850), duration(12_400), duration(185_000)]).toEqual([
      '850 ms',
      '12.4 s',
      '3 min 5 s',
    ]);
  });
});
