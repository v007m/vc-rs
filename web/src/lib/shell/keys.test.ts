import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cycleRegion, holdDelay, isApple, modAria, modHint, watchModifier } from './keys.ts';

describe('modifier hints', () => {
  it('reads Apple platforms from the platform string, then the user agent', () => {
    expect(isApple({ platform: 'MacIntel', userAgent: '' })).toBe(true);
    expect(isApple({ platform: '', userAgent: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0)' })).toBe(
      true,
    );
    expect(isApple({ platform: 'Win32', userAgent: 'Mozilla/5.0 (Windows NT 10.0)' })).toBe(false);
    expect(isApple(undefined)).toBe(false);
  });

  it('writes ⌘K on Apple and Ctrl+K elsewhere, in text and for aria-keyshortcuts', () => {
    expect([modHint('K', true), modHint('K', false)]).toEqual(['⌘K', 'Ctrl+K']);
    expect([modAria('1', true), modAria(',', false)]).toEqual(['Meta+1', 'Control+Comma']);
  });
});

describe('watchModifier', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  function press(target: EventTarget, type: string, key: string): void {
    target.dispatchEvent(new KeyboardEvent(type, { key }));
  }

  it('shows the hints after the modifier is held alone, and hides them on release', () => {
    const target = new EventTarget();
    const seen: boolean[] = [];
    const stop = watchModifier(target, (held) => seen.push(held), true);
    press(target, 'keydown', 'Meta');
    vi.advanceTimersByTime(holdDelay - 1);
    expect(seen).toEqual([]);
    press(target, 'keydown', 'Meta'); // key repeat does not restart the wait
    vi.advanceTimersByTime(1);
    expect(seen).toEqual([true]);
    press(target, 'keyup', 'Meta');
    expect(seen).toEqual([true, false]);
    stop();
  });

  it('never shows them when another key joins the modifier (a shortcut, not a hold)', () => {
    const target = new EventTarget();
    const seen: boolean[] = [];
    watchModifier(target, (held) => seen.push(held), false);
    press(target, 'keydown', 'Control');
    press(target, 'keydown', '1');
    vi.advanceTimersByTime(holdDelay * 2);
    expect(seen).toEqual([]);
  });

  it('stops watching when told to', () => {
    const target = new EventTarget();
    const seen: boolean[] = [];
    watchModifier(target, (held) => seen.push(held), true)();
    press(target, 'keydown', 'Meta');
    vi.advanceTimersByTime(holdDelay);
    expect(seen).toEqual([]);
  });
});

describe('cycleRegion', () => {
  function regions(): HTMLElement {
    const root = document.createElement('div');
    root.innerHTML = `
      <nav data-region><a href="/a">A</a></nav>
      <aside data-region><button>B</button></aside>
      <footer data-region><span>no control</span></footer>
      <main data-region tabindex="-1"><p>C</p></main>`;
    document.body.append(root);
    return root;
  }

  it('moves to the next region with a control, and wraps', () => {
    const root = regions();
    const [link, button, main] = [
      root.querySelector('a'),
      root.querySelector('button'),
      root.querySelector('main'),
    ];
    link?.focus();
    expect(cycleRegion(root, document.activeElement)).toBe(true);
    expect(document.activeElement).toBe(button);
    cycleRegion(root, document.activeElement);
    expect(document.activeElement).toBe(main);
    cycleRegion(root, document.activeElement);
    expect(document.activeElement).toBe(link);
    root.remove();
  });

  it('goes back with Shift, and starts at an end when focus is outside every region', () => {
    const root = regions();
    cycleRegion(root, document.body, true);
    expect(document.activeElement).toBe(root.querySelector('main'));
    cycleRegion(root, document.activeElement, true);
    expect(document.activeElement).toBe(root.querySelector('button'));
    root.remove();
  });
});
