import { describe, expect, it } from 'vitest';
import { prefsKey, type PrefStore } from './prefs.ts';
import { ShellState, shortcutSpace, type ShortcutKey } from './state.svelte.ts';

function memory(): PrefStore & { items: Map<string, string> } {
  const items = new Map<string, string>();
  return {
    items,
    getItem: (key) => items.get(key) ?? null,
    setItem: (key, value) => void items.set(key, value),
  };
}

const press = (init: Partial<ShortcutKey>): ShortcutKey => ({
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  code: '',
  key: '',
  ...init,
});

describe('ShellState', () => {
  it('remembers each space’s last screen and reopens it from another space', () => {
    const shell = new ShellState(memory());
    shell.remember('/runs/overview', '?window=7');
    expect(shell.routeFor('runs', '/client')).toBe('/runs/overview?window=7');
  });

  it('goes to the home of the space it is already in', () => {
    const shell = new ShellState(memory());
    shell.remember('/runs/overview');
    expect(shell.routeFor('runs', '/runs/overview')).toBe('/runs');
  });

  it('goes home when nothing is remembered, and ignores unknown paths', () => {
    const shell = new ShellState(memory());
    shell.remember('/nowhere');
    expect(shell.prefs.lastRoute).toEqual({});
    expect(shell.routeFor('tests', '/client')).toBe('/tests');
  });

  it('keeps the last screen across visits', () => {
    const store = memory();
    new ShellState(store).remember('/tests/editor', '?file=a.test.yaml');
    expect(new ShellState(store).routeFor('tests', '/client')).toBe(
      '/tests/editor?file=a.test.yaml',
    );
  });

  it('drops a remembered route the shell no longer knows', () => {
    const store = memory();
    store.setItem(prefsKey, JSON.stringify({ lastRoute: { tests: '/old' } }));
    expect(new ShellState(store).routeFor('tests', '/client')).toBe('/tests');
  });

  it('closes and opens sidebar sections, saving them', () => {
    const store = memory();
    const shell = new ShellState(store);
    shell.toggleSection('tests', 'views');
    expect(new ShellState(store).isClosed('tests', 'views')).toBe(true);
    shell.toggleSection('tests', 'views');
    expect(new ShellState(store).isClosed('tests', 'views')).toBe(false);
  });

  it('hides the sidebar as a saved pref in the layout, and as an unsaved drawer outside it', () => {
    const store = memory();
    const shell = new ShellState(store);
    shell.toggleSidebar(false);
    expect(shell.drawer).toBe(true);
    expect(shell.prefs.sbCollapsed).toBe(false);
    shell.toggleSidebar(true);
    expect(shell.drawer).toBe(false);
    expect(new ShellState(store).prefs.sbCollapsed).toBe(true);
  });

  it('keeps working in memory when storage throws', () => {
    const shell = new ShellState({
      getItem: () => {
        throw new Error('blocked');
      },
      setItem: () => {
        throw new Error('full');
      },
    });
    shell.toggleSection('runs', 'views');
    shell.setSwitcher('foot');
    expect(shell.isClosed('runs', 'views')).toBe(true);
    expect(shell.prefs.switcher).toBe('foot');
  });
});

describe('shortcutSpace', () => {
  it('maps ⌘1–⌘5 to the spaces in rail order and ⌘, to Settings', () => {
    expect(shortcutSpace(press({ metaKey: true, code: 'Digit1', key: '1' }))).toBe('client');
    expect(shortcutSpace(press({ metaKey: true, code: 'Digit3', key: '3' }))).toBe('runs');
    expect(shortcutSpace(press({ metaKey: true, code: 'Digit5', key: '5' }))).toBe('settings');
    expect(shortcutSpace(press({ ctrlKey: true, code: 'Comma', key: ',' }))).toBe('settings');
  });

  it('ignores plain digits, Alt, Shift and other digits', () => {
    expect(shortcutSpace(press({ code: 'Digit1', key: '1' }))).toBeUndefined();
    expect(shortcutSpace(press({ metaKey: true, altKey: true, code: 'Digit1' }))).toBeUndefined();
    expect(shortcutSpace(press({ ctrlKey: true, shiftKey: true, code: 'Digit2' }))).toBeUndefined();
    expect(shortcutSpace(press({ metaKey: true, code: 'Digit6', key: '6' }))).toBeUndefined();
  });
});
