import type { SpacePreset } from './spaces.ts';

export type Theme = 'system' | 'light' | 'dark';
/** `space` keeps each space's own preset (D591); a preset puts one accent on every space. */
export type Accent = 'space' | SpacePreset;
export type Density = 'comfortable' | 'compact';
/** `system` follows the OS's reduce-motion setting; `reduced` turns motion off here as well. */
export type Motion = 'system' | 'reduced';

/** How this app looks, in this browser only (Settings › Appearance). */
export type Appearance = Readonly<{
  theme: Theme;
  accent: Accent;
  density: Density;
  motion: Motion;
}>;

export const defaultAppearance: Appearance = {
  theme: 'system',
  accent: 'space',
  density: 'comfortable',
  motion: 'system',
};

/** Every choice of each setting, in the order the Appearance page lists them. */
export const choices = {
  theme: ['system', 'light', 'dark'],
  accent: ['space', 'lilac', 'ember', 'tide', 'grove', 'graphite'],
  density: ['comfortable', 'compact'],
  motion: ['system', 'reduced'],
} as const satisfies { [K in keyof Appearance]: readonly Appearance[K][] };

function pick<K extends keyof Appearance>(key: K, value: unknown): Appearance[K] {
  const known: readonly unknown[] = choices[key];
  return known.includes(value) ? (value as Appearance[K]) : defaultAppearance[key];
}

/** Appearance read from a saved value, keeping each setting only when it is a known choice. */
export function parseAppearance(value: unknown): Appearance {
  const saved =
    typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {};
  return {
    theme: pick('theme', saved['theme']),
    accent: pick('accent', saved['accent']),
    density: pick('density', saved['density']),
    motion: pick('motion', saved['motion']),
  };
}

/** The preset the shell paints: the chosen accent, or the space's own. */
export function presetFor(appearance: Appearance, space: SpacePreset): SpacePreset {
  return appearance.accent === 'space' ? space : appearance.accent;
}

/**
 * Puts the appearance on the page root as Strata's hooks: `data-theme` (absent follows the OS),
 * `data-density="compact"` and `data-motion="reduced"` (vibranium.css).
 */
export function applyAppearance(root: HTMLElement, appearance: Appearance): void {
  const set = (name: string, value: string | undefined): void => {
    if (value === undefined) root.removeAttribute(`data-${name}`);
    else root.setAttribute(`data-${name}`, value);
  };
  set('theme', appearance.theme === 'system' ? undefined : appearance.theme);
  set('density', appearance.density === 'compact' ? 'compact' : undefined);
  set('motion', appearance.motion === 'reduced' ? 'reduced' : undefined);
}

/** Each choice as the Appearance page and the palette name it. */
export const choiceLabels: Readonly<Record<string, string>> = {
  system: 'Match the system',
  light: 'Light',
  dark: 'Dark',
  space: 'Each space’s own',
  lilac: 'Lilac',
  ember: 'Ember',
  tide: 'Tide',
  grove: 'Grove',
  graphite: 'Graphite',
  comfortable: 'Comfortable',
  compact: 'Compact',
  reduced: 'Reduced',
};
