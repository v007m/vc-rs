/** One tab of a tab list. */
export type Tab = Readonly<{ id: string; label: string }>;

const moves: Record<string, (at: number, count: number) => number> = {
  ArrowRight: (at, count) => (at + 1) % count,
  ArrowLeft: (at, count) => (at - 1 + count) % count,
  Home: () => 0,
  End: (_, count) => count - 1,
};

/** The tab a key press selects in a WAI-ARIA tab list, or `undefined` when the key does nothing. */
export function tabMove(tabs: readonly Tab[], current: string, key: string): string | undefined {
  const move = moves[key];
  if (!move || tabs.length === 0) return undefined;
  const at = Math.max(
    0,
    tabs.findIndex((t) => t.id === current),
  );
  return tabs[move(at, tabs.length)]?.id;
}
