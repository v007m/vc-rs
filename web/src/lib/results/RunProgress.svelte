<script lang="ts">
  import { doneCount, type Counts } from './status.ts';

  /** `planned` is how many tests the run will have; the bar fills as `counts` reach it. */
  let { counts, planned }: { counts: Counts; planned: number } = $props();

  const outcomes = [
    { key: 'passed', bar: 's-pass', dot: 'vc-pass' },
    { key: 'failed', bar: 's-fail', dot: 'vc-fail' },
    { key: 'blocked', bar: 's-block', dot: 'vc-blocked' },
    { key: 'flaky', bar: 's-flaky', dot: 'vc-flaky' },
    { key: 'skipped', bar: 's-skip', dot: 'vc-skipped' },
  ] as const satisfies readonly { key: keyof Counts; bar: string; dot: string }[];

  let done = $derived(doneCount(counts));
  let total = $derived(Math.max(planned, done));
  let pct = $derived(total ? Math.round((done / total) * 100) : 0);
  let barLabel = $derived(
    `${outcomes.map((o) => `${counts[o.key]} ${o.key}`).join(', ')}, of ${total} tests`,
  );
  const width = (n: number): string => `${total ? (n / total) * 100 : 0}%`;
</script>

<div class="progress">
  <div class="pbar" role="img" aria-label={barLabel}>
    {#each outcomes as o (o.key)}
      <span class={o.bar} style:width={width(counts[o.key])}></span>
    {/each}
  </div>
  <p class="st-secondary"><b>{done}</b> of {total} tests done · {pct}%</p>
</div>
<dl class="stats">
  {#each outcomes as o (o.key)}
    <div class="stat">
      <dt class="l">{o.key}</dt>
      <dd class="v"><span class="st-dot {o.dot}"></span>{counts[o.key]}</dd>
    </div>
  {/each}
</dl>

<style>
  .progress {
    padding: 16px 20px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .progress p {
    margin: 0;
  }
  .pbar {
    display: flex;
    height: 12px;
    border-radius: 6px;
    overflow: hidden;
    background: var(--st-surface-input-on-card);
  }
  .pbar span {
    height: 12px;
    transition: width 0.6s ease;
  }
  .s-pass {
    background: var(--vc-pass);
  }
  .s-fail {
    background: var(--vc-fail);
  }
  .s-block {
    background: var(--vc-blocked);
  }
  .s-flaky {
    background: var(--vc-flaky);
  }
  .s-skip {
    background: var(--vc-skipped);
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(112px, 1fr));
    margin: 0;
    border-top: 1px solid var(--st-chrome-separator);
  }
  .stat {
    padding: 12px 16px;
    display: flex;
    flex-direction: column-reverse;
    gap: 3px;
    border-right: 1px solid var(--st-chrome-separator);
    min-width: 0;
  }
  .stat:last-child {
    border-right: 0;
  }
  .v {
    margin: 0;
    font: 600 20px var(--st-font-sans);
    font-variant-numeric: tabular-nums;
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .l {
    font-size: 12px;
    color: var(--st-text-secondary);
  }
</style>
