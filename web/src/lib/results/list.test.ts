import { describe, expect, it } from 'vitest';
import { listMove, scrollFor, windowOf } from './list.ts';

describe('windowOf', () => {
  it('renders the rows in view plus an overscan, clamped to the list', () => {
    expect(windowOf(0, 320, 10_000, 32)).toEqual({ first: 0, last: 30 });
    expect(windowOf(32_000, 320, 10_000, 32)).toEqual({ first: 990, last: 1020 });
    expect(windowOf(319_800, 320, 10_000, 32)).toEqual({ first: 9983, last: 10_000 });
    expect(windowOf(0, 320, 0, 32)).toEqual({ first: 0, last: 0 });
  });
});

describe('listMove', () => {
  it('moves by row, page and end, never past either end', () => {
    expect(listMove('ArrowDown', 3, 10, 5)).toBe(4);
    expect(listMove('ArrowUp', 0, 10, 5)).toBe(0);
    expect(listMove('PageDown', 8, 10, 5)).toBe(9);
    expect(listMove('PageUp', 3, 10, 5)).toBe(0);
    expect(listMove('Home', 7, 10, 5)).toBe(0);
    expect(listMove('End', 2, 10, 5)).toBe(9);
  });

  it('ignores other keys and empty lists', () => {
    expect(listMove('a', 3, 10, 5)).toBeUndefined();
    expect(listMove('ArrowDown', 0, 0, 5)).toBeUndefined();
  });
});

describe('scrollFor', () => {
  it('scrolls only as far as needed to show the row whole', () => {
    expect(scrollFor(5, 0, 320, 32)).toBe(0);
    expect(scrollFor(10, 0, 320, 32)).toBe(32);
    expect(scrollFor(2, 200, 320, 32)).toBe(64);
  });
});
