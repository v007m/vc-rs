<script lang="ts">
  import { clock } from './chart.ts';
  import type { LoadReport } from './port.ts';

  /**
   * A finished load run's report (D639): each threshold's verdict, the latency histogram, the
   * scenario split, and p99 against earlier runs of the same file.
   */
  let { report }: { report: LoadReport } = $props();

  const uid = $props.id();
  const most = $derived(Math.max(1, ...report.histogram.map(([, n]) => n)));
  const slowest = $derived(Math.max(1, ...report.previous.map((p) => p.p99)));
  const num = (v: number): string => String(+v.toFixed(2));
</script>

<div class="report">
  <section class="st-card wide" aria-labelledby="{uid}-verdicts">
    <h3 class="st-title3" id="{uid}-verdicts">Thresholds</h3>
    {#if report.verdicts.length}
      <table class="vc-table">
        <thead>
          <tr>
            <th scope="col">Threshold</th><th scope="col">Scope</th><th scope="col">Verdict</th>
            <th scope="col" class="r">Worst</th><th scope="col" class="r">At</th>
            <th scope="col" class="r">Whole run</th>
          </tr>
        </thead>
        <tbody>
          {#each report.verdicts as v (`${v.threshold.label} ${v.scope}`)}
            <tr>
              <td class="vc-mono">{v.threshold.label}</td>
              <td>{v.scope}</td>
              <td>
                <span class="st-badge {v.held ? 'vc-pass' : 'vc-fail'}"
                  >{v.held ? '✓ Held' : '✕ Breached'}</span
                >
              </td>
              <td class="r">{num(v.worst)}</td>
              <td class="r vc-mono">{clock(v.at)}</td>
              <td class="r">{num(v.whole)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p class="st-secondary">This load file sets no thresholds.</p>
    {/if}
  </section>
  <section class="st-card" aria-labelledby="{uid}-hist">
    <h3 class="st-title3" id="{uid}-hist">Latency histogram</h3>
    <ul class="bars">
      {#each report.histogram as [upper, n] (upper)}
        <li>
          <span class="lbl vc-mono">≤ {upper} ms</span>
          <span class="bar"><i style:width="{(n / most) * 100}%"></i></span>
          <span class="n">{n.toLocaleString('en-US')}</span>
        </li>
      {/each}
    </ul>
  </section>
  <section class="st-card" aria-labelledby="{uid}-prev">
    <h3 class="st-title3" id="{uid}-prev">p99 against earlier runs</h3>
    <ul class="bars">
      {#each report.previous as p, i (p.run)}
        <li class:this={i === 0}>
          <span class="lbl vc-mono">#{p.run}{i === 0 ? ' (this)' : ''}</span>
          <span class="bar"><i style:width="{(p.p99 / slowest) * 100}%"></i></span>
          <span class="n">{Math.round(p.p99)} ms</span>
        </li>
      {/each}
    </ul>
  </section>
  <section class="st-card wide" aria-labelledby="{uid}-mix">
    <h3 class="st-title3" id="{uid}-mix">Scenarios</h3>
    <table class="vc-table">
      <thead>
        <tr>
          <th scope="col">Test</th><th scope="col" class="r">Weight</th>
          <th scope="col" class="r">Iterations</th><th scope="col" class="r">p50</th>
          <th scope="col" class="r">p95</th><th scope="col" class="r">p99</th>
          <th scope="col" class="r">Errors</th><th scope="col">Verdict</th>
        </tr>
      </thead>
      <tbody>
        {#each report.scenarios as s (s.test)}
          <tr>
            <td class="vc-mono">{s.test}</td>
            <td class="r">{s.weight}</td>
            <td class="r">{s.iterations.toLocaleString('en-US')}</td>
            <td class="r">{Math.round(s.p50)} ms</td>
            <td class="r">{Math.round(s.p95)} ms</td>
            <td class="r">{Math.round(s.p99)} ms</td>
            <td class="r">{num(s.errorRate)} %</td>
            <td>
              <span class="st-badge {s.held ? 'vc-pass' : 'vc-fail'}"
                >{s.held ? '✓ Held' : '✕ Breached'}</span
              >
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </section>
</div>

<style>
  .report {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 12px;
  }
  .report > section {
    display: grid;
    gap: 10px;
    align-content: start;
    min-width: 0;
    padding: 14px 16px;
    overflow-x: auto;
  }
  .wide {
    grid-column: 1 / -1;
  }
  h3 {
    margin: 0;
  }
  .bars {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .bars li {
    display: grid;
    grid-template-columns: 96px minmax(0, 1fr) 72px;
    align-items: center;
    gap: 10px;
    font-size: var(--vc-size-caption);
  }
  .bar {
    height: 8px;
    border-radius: 4px;
    background: var(--st-surface-wash);
    overflow: hidden;
  }
  .bar i {
    display: block;
    height: 100%;
    background: var(--vc-info);
  }
  .this .bar i {
    background: var(--st-accent);
  }
  .this {
    font-weight: 600;
  }
  .n {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .r {
    text-align: right;
  }
</style>
