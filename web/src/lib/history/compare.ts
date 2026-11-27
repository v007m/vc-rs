import type { Tab } from '#lib/ui/tabs.ts';
import { moved, type RunComparison, type TimingChange } from './seams.ts';

/** How far a test's time must move to count as slower or faster (D637): 20 % and 50 ms. */
export const MOVED = { percent: 20, floorMs: 50 } as const;

/** The comparison's tabs, in order: what broke first, then what got fixed, then timing. */
export type CompareTab = 'failing' | 'fixed' | 'slower' | 'faster' | 'shapes';

/** The slower and faster tests, the biggest move first. */
export function timingMoves(
  timing: readonly TimingChange[],
): Record<'slower' | 'faster', TimingChange[]> {
  const by = (a: TimingChange, b: TimingChange): number =>
    Math.abs(b.headMs - b.baseMs) - Math.abs(a.headMs - a.baseMs);
  return {
    slower: moved(timing, 1, MOVED).sort(by),
    faster: moved(timing, -1, MOVED).sort(by),
  };
}

/** The tab strip with each tab's count; Newly failing is marked bad when it has any. */
export function compareTabs(result: RunComparison): Tab[] {
  const { slower, faster } = timingMoves(result.timing);
  return [
    {
      id: 'failing',
      label: 'Newly failing',
      count: String(result.newlyFailing.length),
      bad: result.newlyFailing.length > 0,
    },
    { id: 'fixed', label: 'Fixed', count: String(result.fixed.length) },
    { id: 'slower', label: 'Slower', count: String(slower.length) },
    { id: 'faster', label: 'Faster', count: String(faster.length) },
    { id: 'shapes', label: 'Response shapes', count: String(result.shapes.length) },
  ];
}

/** One line that sums the comparison up, for the screen reader and the header. */
export function compareLine(result: RunComparison): string {
  const { slower } = timingMoves(result.timing);
  const parts = [
    `${result.newlyFailing.length} newly failing`,
    `${result.fixed.length} fixed`,
    `${slower.length} slower`,
  ];
  if (result.shapes.length) parts.push(`${result.shapes.length} shape changes`);
  return parts.join(' · ');
}
