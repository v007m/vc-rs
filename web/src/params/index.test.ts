import { describe, expect, it } from 'vitest';
import { integer } from './index.ts';

describe('the integer route param', () => {
  it('reads a run number and leaves the named Runs screens to their own routes', () => {
    expect(integer('12')).toBe(12);
    expect(['0', '012', 'live', 'overview', '1.5', ''].map(integer)).toEqual(
      Array.from({ length: 6 }, () => undefined),
    );
  });
});
