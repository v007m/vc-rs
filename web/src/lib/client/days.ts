import type { HistoryEntry } from './seams.ts';

export type HistoryDay = Readonly<{ label: string; entries: readonly HistoryEntry[] }>;

const DAY = 24 * 60 * 60 * 1000;

function dayStart(ms: number): number {
  const at = new Date(ms);
  return new Date(at.getFullYear(), at.getMonth(), at.getDate()).getTime();
}

function dayLabel(start: number, today: number): string {
  if (start === today) return 'Today';
  if (start === dayStart(today - DAY)) return 'Yesterday';
  const style = { weekday: 'short', day: 'numeric', month: 'short' } as const;
  return new Date(start).toLocaleDateString('en-GB', style);
}

/** History entries in local days, newest day first, each day keeping its order. */
export function byDay(entries: readonly HistoryEntry[], nowMs: number): HistoryDay[] {
  const today = dayStart(nowMs);
  const days = new Map<number, HistoryEntry[]>();
  for (const entry of entries) {
    const start = dayStart(entry.startedMs);
    days.set(start, [...(days.get(start) ?? []), entry]);
  }
  return [...days]
    .sort(([a], [b]) => b - a)
    .map(([start, list]) => ({ label: dayLabel(start, today), entries: list }));
}

/** A time of day, `14:06`, for a history line. */
export function clock(ms: number): string {
  return new Date(ms).toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' });
}
