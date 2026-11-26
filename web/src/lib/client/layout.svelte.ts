import { browserStore, type PrefStore } from '#lib/shell/prefs.ts';

export type Orient = 'auto' | 'side' | 'stack';

export const layoutKey = 'vc.ui.client';

export function clampSplit(value: number): number {
  return Math.round(Math.min(80, Math.max(20, value)));
}

type Saved = { orient: Orient; split: number };

const fallback: Saved = { orient: 'auto', split: 50 };

function read(store: PrefStore | undefined): Saved {
  try {
    const value: unknown = JSON.parse(store?.getItem(layoutKey) ?? '{}');
    if (typeof value !== 'object' || value === null) return fallback;
    const { orient, split } = value as Partial<Record<keyof Saved, unknown>>;
    return {
      orient: orient === 'side' || orient === 'stack' ? orient : 'auto',
      split: typeof split === 'number' ? clampSplit(split) : 50,
    };
  } catch {
    return fallback;
  }
}

/**
 * How the Client lays out its request and response cards: side by side or stacked (`auto`
 * follows the window), and the request card's share in percent. Kept per browser.
 */
export class ClientLayout {
  orient: Orient = $state('auto');
  split = $state(50);
  readonly #store: PrefStore | undefined;

  constructor(store: PrefStore | undefined = browserStore()) {
    this.#store = store;
    const saved = read(store);
    this.orient = saved.orient;
    this.split = saved.split;
  }

  setOrient(orient: Orient): void {
    this.orient = orient;
    this.#save();
  }

  setSplit(split: number): void {
    this.split = clampSplit(split);
    this.#save();
  }

  #save(): void {
    try {
      this.#store?.setItem(layoutKey, JSON.stringify({ orient: this.orient, split: this.split }));
    } catch {
      // A browser that refuses storage keeps the layout for this visit only.
    }
  }
}
