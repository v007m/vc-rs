import { resolve } from '$app/paths';
import {
  browserStore,
  defaultPrefs,
  loadPrefs,
  savePrefs,
  type PrefStore,
  type Switcher,
  type UiPrefs,
} from './prefs.ts';
import { isKnown, spaceByKey, spaceOf, type SpaceKey } from './spaces.ts';

/** Above this width of the app the sidebar is in the layout; at it and below, a drawer. */
export const drawerWidth = 1100;

/**
 * The shell's state: the saved prefs (each space's last screen, closed sections, the hidden
 * sidebar, the switcher) and the drawer, which is never saved. Every change saves the prefs.
 */
export class ShellState {
  prefs: UiPrefs = $state.raw(defaultPrefs);
  drawer = $state(false);
  readonly #store: PrefStore | undefined;

  constructor(store: PrefStore | undefined = browserStore()) {
    this.#store = store;
    this.prefs = loadPrefs(store);
  }

  /** Notes `path` (pathname plus query) as the last screen of its space, if it is a known screen. */
  remember(pathname: string, search = ''): void {
    if (!isKnown(pathname)) return;
    const key = spaceOf(pathname).key;
    const path = `${pathname}${search}`;
    if (this.prefs.lastRoute[key] === path) return;
    this.#set({ lastRoute: { ...this.prefs.lastRoute, [key]: path } });
  }

  /**
   * Where switching to `key` from `pathname` goes: the space's home when it is the current space
   * already, else its last screen, else its home.
   */
  routeFor(key: SpaceKey, pathname: string): string {
    const space = spaceByKey(key);
    const home = resolve(space.home);
    if (spaceOf(pathname).key === key) return home;
    const last = this.prefs.lastRoute[key];
    return last && isKnown(last.split('?', 1)[0] ?? '') ? last : home;
  }

  isClosed(space: SpaceKey, section: string): boolean {
    return this.prefs.sbClosed.includes(`${space}:${section}`);
  }

  toggleSection(space: SpaceKey, section: string): void {
    const id = `${space}:${section}`;
    const closed = this.prefs.sbClosed;
    this.#set({ sbClosed: closed.includes(id) ? closed.filter((c) => c !== id) : [...closed, id] });
  }

  /** Hides or shows the sidebar: a saved pref while it is in the layout, else the drawer. */
  toggleSidebar(inLayout: boolean): void {
    if (!inLayout) {
      this.drawer = !this.drawer;
      return;
    }
    this.drawer = false;
    this.#set({ sbCollapsed: !this.prefs.sbCollapsed });
  }

  setSwitcher(switcher: Switcher): void {
    this.#set({ switcher });
  }

  #set(change: Partial<UiPrefs>): void {
    this.prefs = { ...this.prefs, ...change };
    savePrefs(this.prefs, this.#store);
  }
}

/** The keys a shortcut reads, so tests can pass a plain object. */
export type ShortcutKey = Pick<
  KeyboardEvent,
  'metaKey' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'code' | 'key'
>;

const order: readonly SpaceKey[] = ['client', 'tests', 'runs', 'agents', 'settings'];

/** The space a key press switches to: ⌘1–⌘5 in rail order, ⌘, Settings (⌘ or Ctrl, no Alt or Shift). */
export function shortcutSpace(event: ShortcutKey): SpaceKey | undefined {
  if (!(event.metaKey || event.ctrlKey) || event.altKey || event.shiftKey) return undefined;
  if (event.key === ',') return 'settings';
  const digit = /^Digit([1-5])$/.exec(event.code)?.[1];
  return digit ? order[Number(digit) - 1] : undefined;
}
