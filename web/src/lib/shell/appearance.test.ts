import { describe, expect, it } from 'vitest';
import { applyAppearance, defaultAppearance, parseAppearance, presetFor } from './appearance.ts';

describe('appearance', () => {
  it('keeps only known choices from a saved value', () => {
    expect(parseAppearance(undefined)).toEqual(defaultAppearance);
    expect(parseAppearance({ theme: 'sepia', accent: 'ember', density: 'compact' })).toEqual({
      ...defaultAppearance,
      accent: 'ember',
      density: 'compact',
    });
  });

  it("paints the space's own preset unless one accent is chosen", () => {
    expect(presetFor(defaultAppearance, 'grove')).toBe('grove');
    expect(presetFor({ ...defaultAppearance, accent: 'tide' }, 'grove')).toBe('tide');
  });

  it("puts Strata's hooks on the root, and takes them off for the defaults", () => {
    const root = document.createElement('html');
    applyAppearance(root, {
      theme: 'dark',
      accent: 'space',
      density: 'compact',
      motion: 'reduced',
    });
    expect({ ...root.dataset }).toEqual({ theme: 'dark', density: 'compact', motion: 'reduced' });
    applyAppearance(root, defaultAppearance);
    expect({ ...root.dataset }).toEqual({});
  });
});
