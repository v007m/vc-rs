import type { SpaceKey } from './spaces.ts';

/** Where the space switcher sits: an icon rail left of the sidebar, or a row at its foot. */
export type Switcher = 'rail' | 'foot';

/** The shell's choices kept between visits, in this browser only. */
export type UiPrefs = Readonly<{
  /** The last screen of each space, as a path with its query, to reopen on switching back. */
  lastRoute: Readonly<Partial<Record<SpaceKey, string>>>;
  /** The closed sidebar sections, as `<space>:<section>` keys (D515). */
  sbClosed: readonly string[];
  /** Whether the sidebar is hidden while it is in the layout (wider than 1100px). */
  sbCollapsed: boolean;
  switcher: Switcher;
}>;

export const defaultPrefs: UiPrefs = {
  lastRoute: {},
  sbClosed: [],
  sbCollapsed: false,
  switcher: 'rail',
};

/** The localStorage key; prefs are a convenience, so nothing breaks without them. */
export const prefsKey = 'vc.ui.prefs';

/** The part of `Storage` the prefs use, so tests can pass one that throws. */
export type PrefStore = Pick<Storage, 'getItem' | 'setItem'>;

/** The browser's localStorage, or undefined where reading it throws (blocked site data). */
export function browserStore(): PrefStore | undefined {
  try {
    return globalThis.localStorage;
  } catch {
    return undefined;
  }
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === 'object' && value !== null && !Array.isArray(value);

/** The string values of `value`, when it is an object; anything else is dropped. */
function strings(value: unknown): Record<string, string> {
  if (!isRecord(value)) return {};
  return Object.fromEntries(
    Object.entries(value).filter(
      (entry): entry is [string, string] => typeof entry[1] === 'string',
    ),
  );
}

/** Prefs read from `raw`, keeping each field only when it has the right shape. */
export function parsePrefs(raw: string | null): UiPrefs {
  if (raw === null) return defaultPrefs;
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return defaultPrefs;
  }
  if (!isRecord(value)) return defaultPrefs;
  const closed = Array.isArray(value.sbClosed) ? value.sbClosed : [];
  return {
    lastRoute: strings(value.lastRoute),
    sbClosed: closed.filter((key): key is string => typeof key === 'string'),
    sbCollapsed: value.sbCollapsed === true,
    switcher: value.switcher === 'foot' ? 'foot' : 'rail',
  };
}

/** The saved prefs, or the defaults when there are none, they are malformed, or storage throws. */
export function loadPrefs(store: PrefStore | undefined): UiPrefs {
  try {
    return parsePrefs(store?.getItem(prefsKey) ?? null);
  } catch {
    return defaultPrefs;
  }
}

/** Saves `prefs`; a store that throws (full, or blocked) leaves them unsaved, silently. */
export function savePrefs(prefs: UiPrefs, store: PrefStore | undefined): void {
  try {
    store?.setItem(prefsKey, JSON.stringify(prefs));
  } catch {
    // A convenience only: the shell keeps working with the prefs in memory.
  }
}
