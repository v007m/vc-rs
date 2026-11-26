/**
 * Keyboard hints and region cycling (docs/design/ui.md §2, D600). A hint is plain text, never a
 * keycap: `⌘K` on Apple platforms, `Ctrl+K` elsewhere. Holding the modifier alone for
 * `holdDelay` ms shows the space shortcuts beside the switcher; F6 and Shift+F6 move between
 * the regions marked `data-region`.
 */

/** How long the modifier is held, alone, before the hints show. */
export const holdDelay = 450;

type Platform = Pick<Navigator, 'platform' | 'userAgent'> | undefined;

/** True on macOS and iOS, where the command modifier is ⌘. */
export function isApple(nav: Platform = globalThis.navigator): boolean {
  return /Mac|iPhone|iPad|iPod/i.test(nav?.platform || nav?.userAgent || '');
}

/** `⌘K` on Apple, `Ctrl+K` elsewhere. */
export function modHint(key: string, apple = isApple()): string {
  return apple ? `⌘${key}` : `Ctrl+${key}`;
}

/** The same chord as an `aria-keyshortcuts` value. */
export function modAria(key: string, apple = isApple()): string {
  return `${apple ? 'Meta' : 'Control'}+${key === ',' ? 'Comma' : key}`;
}

/**
 * Calls `onchange(true)` once the modifier has been held alone for `holdDelay` ms, and
 * `onchange(false)` as soon as it is released, another key is pressed or the window loses focus.
 * Returns the function that stops watching.
 */
export function watchModifier(
  target: EventTarget,
  onchange: (held: boolean) => void,
  apple = isApple(),
): () => void {
  const modifier = apple ? 'Meta' : 'Control';
  let timer: ReturnType<typeof setTimeout> | undefined;
  let held = false;
  const reset = (): void => {
    clearTimeout(timer);
    timer = undefined;
    if (held) onchange((held = false));
  };
  const down = (event: Event): void => {
    if ((event as KeyboardEvent).key !== modifier) return reset();
    if (timer || held) return;
    timer = setTimeout(() => {
      timer = undefined;
      onchange((held = true));
    }, holdDelay);
  };
  const events: [string, (event: Event) => void][] = [
    ['keydown', down],
    ['keyup', reset],
    ['blur', reset],
  ];
  for (const [name, fn] of events) target.addEventListener(name, fn);
  return () => {
    clearTimeout(timer);
    for (const [name, fn] of events) target.removeEventListener(name, fn);
  };
}

const focusable =
  'a[href], button:not([disabled]), input:not([disabled]), select, textarea, [tabindex]:not([tabindex="-1"])';

function shown(element: HTMLElement): boolean {
  return element.checkVisibility?.({ visibilityProperty: true }) ?? true;
}

/** Where focus lands in a region: the region itself when it takes focus, else its first control. */
function entry(region: HTMLElement): HTMLElement | undefined {
  if (region.hasAttribute('tabindex')) return region;
  return [...region.querySelectorAll<HTMLElement>(focusable)].find(shown);
}

/** Moves focus to the next region after the one holding `active` (the previous with `back`). */
export function cycleRegion(root: ParentNode, active: Element | null, back = false): boolean {
  const regions = [...root.querySelectorAll<HTMLElement>('[data-region]')].filter(
    (region) => shown(region) && entry(region),
  );
  if (regions.length === 0) return false;
  const at = regions.findIndex((region) => active && region.contains(active));
  const step = back ? -1 : 1;
  const next =
    at < 0 ? (back ? regions.length - 1 : 0) : (at + step + regions.length) % regions.length;
  entry(regions[next] as HTMLElement)?.focus();
  return true;
}

/** ⌘K (Ctrl+K off Apple), the chord that opens the command palette from anywhere (D620). */
export function isPaletteKey(
  event: Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey'>,
): boolean {
  const mod = event.metaKey || event.ctrlKey;
  return mod && !event.altKey && !event.shiftKey && event.key.toLowerCase() === 'k';
}
