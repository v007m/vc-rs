import type { LoadMetric, LoadThreshold, LoadTick } from './port.ts';

/** The plot's margins inside the SVG, in px: room for the y axis, end labels and the x axis. */
export const margin = { l: 46, r: 72, t: 16, b: 26 } as const;

/** A round number at or above `value`: 1, 2, 2.5 or 5 times a power of ten. */
export function niceCeil(value: number): number {
  if (!(value > 0)) return 1;
  const power = 10 ** Math.floor(Math.log10(value));
  const step = [1, 2, 2.5, 5, 10].find((f) => f * power >= value) ?? 10;
  return step * power;
}

export type Scale = Readonly<{
  x: (t: number) => number;
  y: (v: number) => number;
  yMax: number;
  width: number;
  height: number;
}>;

/** Maps seconds `0..tMax` and values `0..yMax` onto a `width` × `height` plot. */
export function scale(size: readonly [number, number], tMax: number, yMax: number): Scale {
  const [width, height] = size;
  const span = Math.max(1, tMax);
  return {
    x: (t) => margin.l + (t / span) * (width - margin.l - margin.r),
    y: (v) => margin.t + (1 - v / yMax) * (height - margin.t - margin.b),
    yMax,
    width,
    height,
  };
}

/** The SVG path through each tick's `metric`. */
export function linePath(ticks: readonly LoadTick[], metric: LoadMetric, at: Scale): string {
  return ticks
    .map((tick, i) => `${i ? 'L' : 'M'}${at.x(tick.t).toFixed(1)},${at.y(tick[metric]).toFixed(1)}`)
    .join('');
}

/** Each run of ticks where `over` holds, as the first and last tick's index. */
export function spansWhere(
  ticks: readonly LoadTick[],
  over: (tick: LoadTick) => boolean,
): [number, number][] {
  const spans: [number, number][] = [];
  let start = -1;
  ticks.forEach((tick, i) => {
    const on = over(tick);
    if (on && start < 0) start = i;
    if (!on && start >= 0) {
      spans.push([start, i - 1]);
      start = -1;
    }
  });
  if (start >= 0) spans.push([start, ticks.length - 1]);
  return spans;
}

/** Each run of ticks over `limit`, as the first and last tick's index. */
export function breaches(
  ticks: readonly LoadTick[],
  metric: LoadMetric,
  limit: number,
): [number, number][] {
  return spansWhere(ticks, (tick) => tick[metric] > limit);
}

/**
 * The metric a chart marks breaches of, and the ticks where it breached: the threshold's when the
 * run's thresholds are known, else the first shown metric a tick names in `breaching`.
 */
export function breachOf(
  ticks: readonly LoadTick[],
  metrics: readonly LoadMetric[],
  threshold?: LoadThreshold,
): Readonly<{ metric?: LoadMetric; spans: [number, number][] }> {
  if (threshold)
    return { metric: threshold.metric, spans: breaches(ticks, threshold.metric, threshold.limit) };
  const metric = metrics.find((m) => ticks.some((t) => t.breaching.includes(m)));
  return metric
    ? { metric, spans: spansWhere(ticks, (t) => t.breaching.includes(metric)) }
    : { spans: [] };
}

/** The index of the tick with the highest `metric` in `[from, to]`. */
export function peakIndex(
  ticks: readonly LoadTick[],
  metric: LoadMetric,
  [from, to]: readonly [number, number],
): number {
  let best = from;
  for (let i = from; i <= to; i++) {
    if ((ticks[i]?.[metric] ?? 0) > (ticks[best]?.[metric] ?? 0)) best = i;
  }
  return best;
}

/** The y axis's top: the highest value, or the threshold with room above it. */
export function yTop(
  ticks: readonly LoadTick[],
  metrics: readonly LoadMetric[],
  threshold?: LoadThreshold,
): number {
  const values = ticks.flatMap((tick) => metrics.map((m) => tick[m]));
  return niceCeil(Math.max(threshold ? threshold.limit * 1.25 : 0, ...values.map((v) => v * 1.05)));
}

/** `seconds` as `m:ss`. */
export function clock(seconds: number): string {
  const whole = Math.max(0, Math.round(seconds));
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, '0')}`;
}

/** The x axis's labels: every 60 s, or every 120 s on a narrow plot. */
export function timeTicks(tMax: number, width: number): number[] {
  const every = width < 520 ? 120 : 60;
  const out: number[] = [];
  for (let t = 0; t <= tMax; t += every) out.push(t);
  return out;
}
