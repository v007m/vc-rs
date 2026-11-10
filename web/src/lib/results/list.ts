/** Rows rendered above and below the visible ones, so a fast scroll shows no gap. */
const OVERSCAN = 10;

/** The rows `[first, last)` a virtual list renders for a scroll position. */
export function windowOf(
  scrollTop: number,
  height: number,
  count: number,
  row: number,
): Readonly<{ first: number; last: number }> {
  const first = Math.max(0, Math.floor(scrollTop / row) - OVERSCAN);
  const last = Math.min(count, first + Math.ceil(height / row) + 2 * OVERSCAN);
  return { first, last };
}

/** The listbox keys: where each moves the active row, given the rows that fit on a page. */
const moves: Record<string, (at: number, count: number, page: number) => number> = {
  ArrowDown: (at) => at + 1,
  ArrowUp: (at) => at - 1,
  Home: () => 0,
  End: (_, count) => count - 1,
  PageDown: (at, _, page) => at + page,
  PageUp: (at, _, page) => at - page,
};

/** The active row after `key`, or `undefined` when the key does not move it. */
export function listMove(key: string, at: number, count: number, page: number): number | undefined {
  const move = moves[key];
  if (!move || count === 0) return undefined;
  return Math.min(count - 1, Math.max(0, move(at, count, page)));
}

/** The scroll position that shows row `at` whole, moving as little as possible. */
export function scrollFor(at: number, scrollTop: number, height: number, row: number): number {
  const top = at * row;
  if (top < scrollTop) return top;
  if (top + row > scrollTop + height) return top + row - height;
  return scrollTop;
}
