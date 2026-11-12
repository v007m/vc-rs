<script lang="ts">
  import MethodTag from './MethodTag.svelte';
  import type { ApiTrend } from './port.ts';
  import { latency, slowest } from './stats.ts';

  /** The apis slowest at p95 over the range, each a bar of its p50 inside its p95. */
  let { trends, rangeLabel }: { trends: readonly ApiTrend[]; rangeLabel: string } = $props();

  const rows = $derived(slowest(trends));
  const widest = $derived(Math.max(1, ...rows.map((t) => t.p95Ms ?? 0)));
  const width = (ms: number | undefined) => `${((ms ?? 0) / widest) * 100}%`;
  const times = (t: ApiTrend) => `p50 ${latency(t.p50Ms ?? 0)}, p95 ${latency(t.p95Ms ?? 0)}`;
  const calls = (n: number) => `${n.toLocaleString('en-US')} ${n === 1 ? 'call' : 'calls'}`;
</script>

<section aria-labelledby="slowest" class="slowest">
  <h2 class="h2" id="slowest">Slowest APIs</h2>
  <p class="muted">Response time across all tests · {rangeLabel}</p>
  {#if rows.length === 0}
    <p class="muted">No api answered in this range yet.</p>
  {:else}
    <p class="legend" aria-hidden="true">
      <span><span class="sw p50"></span>p50</span><span><span class="sw p95"></span>p95</span>
      <span class="end">p50 / p95 · calls</span>
    </p>
    <ol>
      {#each rows as t (t.apiId)}
        <li>
          <span class="id mono">
            <MethodTag method={t.method} />
            <span class="name" title={t.apiId}>{t.apiId}</span>
          </span>
          <span class="bar" role="img" aria-label="{t.apiId}: {times(t)}">
            <span class="p95" style:width={width(t.p95Ms)}></span>
            <span class="p50" style:width={width(t.p50Ms)}></span>
          </span>
          <span class="n mono"
            >{latency(t.p50Ms ?? 0)} / {latency(t.p95Ms ?? 0)}<br /><span class="muted small"
              >{calls(t.calls)}</span
            ></span
          >
        </li>
      {/each}
    </ol>
  {/if}
</section>

<style>
  .slowest p {
    margin: 4px 0 0;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 14px;
    font-size: 12px;
    color: var(--ink2);
  }
  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .legend .end {
    margin-left: auto;
  }
  .sw {
    width: 18px;
    height: 8px;
    border-radius: 3px;
  }
  ol {
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: grid;
    grid-template-columns: minmax(150px, 1.4fr) minmax(70px, 1.6fr) auto;
    gap: 10px 14px;
    align-items: center;
    padding: 7px 0;
    border-bottom: 1px solid var(--line);
    font-size: 12px;
  }
  li:last-child {
    border-bottom: 0;
  }
  .id {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    white-space: nowrap;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .bar {
    position: relative;
    height: 10px;
    border-radius: 5px;
    background: var(--panel3);
  }
  .bar span {
    position: absolute;
    top: 0;
    left: 0;
    height: 10px;
    border-radius: 5px;
  }
  .p95 {
    background: var(--accentMid);
  }
  .p50 {
    background: var(--accent);
  }
  .n {
    text-align: right;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .small {
    font-size: 11px;
  }
  .muted {
    color: var(--ink2);
  }
  @media (max-width: 560px) {
    li {
      grid-template-columns: minmax(0, 1fr) auto;
    }
    .bar {
      grid-column: 1 / -1;
      grid-row: 2;
    }
  }
</style>
