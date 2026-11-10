<script lang="ts" module>
  /** One column; `label` is what a screen reader and the hover readout say about it. */
  export type Bar = Readonly<{ key: string; value: number; label: string; marked?: boolean }>;
</script>

<script lang="ts">
  import type { Snippet } from 'svelte';

  let {
    title,
    subtitle,
    bars,
    max,
    tick,
    legend,
  }: {
    title: string;
    subtitle: string;
    bars: readonly Bar[];
    /** The axis top; columns are drawn from 0. */
    max: number;
    tick: (value: number) => string;
    legend?: Snippet;
  } = $props();

  const id = $props.id();
  let hover: Bar | undefined = $state();
  const ticks = $derived([max, max / 2, 0]);
</script>

<section class="chart" aria-labelledby="{id}-title">
  <div class="head">
    <h2 class="h2" id="{id}-title">{title}</h2>
    <p class="sub">{subtitle}</p>
  </div>
  <p class="readout" aria-hidden="true">{hover?.label ?? 'Hover for each run'}</p>
  <div class="plot">
    <ul class="axis" aria-hidden="true">
      {#each ticks as value, i (i)}
        <li>{tick(value)}</li>
      {/each}
    </ul>
    <div class="bars" class:empty={bars.length === 0}>
      {#each bars as bar (bar.key)}
        <div
          class="slot"
          role="presentation"
          onpointerenter={() => (hover = bar)}
          onpointerleave={() => (hover = undefined)}
        >
          <div
            class="bar"
            class:marked={bar.marked}
            role="img"
            aria-label={bar.label}
            style:height="{Math.max(0, Math.min(1, bar.value / max)) * 100}%"
          ></div>
        </div>
      {:else}
        <p>No runs in this range yet.</p>
      {/each}
    </div>
  </div>
  {@render legend?.()}
</section>

<style>
  .chart {
    display: grid;
    gap: 6px;
    min-width: 0;
  }
  .head {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 12px;
    align-items: baseline;
  }
  .sub,
  .readout {
    margin: 0;
    font-size: 13px;
    color: var(--ink2);
  }
  .readout {
    min-height: 1.4em;
    font-variant-numeric: tabular-nums;
  }
  .plot {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 8px;
    height: 140px;
  }
  .axis {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: 12px;
    color: var(--ink3);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .axis li {
    line-height: 1;
  }
  .bars {
    display: flex;
    gap: 2px;
    align-items: flex-end;
    border-bottom: 1px solid var(--line2);
    background: linear-gradient(var(--line) 1px, transparent 1px) 0 0 / 100% 50%;
  }
  .bars.empty {
    align-items: center;
    justify-content: center;
    color: var(--ink2);
  }
  .slot {
    display: flex;
    flex: 1 1 0;
    justify-content: center;
    align-items: flex-end;
    height: 100%;
  }
  .bar {
    width: 100%;
    max-width: 24px;
    min-height: 2px;
    border-radius: 4px 4px 0 0;
    background: var(--accent);
  }
  .bar.marked {
    background: var(--block);
  }
  .slot:hover .bar {
    opacity: 0.8;
  }
</style>
