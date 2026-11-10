import { describe, expect, it } from 'vitest';
import { phases, reused } from './waterfall.ts';

describe('phases', () => {
  it('lays a new connection out end to end against the total', () => {
    const laid = phases({
      dnsMs: 10,
      connectMs: 20,
      tlsMs: 30,
      firstByteMs: 100,
      downloadMs: 40,
      totalMs: 200,
    });
    expect(laid.map((p) => p.label)).toEqual(['dns', 'connect', 'tls', 'firstByte', 'download']);
    expect(laid.map((p) => [p.start, p.width])).toEqual([
      [0, 5],
      [5, 10],
      [15, 15],
      [30, 50],
      [80, 20],
    ]);
  });

  it('leaves out the phases a reused connection skips and a phase that took no time', () => {
    const laid = phases({ firstByteMs: 30, downloadMs: 0, totalMs: 40 });
    expect(laid).toEqual([{ label: 'firstByte', ms: 30, start: 0, width: 75 }]);
  });

  it('scales to the phases when they add up to more than the total', () => {
    const laid = phases({ dnsMs: 50, firstByteMs: 50, downloadMs: 0, totalMs: 80 });
    expect(laid.map((p) => p.width)).toEqual([50, 50]);
  });

  it('tells a reused connection from a new one', () => {
    expect(reused({ firstByteMs: 30, downloadMs: 1, totalMs: 31 })).toBe(true);
    expect(reused({ connectMs: 2, firstByteMs: 30, downloadMs: 1, totalMs: 33 })).toBe(false);
  });

  it('draws nothing when nothing took time', () => {
    expect(phases({ firstByteMs: 0, downloadMs: 0, totalMs: 0 })).toEqual([]);
  });
});
