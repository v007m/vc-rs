<script lang="ts">
  import { doneCount, type Counts } from './status.ts';

  /** `planned` is how many tests the run will have; the bar fills as `counts` reach it. */
  let { counts, planned }: { counts: Counts; planned: number } = $props();

  const outcomes = [
    { key: 'passed', bar: 's-pass', dot: 'pass' },
    { key: 'failed', bar: 's-fail', dot: 'fail' },
    { key: 'blocked', bar: 's-block', dot: 'block' },
    { key: 'flaky', bar: 's-flaky', dot: 'flaky' },
    { key: 'skipped', bar: 's-skip', dot: 'skip' },
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
  <p class="muted"><b>{done}</b> of {total} tests done · {pct}%</p>
</div>
<dl class="stats">
  {#each outcomes as o (o.key)}
    <div class="stat">
      <dt class="l">{o.key}</dt>
      <dd class="v"><span class="dot {o.dot}"></span>{counts[o.key]}</dd>
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
    background: var(--panel3);
  }
  .pbar span {
    height: 12px;
    transition: width 0.6s ease;
  }
  .s-pass {
    background: var(--pass);
  }
  .s-fail {
    background: var(--fail);
  }
  .s-block {
    background: #c27a00;
  }
  .s-flaky {
    background: var(--flaky);
  }
  .s-skip {
    background: #a9a5ba;
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(112px, 1fr));
    margin: 0;
    border-top: 1px solid var(--line);
  }
  .stat {
    padding: 12px 16px;
    display: flex;
    flex-direction: column-reverse;
    gap: 3px;
    border-right: 1px solid var(--line);
    min-width: 0;
  }
  .stat:last-child {
    border-right: 0;
  }
  .v {
    margin: 0;
    font: 600 20px var(--font);
    font-variant-numeric: tabular-nums;
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .l {
    font-size: 12px;
    color: var(--ink2);
  }
</style>
