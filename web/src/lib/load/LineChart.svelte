<script lang="ts" module>
  import type { LoadMetric } from './port.ts';

  /** One line of a chart: the tick field it draws, its legend label and its tone class. */
  export type Series = Readonly<{ metric: LoadMetric; label: string; tone: 'a' | 'b' | 'c' }>;
</script>

<script lang="ts">
  import { breachOf, clock, linePath, margin, peakIndex, scale, timeTicks, yTop } from './chart.ts';
  import type { LoadThreshold, LoadTick } from './port.ts';

  /**
   * One live load chart, drawn by hand in SVG at its plot's measured width (ui.md §6): a line per
   * series, the threshold as a dashed line with its label, and each breach as a shaded band with a
   * ✕ marker at its peak, so a breach never rests on colour alone (D641). **Table** shows the
   * same numbers every 30 s.
   */
  let {
    title,
    sub,
    series,
    ticks,
    tMax,
    unit,
    threshold,
    table = false,
  }: {
    title: string;
    sub: string;
    series: readonly Series[];
    ticks: readonly LoadTick[];
    /** The run's planned length in seconds, so the x axis does not move while it runs. */
    tMax: number;
    unit: string;
    threshold?: LoadThreshold | undefined;
    table?: boolean;
  } = $props();

  const uid = $props.id();
  let width = $state(0);
  const height = 220;
  const metrics = $derived(series.map((s) => s.metric));
  const at = $derived(scale([Math.max(240, width), height], tMax, yTop(ticks, metrics, threshold)));
  const grid = $derived([0, 0.25, 0.5, 0.75, 1].map((f) => f * at.yMax));
  const breach = $derived(breachOf(ticks, metrics, threshold));
  const spans = $derived(breach.spans);
  const marked = $derived(breach.metric);
  const last = $derived(ticks.at(-1));
  const now = $derived(
    last ? `now ${fmt(last[series.at(-1)?.metric ?? 'rps'])}${unit}` : 'waiting for the first tick',
  );
  const rows = $derived(ticks.filter((_, i) => i % 6 === 0));
  const summary = $derived(
    spans.length
      ? `${threshold?.label ?? marked} breached from ${spans.map(([a, b]) => `${clock(ticks[a]?.t ?? 0)} to ${clock(ticks[b]?.t ?? 0)}`).join(', ')}`
      : threshold
        ? `${threshold.label} held`
        : '',
  );

  function fmt(value: number): string {
    return value >= 1000 ? Math.round(value).toLocaleString('en-US') : String(+value.toFixed(2));
  }

  function over(tick: LoadTick, metric: LoadMetric): boolean {
    if (marked !== metric) return false;
    return threshold ? tick[metric] > threshold.limit : tick.breaching.includes(metric);
  }
</script>

