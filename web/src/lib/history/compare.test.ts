import { describe, expect, it } from 'vitest';
import { compareLine, compareTabs, timingMoves } from './compare.ts';
import type { RunComparison, TimingChange } from './seams.ts';

const t = (id: string, baseMs: number, headMs: number): TimingChange => ({ id, baseMs, headMs });

const TIMING: TimingChange[] = [
  t('a', 100, 200),
  t('tooSmallShare', 1000, 1100),
  t('tooFewMs', 100, 140),
  t('d', 300, 600),
  t('e', 500, 300),
  t('noBase', 0, 500),
  t('g', 200, 100),
];

const RESULT: RunComparison = {
  newlyFailing: [{ id: 'users.crud#a', was: 'passed', now: 'failed', why: 'status 500' }],
  fixed: [],
  timing: TIMING,
  shapes: [{ api: 'users#get', path: '$.email', change: 'removed', type: 'string' }],
};

describe('timingMoves', () => {
  it('counts a move only at 20 % and 50 ms or more, the biggest move first', () => {
    const { slower, faster } = timingMoves(TIMING);
    expect(slower.map((m) => m.id)).toEqual(['d', 'a']);
    expect(faster.map((m) => m.id)).toEqual(['e', 'g']);
  });

  it('keeps a move right at both limits', () => {
    const { slower } = timingMoves([t('edge', 250, 300)]);
    expect(slower.map((m) => m.id)).toEqual(['edge']);
  });
});

describe('compareTabs', () => {
  it('counts each tab and marks Newly failing bad when a test broke', () => {
    expect(compareTabs(RESULT)).toEqual([
      { id: 'failing', label: 'Newly failing', count: '1', bad: true },
      { id: 'fixed', label: 'Fixed', count: '0' },
      { id: 'slower', label: 'Slower', count: '2' },
      { id: 'faster', label: 'Faster', count: '2' },
      { id: 'shapes', label: 'Response shapes', count: '1' },
    ]);
  });

  it('does not mark Newly failing bad when nothing broke', () => {
    const [failing] = compareTabs({ ...RESULT, newlyFailing: [] });
    expect(failing?.bad).toBe(false);
  });
});

describe('compareLine', () => {
  it('sums the comparison up, naming shape changes only when there are some', () => {
    expect(compareLine(RESULT)).toBe('1 newly failing · 0 fixed · 2 slower · 1 shape changes');
    expect(compareLine({ ...RESULT, shapes: [] })).toBe('1 newly failing · 0 fixed · 2 slower');
  });
});
