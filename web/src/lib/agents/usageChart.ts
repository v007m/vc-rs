import { niceMax } from '#lib/insights/stats.ts';
import { count, type Usage } from './usage.ts';

/** A UTC day; the engine counts days in UTC (D709). */
export const DAY_MS = 86_400_000;

/** How many series tones the chart has; a sixth agent reuses the first tone. */
export const TONES = 5;

/** The plot's inset inside the SVG, room for the axis labels. */
export const margin = { l: 40, r: 8, t: 10, b: 24 } as const;

/** The chart's height in px; its width is the space it has. */
export const chartHeight = 180;

export type Segment = Readonly<{
  agent: string;
  tone: number;
  calls: number;
  y: number;
  h: number;
}>;

/** One day's bar: its agents' calls stacked, the busiest agent at the foot. */
export type Bar = Readonly<{
  day: string;
  x: number;
  w: number;
  total: number;
  segments: readonly Segment[];
}>;

export type Tick = Readonly<{ at: number; text: string }>;

export type ChartLayout = Readonly<{
  width: number;
  bars: readonly Bar[];
  grid: readonly (Tick & { value: number })[];
  labels: readonly Tick[];
}>;

/** `Nov 27`: the UTC day starting at `ms`. */
export function dayLabel(ms: number): string {
  return new Date(ms).toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    timeZone: 'UTC',
  });
}

/** The window's days, the oldest first, as labels. */
export function dayLabels(usage: Usage): string[] {
  const days = Math.max(0, ...usage.agents.map((a) => a.daily.length));
  return Array.from({ length: days }, (_, i) => dayLabel(usage.fromMs + i * DAY_MS));
}

/** Each day's calls over every agent. */
export function dailyTotals(usage: Usage): number[] {
  return dayLabels(usage).map((_, i) =>
    usage.agents.reduce((sum, a) => sum + (a.daily[i] ?? 0), 0),
  );
}

/** The tone class index of the agent at `index` in the engine's order. */
export function toneOf(index: number): number {
  return index % TONES;
}

function segments(usage: Usage, day: number, y: (v: number) => number): Segment[] {
  let below = 0;
  return usage.agents.flatMap((a, i) => {
    const calls = a.daily[day] ?? 0;
    if (calls === 0) return [];
    const top = y(below + calls);
    // a 1px gap parts a segment from the one beneath it
    const h = Math.max(1, y(below) - top - (below > 0 ? 1 : 0));
    below += calls;
    return [{ agent: a.agent, tone: toneOf(i), calls, y: top, h }];
  });
}

/** Every `step`th day is labelled, counting back from today, so today always is. */
function labels(days: readonly string[], plot: number, x: (i: number) => number): Tick[] {
  const step = Math.ceil(days.length / Math.max(2, Math.floor(plot / 64)));
  return days.flatMap((text, i) =>
    (days.length - 1 - i) % step === 0 ? [{ at: x(i), text }] : [],
  );
}

/** Where every mark of the chart goes at `width` px. */
export function chartLayout(usage: Usage, width: number): ChartLayout {
  const days = dayLabels(usage);
  const totals = dailyTotals(usage);
  const top = niceMax(Math.max(0, ...totals));
  const plotW = width - margin.l - margin.r;
  const plotH = chartHeight - margin.t - margin.b;
  const slot = plotW / Math.max(1, days.length);
  const w = Math.max(1, Math.min(28, slot * 0.64));
  const x = (i: number) => margin.l + i * slot + (slot - w) / 2;
  const y = (v: number) => margin.t + plotH * (1 - v / top);
  const bars = days.map((day, i) => ({
    day,
    x: x(i),
    w,
    total: totals[i] ?? 0,
    segments: segments(usage, i, y),
  }));
  const grid = [0, top / 2, top]
    .filter(Number.isInteger)
    .map((value) => ({ value, at: y(value), text: count(value) }));
  return { width, bars, grid, labels: labels(days, plotW, (i) => x(i) + w / 2) };
}

/** The chart in words, for its accessible name. */
export function chartSummary(usage: Usage): string {
  const days = dayLabels(usage);
  const totals = dailyTotals(usage);
  const all = totals.reduce((a, b) => a + b, 0);
  const peak = totals.indexOf(Math.max(...totals));
  const by = usage.agents.map((a) => `${a.agent} ${count(a.total.calls)}`).join(', ');
  const span = `${days.length} days, ${days[0] ?? ''} to ${days.at(-1) ?? ''}`;
  const busiest =
    all > 0 ? ` Busiest day ${days[peak] ?? ''} with ${count(totals[peak] ?? 0)} calls.` : '';
  return `${count(all)} calls over ${span}: ${by}.${busiest}`;
}
