import { describe, expect, it } from 'vitest';
import { defaultAppearance } from './appearance.ts';
import {
  defaultPrefs,
  loadPrefs,
  parsePrefs,
  prefsKey,
  savePrefs,
  type PrefStore,
} from './prefs.ts';

/** A store that keeps items in a map. */
function memory(): PrefStore & { items: Map<string, string> } {
  const items = new Map<string, string>();
  return {
    items,
    getItem: (key) => items.get(key) ?? null,
    setItem: (key, value) => void items.set(key, value),
  };
}

const throwing: PrefStore = {
  getItem: () => {
    throw new DOMException('blocked', 'SecurityError');
  },
  setItem: () => {
    throw new DOMException('full', 'QuotaExceededError');
  },
};

describe('prefs', () => {
  it('round-trips through a store', () => {
    const store = memory();
    const prefs = {
      ...defaultPrefs,
      lastRoute: { runs: '/runs?run=7' },
      sbClosed: ['tests:views'],
    };
    savePrefs(prefs, store);
    expect(store.items.has(prefsKey)).toBe(true);
    expect(loadPrefs(store)).toEqual(prefs);
  });

  it('falls back to the defaults when storage throws, and saving does not throw', () => {
    expect(loadPrefs(throwing)).toEqual(defaultPrefs);
    expect(() => savePrefs(defaultPrefs, throwing)).not.toThrow();
  });

  it('falls back to the defaults with no storage at all', () => {
    expect(loadPrefs(undefined)).toEqual(defaultPrefs);
    expect(() => savePrefs(defaultPrefs, undefined)).not.toThrow();
  });

  it('falls back on malformed JSON or a non-object', () => {
    expect(parsePrefs('{not json')).toEqual(defaultPrefs);
    expect(parsePrefs('[1,2]')).toEqual(defaultPrefs);
    expect(parsePrefs('null')).toEqual(defaultPrefs);
  });

  it('keeps only the fields with the right shape', () => {
    const raw = JSON.stringify({
      lastRoute: { tests: '/tests', runs: 7 },
      sbClosed: ['a:b', 3],
      sbCollapsed: 'yes',
      switcher: 'side',
      appearance: { theme: 'dark', accent: 'neon', density: 'compact', motion: 1 },
    });
    expect(parsePrefs(raw)).toEqual({
      lastRoute: { tests: '/tests' },
      sbClosed: ['a:b'],
      sbCollapsed: false,
      switcher: 'rail',
      appearance: { theme: 'dark', accent: 'space', density: 'compact', motion: 'system' },
    });
  });
  it('starts with the default appearance, and keeps a saved one', () => {
    expect(parsePrefs('{}').appearance).toEqual(defaultAppearance);
    const store = memory();
    const appearance = { theme: 'light', accent: 'tide', density: 'compact', motion: 'reduced' };
    savePrefs({ ...defaultPrefs, appearance } as typeof defaultPrefs, store);
    expect(loadPrefs(store).appearance).toEqual(appearance);
  });
});
