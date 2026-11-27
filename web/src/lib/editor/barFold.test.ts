import { describe, expect, it } from 'vitest';
import { barFold, FOLD_MORE, FOLD_SOME } from './barFold.ts';

describe('barFold', () => {
  it('shows every action on a wide or unmeasured bar', () => {
    expect(barFold(0)).toBe(0);
    expect(barFold(FOLD_SOME)).toBe(0);
    expect(barFold(1200)).toBe(0);
  });

  it('folds Revert and Check first, then the rest below the second width', () => {
    expect(barFold(FOLD_SOME - 1)).toBe(1);
    expect(barFold(FOLD_MORE)).toBe(1);
    expect(barFold(FOLD_MORE - 1)).toBe(2);
    expect(barFold(320)).toBe(2);
  });
});
