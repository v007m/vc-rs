import { describe, expect, it } from 'vitest';
import {
  duration,
  expiring,
  flakiest,
  latency,
  localDay,
  median,
  niceMax,
  passRate,
  slowest,
} from './stats.ts';

const run = { id: 1, startedMs: 0, passed: 7, failed: 1, blocked: 1, flaky: 1 };

describe('stats', () => {
  it('keeps the skips ending within the days given, across a month end, soonest first', () => {
    const skips = [
      { id: 'b', until: '2026-12-01', ended: false },
      { id: 'a', until: '2026-12-02', ended: false },
      { id: 'c', until: '2026-11-01', ended: true },
      { id: 'a2', until: '2026-12-01', ended: false },
    ];
    expect(expiring(skips, '2026-11-17').map(({ id }) => id)).toEqual(['c', 'a2', 'b']);
    expect(expiring(skips, '2026-11-17', 15).map(({ id }) => id)).toEqual(['c', 'a2', 'b', 'a']);
  });

  it('keeps an ended skip whatever its date, as the engine says it has ended', () => {
    const late = { id: 'late', until: '2027-01-01', ended: true };
    expect(expiring([late], '2026-11-17')).toEqual([late]);
  });

  it('writes a local day as skip.until is written', () => {
    expect(localDay(new Date(2026, 0, 5, 23, 59).getTime())).toBe('2026-01-05');
  });

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

  it('ranks apis by p95, ties by id, dropping those with no answer, at most the limit', () => {
    const api = (apiId: string, p95Ms?: number) => ({
      apiId,
      method: 'GET',
      calls: 1,
      ...(p95Ms === undefined ? {} : { p50Ms: 1, p95Ms }),
    });
    const trends = [api('b', 50), api('none'), api('a', 50), api('c', 900), api('d', 10)];
    expect(slowest(trends, 3).map(({ apiId }) => apiId)).toEqual(['c', 'a', 'b']);
  });

  it('formats a response time in ms below a second, else in seconds to two places', () => {
    expect([latency(999.4), latency(1000), latency(1236)]).toEqual(['999 ms', '1.00 s', '1.24 s']);
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
