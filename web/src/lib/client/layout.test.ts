import { describe, expect, it } from 'vitest';
import type { PrefStore } from '#lib/shell/prefs.ts';
import { ClientLayout, clampSplit, layoutKey } from './layout.svelte.ts';

function memory(initial?: string): PrefStore & { saved: Map<string, string> } {
  const saved = new Map<string, string>(initial === undefined ? [] : [[layoutKey, initial]]);
  return {
    saved,
    getItem: (key) => saved.get(key) ?? null,
    setItem: (key, value) => void saved.set(key, value),
  };
}

describe('client layout', () => {
  it('keeps the split between 20 and 80 percent', () => {
    expect([clampSplit(5), clampSplit(55.4), clampSplit(99)]).toEqual([20, 55, 80]);
  });

  it('starts automatic and even, and keeps what was chosen', () => {
    const store = memory();
    const layout = new ClientLayout(store);
    expect([layout.orient, layout.split]).toEqual(['auto', 50]);
    layout.setOrient('stack');
    layout.setSplit(90);
    expect(store.saved.get(layoutKey)).toBe('{"orient":"stack","split":80}');
    const again = new ClientLayout(store);
    expect([again.orient, again.split]).toEqual(['stack', 80]);
  });

  it('ignores a saved layout it cannot read, and a store that refuses writes', () => {
    for (const bad of ['not json', 'null', '{"orient":"diagonal","split":"wide"}']) {
      const layout = new ClientLayout(memory(bad));
      expect([layout.orient, layout.split]).toEqual(['auto', 50]);
    }
    const refusing: PrefStore = {
      getItem: () => null,
      setItem: () => {
        throw new Error('quota');
      },
    };
    const layout = new ClientLayout(refusing);
    layout.setOrient('side');
    expect(layout.orient).toBe('side');
    expect(new ClientLayout(undefined).orient).toBe('auto');
  });
});
