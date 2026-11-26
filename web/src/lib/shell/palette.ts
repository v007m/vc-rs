import type { RouteId } from '$app/types';
import type { Space } from './spaces.ts';

/** One result of the command palette: a screen to open, under its space. */
export interface PaletteItem {
  readonly id: string;
  readonly label: string;
  /** The space it belongs to, the heading it is grouped under. */
  readonly group: string;
  readonly href: string;
  /** The space's shortcut, on its home screen only. */
  readonly shortcut?: string;
}

/** The results under one heading, in the spaces' rail order. */
export interface PaletteGroup {
  readonly group: string;
  readonly items: readonly PaletteItem[];
}

/** Every screen of every space as a palette item; `href` resolves a route (SvelteKit's resolve). */
export function paletteItems(
  spaces: readonly Space[],
  href: (route: RouteId) => string,
): PaletteItem[] {
  return spaces.flatMap((space) =>
    space.sections.flatMap((section) =>
      section.views.map((view) => ({
        id: `${space.key}:${view.route}`,
        label: view.title,
        group: space.label,
        href: href(view.route),
        ...(view.route === space.home ? { shortcut: space.shortcut } : {}),
      })),
    ),
  );
}

/** Whether every word of `query` starts a word of the item's label or group (case-insensitive). */
function matches(item: PaletteItem, words: readonly string[]): boolean {
  const hay = `${item.label} ${item.group}`.toLowerCase().split(/[\s/]+/);
  return words.every((word) => hay.some((part) => part.startsWith(word)));
}

/** The items `query` keeps, grouped by space in the order given; an empty query keeps all. */
export function filterPalette(items: readonly PaletteItem[], query: string): PaletteGroup[] {
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  const groups = new Map<string, PaletteItem[]>();
  for (const item of items) {
    if (!matches(item, words)) continue;
    const list = groups.get(item.group) ?? [];
    list.push(item);
    groups.set(item.group, list);
  }
  return [...groups].map(([group, list]) => ({ group, items: list }));
}

/** The item `step` places after `current` in the flattened results, wrapping at both ends. */
export function stepPalette(
  groups: readonly PaletteGroup[],
  current: string | undefined,
  step: number,
): string | undefined {
  const flat = groups.flatMap((group) => group.items.map((item) => item.id));
  if (flat.length === 0) return undefined;
  const at = current === undefined ? -1 : flat.indexOf(current);
  if (at < 0) return step < 0 ? flat.at(-1) : flat[0];
  return flat[(at + step + flat.length) % flat.length];
}
