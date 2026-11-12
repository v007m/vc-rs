import { describe, expect, it } from 'vitest';
import { isCurrent, liveCount, navItems } from './nav.ts';

describe('liveCount', () => {
  it('counts the runs going on beside Runs only, and says nothing while none is', () => {
    const counted = navItems.filter((item) => liveCount(item, 1) !== undefined);
    expect(counted.map((item) => item.label)).toEqual(['Runs']);
    const [runs] = counted;
    if (!runs) throw new Error('no Runs item');
    expect(liveCount(runs, 1)).toBe('1 live');
    expect(liveCount(runs, 0)).toBeUndefined();
  });
});

describe('isCurrent', () => {
  it('marks a screen and the pages under it, not one whose name only starts the same', () => {
    expect(isCurrent('/runs', '/runs')).toBe(true);
    expect(isCurrent('/runs/7', '/runs')).toBe(true);
    expect(isCurrent('/runsx', '/runs')).toBe(false);
  });
});
