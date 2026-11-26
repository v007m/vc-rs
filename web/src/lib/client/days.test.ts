import { describe, expect, it } from 'vitest';
import { byDay, clock } from './days.ts';
import type { HistoryEntry } from './seams.ts';

function entry(id: number, at: Date): HistoryEntry {
  return { id, label: `/r/${id}`, outcome: 'passed', startedMs: at.getTime() };
}

describe('history days', () => {
  it('groups history by local day, newest day first, keeping each day in order', () => {
    const now = new Date(2026, 10, 26, 15, 0).getTime();
    const days = byDay(
      [
        entry(1, new Date(2026, 10, 26, 14, 6)),
        entry(2, new Date(2026, 10, 24, 9, 0)),
        entry(3, new Date(2026, 10, 25, 23, 59)),
        entry(4, new Date(2026, 10, 26, 0, 1)),
      ],
      now,
    );
    expect(days.map((d) => [d.label, d.entries.map((e) => e.id)])).toEqual([
      ['Today', [1, 4]],
      ['Yesterday', [3]],
      ['Tue 24 Nov', [2]],
    ]);
    expect(byDay([], now)).toEqual([]);
  });

  it('writes a time of day in 24 hours', () => {
    expect(clock(new Date(2026, 10, 26, 14, 6).getTime())).toBe('14:06');
  });
});