<figure class="chart st-card" aria-labelledby="{uid}-t">
  <div class="head">
    <h3 class="st-title3" id="{uid}-t">{title}</h3>
    <span class="st-secondary">{sub} · {now}</span>
  </div>
  {#if series.length > 1 || threshold}
    <ul class="legend" aria-label="Series">
      {#each series as s (s.metric)}<li><i class="sw t-{s.tone}"></i>{s.label}</li>{/each}
      {#if threshold}<li><i class="sw th"></i>threshold</li>{/if}
    </ul>
  {/if}
  {#if table}
    <table class="vc-table">
      <caption class="vc-sr">{title} every 30 s</caption>
      <thead>
        <tr>
          <th scope="col">Time</th>
          <th scope="col" class="r">Users</th>
          {#each series as s (s.metric)}<th scope="col" class="r">{s.label}</th>{/each}
        </tr>
      </thead>
      <tbody>
        {#each rows as tick (tick.t)}
          <tr>
            <td class="vc-mono">{clock(tick.t)}</td>
            <td class="r">{fmt(tick.users)}</td>
            {#each series as s (s.metric)}
              <td class="r" class:bad={over(tick, s.metric)}>
                {fmt(tick[s.metric])}{unit}{#if over(tick, s.metric)}<span class="vc-sr">
                    over the limit</span
                  >{/if}
              </td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
  {:else}
    <div class="plot" bind:clientWidth={width} role="img" aria-label="{title}. {now}. {summary}">
      <svg
        viewBox="0 0 {at.width} {at.height}"
        width={at.width}
        height={at.height}
        aria-hidden="true"
      >
        {#each grid as v (v)}
          <line class="grid" x1={margin.l} x2={at.width - margin.r} y1={at.y(v)} y2={at.y(v)} />
          <text class="ax" x={margin.l - 6} y={at.y(v) + 4} text-anchor="end">{fmt(v)}</text>
        {/each}
        {#each timeTicks(tMax, at.width) as t (t)}
          <text class="ax" x={at.x(t)} y={at.height - 8} text-anchor="middle">{clock(t)}</text>
        {/each}
        {#each spans as [a, b] (a)}
          <rect
            class="band"
            x={at.x(ticks[a]?.t ?? 0)}
            y={margin.t}
            width={Math.max(2, at.x(ticks[b]?.t ?? 0) - at.x(ticks[a]?.t ?? 0))}
            height={at.height - margin.t - margin.b}
          />
        {/each}
        {#if threshold}
          <line
            class="th"
            x1={margin.l}
            x2={at.width - margin.r}
            y1={at.y(threshold.limit)}
            y2={at.y(threshold.limit)}
          />
          <text class="thl" x={margin.l + 6} y={at.y(threshold.limit) - 5}>{threshold.label}</text>
        {/if}
        {#each series as s (s.metric)}
          {#if ticks.length > 1}<path
              class="line t-{s.tone}"
              d={linePath(ticks, s.metric, at)}
            />{/if}
          {#if last}
            <circle class="dot t-{s.tone}" cx={at.x(last.t)} cy={at.y(last[s.metric])} r="3.5" />
            <text class="end" x={at.x(last.t) + 8} y={at.y(last[s.metric]) + 4}>{s.label}</text>
          {/if}
        {/each}
        {#if marked}
          {#each spans as span (span[0])}
            {@const peak = ticks[peakIndex(ticks, marked, span)]}
            {#if peak}
              <g class="breach">
                <circle cx={at.x(peak.t)} cy={at.y(peak[marked])} r="5" />
                <text
                  x={at.x(peak.t)}
                  y={Math.max(margin.t + 10, at.y(peak[marked]) - 10)}
                  text-anchor="middle">✕ breach {clock(ticks[span[0]]?.t ?? 0)}</text
                >
              </g>
            {/if}
          {/each}
        {/if}
      </svg>
    </div>
  {/if}
  {#if summary}<figcaption class="st-secondary" class:bad={spans.length > 0}>
      {spans.length ? '✕' : '✓'}
      {summary}
    </figcaption>{/if}
</figure>

<style>
  .chart {
    display: grid;
    gap: 8px;
    min-width: 0;
    margin: 0;
    padding: 14px 16px;
  }
  .head {
    display: grid;
    gap: 2px;
  }
  .head h3 {
    margin: 0;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 14px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--vc-size-caption);
    color: var(--st-text-secondary);
  }
  .legend li {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .sw {
    width: 14px;
    height: 3px;
    border-radius: 2px;
    background: currentColor;
  }
  .sw.th {
    height: 0;
    border-top: 2px dashed var(--st-text-secondary);
    background: none;
  }
  .plot {
    min-width: 0;
    overflow: hidden;
  }
  svg {
    display: block;
  }
  .grid {
    stroke: var(--st-chrome-separator);
  }
  .ax,
  .end,
  .thl {
    fill: var(--st-text-tertiary);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .end {
    fill: var(--st-text-secondary);
  }
  .thl {
    fill: var(--st-text-secondary);
  }
  .line {
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linejoin: round;
  }
  .dot {
    fill: currentColor;
  }
  .t-a {
    color: var(--vc-info);
  }
  .t-b {
    color: var(--vc-accent-mid);
  }
  .t-c {
    color: var(--st-accent);
  }
  .th {
    stroke: var(--st-text-secondary);
    stroke-dasharray: 5 4;
  }
  .band {
    fill: var(--vc-fail-soft);
  }
  .breach circle {
    fill: none;
    stroke: var(--vc-fail);
    stroke-width: 2;
  }
  .breach text {
    fill: var(--vc-fail-ink);
    font-size: 11px;
    font-weight: 600;
  }
  figcaption {
    font-size: var(--vc-size-caption);
  }
  .bad {
    color: var(--vc-fail-ink);
  }
  .r {
    text-align: right;
  }
</style>
