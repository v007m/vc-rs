import { describe, expect, it } from 'vitest';
import { breaches, clock, linePath, niceCeil, peakIndex, scale, timeTicks, yTop } from './chart.ts';
import type { LoadTick } from './port.ts';

const host = { cpu: 10, memMb: 100, conns: 4, dropped: 0 };
const tick = (t: number, p99: number): LoadTick => ({
  t,
  users: 10,
  rps: 100,
  p50: p99 / 4,
  p95: p99 / 2,
  p99,
  errorRate: 0,
  host,
});

describe('load chart geometry', () => {
  it('rounds the axis up to 1, 2, 2.5 or 5 times a power of ten', () => {
    expect([niceCeil(0), niceCeil(0.8), niceCeil(130), niceCeil(240), niceCeil(4100)]).toEqual([
      1, 1, 200, 250, 5000,
    ]);
  });

  it('maps seconds and values into the plot inside its margins', () => {
    const at = scale([400, 200], 100, 50);
    expect(at.x(0)).toBe(46);
    expect(at.x(100)).toBe(400 - 72);
    expect(at.y(50)).toBe(16);
    expect(at.y(0)).toBe(200 - 26);
  });

  it('draws a path through the ticks', () => {
    const at = scale([400, 200], 10, 100);
    expect(linePath([tick(0, 100), tick(10, 0)], 'p99', at)).toBe('M46.0,16.0L328.0,174.0');
  });

  it('finds each run of ticks over the limit, open runs included', () => {
    const ticks = [100, 900, 950, 700, 820, 860].map((v, i) => tick(i * 5, v));
    expect(breaches(ticks, 'p99', 800)).toEqual([
      [1, 2],
      [4, 5],
    ]);
    expect(peakIndex(ticks, 'p99', [1, 2])).toBe(2);
  });

  it('leaves room above the threshold', () => {
    const threshold = { metric: 'p99', limit: 800, label: 'p99 lt 800ms', abortOn: false } as const;
    expect(yTop([tick(0, 300)], ['p99'], threshold)).toBe(1000);
    expect(yTop([tick(0, 1200)], ['p99'])).toBe(2000);
  });

  it('labels time as m:ss, every minute or every two on a narrow plot', () => {
    expect(clock(125)).toBe('2:05');
    expect(timeTicks(180, 800)).toEqual([0, 60, 120, 180]);
    expect(timeTicks(180, 400)).toEqual([0, 120]);
  });
});
