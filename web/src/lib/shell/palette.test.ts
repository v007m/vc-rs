import { describe, expect, it } from 'vitest';
import { filterPalette, paletteItems, stepPalette } from './palette.ts';
import { spaces } from './spaces.ts';

const items = paletteItems(spaces, (route) => `/app${route}`);

describe('palette', () => {
  it('lists every screen under its space, with the shortcut on the space home', () => {
    const groups = filterPalette(items, '');
    expect(groups.map((g) => g.group)).toEqual(['Client', 'Tests', 'Runs', 'Agents', 'Settings']);
    const tests = groups[1]?.items ?? [];
    expect(tests.map((i) => [i.label, i.href, i.shortcut])).toEqual([
      ['Tests', '/app/tests', '2'],
      ['Editor', '/app/tests/editor', undefined],
    ]);
  });

  it('keeps items where every word starts a word of the label or space', () => {
    expect(filterPalette(items, 'run ov').flatMap((g) => g.items.map((i) => i.label))).toEqual([
      'Runs overview',
    ]);
    expect(filterPalette(items, 'EDIT').flatMap((g) => g.items.map((i) => i.label))).toEqual([
      'Editor',
    ]);
    expect(filterPalette(items, 'nothing like this')).toEqual([]);
  });

  it('steps through the flattened results, wrapping at both ends', () => {
    const groups = filterPalette(items, 'runs');
    const ids = groups.flatMap((g) => g.items.map((i) => i.id));
    expect(stepPalette(groups, undefined, 1)).toBe(ids[0]);
    expect(stepPalette(groups, undefined, -1)).toBe(ids.at(-1));
    expect(stepPalette(groups, ids.at(-1), 1)).toBe(ids[0]);
    expect(stepPalette([], undefined, 1)).toBeUndefined();
  });
});
