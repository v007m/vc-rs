<script lang="ts">
  import { watchWidth } from '#lib/ui/width.ts';
  import { count, type Usage } from './usage.ts';
  import {
    chartHeight,
    chartLayout,
    chartSummary,
    dailyTotals,
    dayLabels,
    margin,
    toneOf,
  } from './usageChart.ts';

  /**
   * Calls per day by agent, one stacked bar a day, drawn by hand in SVG at the width it has. The
   * same numbers sit in a visually hidden table, and the SVG's title and description say them in
   * words, so nothing rests on colour or sight alone.
   */
  let { usage }: { usage: Usage } = $props();

  const uid = $props.id();
  let width = $state(0);
  const at = $derived(chartLayout(usage, Math.max(320, width)));
  const summary = $derived(chartSummary(usage));
  const days = $derived(dayLabels(usage));
  const totals = $derived(dailyTotals(usage));
</script>

<figure class="chart vc-panel" aria-labelledby="{uid}-h">
  <h2 class="st-title3" id="{uid}-h">Calls per day</h2>
  <ul class="legend" aria-label="Agents">
    {#each usage.agents as a, i (a.agent)}
      <li><i class="sw t-{toneOf(i)}"></i>{a.agent}</li>
    {/each}
  </ul>
  <div class="plot" {@attach watchWidth((w) => (width = w))}>
    <svg
      viewBox="0 0 {at.width} {chartHeight}"
      width={at.width}
      height={chartHeight}
      role="img"
      aria-labelledby="{uid}-t {uid}-d"
    >
      <title id="{uid}-t">Calls per day by agent</title>
      <desc id="{uid}-d">{summary}</desc>
      {#each at.grid as tick (tick.value)}
        <line class="grid" x1={margin.l} x2={at.width - margin.r} y1={tick.at} y2={tick.at} />
        <text class="ax" x={margin.l - 6} y={tick.at + 4} text-anchor="end">{tick.text}</text>
      {/each}
      {#each at.bars as bar, d (d)}
        {#each bar.segments as s (s.agent)}
          <rect class="bar t-{s.tone}" x={bar.x} y={s.y} width={bar.w} height={s.h} rx="1.5"
            ><title>{bar.day} · {s.agent}: {count(s.calls)} calls</title></rect
          >
        {/each}
      {/each}
      {#each at.labels as label (label.text)}
        <text class="ax" x={label.at} y={chartHeight - 6} text-anchor="middle">{label.text}</text>
      {/each}
    </svg>
  </div>
  <table class="vc-sr">
    <caption>Calls per day by agent</caption>
    <thead>
      <tr>
        <th scope="col">Day</th>
        {#each usage.agents as a (a.agent)}<th scope="col">{a.agent}</th>{/each}
        <th scope="col">All agents</th>
      </tr>
    </thead>
    <tbody>
      {#each days as day, i (i)}
        <tr>
          <th scope="row">{day}</th>
          {#each usage.agents as a (a.agent)}<td>{count(a.daily[i] ?? 0)}</td>{/each}
          <td>{count(totals[i] ?? 0)}</td>
        </tr>
      {/each}
    </tbody>
  </table>
</figure>

<style>
  .chart {
    display: grid;
    gap: 8px;
  }
  h2 {
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
    width: 10px;
    height: 10px;
    border-radius: 2px;
    background: currentColor;
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
  .ax {
    fill: var(--st-text-secondary);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  .bar {
    fill: currentColor;
  }
  /* series inks: each is 3:1 or more on the card in both themes */
  .t-0 {
    color: var(--vc-info);
  }
  .t-1 {
    color: var(--vc-allow);
  }
  .t-2 {
    color: var(--vc-m-stream);
  }
  .t-3 {
    color: var(--vc-blocked);
  }
  .t-4 {
    color: var(--vc-m-patch);
  }
</style>
