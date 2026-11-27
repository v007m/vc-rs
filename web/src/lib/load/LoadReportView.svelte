<script lang="ts">
  import { clock } from './chart.ts';
  import type { LoadReport } from './port.ts';

  /**
   * A finished load run's report, as `vc load --json` gives it (D639, D678): the whole run's
   * figures, the thresholds it breached, each mix test, and the failed iterations by kind.
   */
  let { report }: { report: LoadReport } = $props();

  const uid = $props.id();
  const whole = (n: number): string => Math.round(n).toLocaleString('en-US');
  const share = (errors: number, of: number): string =>
    of > 0 ? `${+((errors / of) * 100).toFixed(2)} %` : '—';
  const figures = $derived<readonly (readonly [string, string])[]>([
    ['Length', clock(report.seconds)],
    ['Iterations', whole(report.iterations)],
    ['Requests a second', whole(report.rps)],
    ['Errors', share(report.errors, report.iterations)],
    ['p50', `${whole(report.latency.p50)} ms`],
    ['p95', `${whole(report.latency.p95)} ms`],
    ['p99', `${whole(report.latency.p99)} ms`],
    ['Max', `${whole(report.latency.max)} ms`],
  ]);
</script>

<div class="report">
  {#if report.blocked}
    <p class="blocked wide" role="alert">No user started: {report.blocked}</p>
  {/if}
  <section class="vc-panel wide" aria-labelledby="{uid}-sum">
    <h3 class="st-title3" id="{uid}-sum">Whole run</h3>
    <dl class="figures">
      {#each figures as [label, value] (label)}
        <div>
          <dt>{label}</dt>
          <dd>{value}</dd>
        </div>
      {/each}
    </dl>
    <p class="st-secondary">Seed {report.seed}, to replay it.</p>
  </section>
  <section class="vc-panel" aria-labelledby="{uid}-breaches">
    <h3 class="st-title3" id="{uid}-breaches">Thresholds</h3>
    {#if report.breaches.length}
      <ul class="rows">
        {#each report.breaches as b (`${b.test ?? ''} ${b.metric}`)}
          <li>
            <span class="st-badge vc-fail">✕ Breached</span>
            <span class="vc-mono">{b.test ? `${b.test}: ` : ''}{b.metric}</span>
            <span class="st-secondary">{b.shown}</span>
          </li>
        {/each}
      </ul>
    {:else}
      <p class="st-secondary"><span aria-hidden="true">✓</span> No threshold was breached.</p>
    {/if}
  </section>
  <section class="vc-panel" aria-labelledby="{uid}-errors">
    <h3 class="st-title3" id="{uid}-errors">Failed iterations</h3>
    {#if report.errorKinds.length}
      <table class="vc-table">
        <thead>
          <tr>
            <th scope="col">Where</th><th scope="col">Status</th><th scope="col">Code</th>
            <th scope="col" class="r">Count</th>
          </tr>
        </thead>
        <tbody>
          {#each report.errorKinds as k, i (i)}
            <tr>
              <td class="vc-mono">{k.api ?? k.test ?? '—'}</td>
              <td>{k.status ?? '—'}</td>
              <td class="vc-mono">{k.code ?? '—'}</td>
              <td class="r">{whole(k.count)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p class="st-secondary">No iteration failed.</p>
    {/if}
  </section>
  <section class="vc-panel wide" aria-labelledby="{uid}-mix">
    <h3 class="st-title3" id="{uid}-mix">Mix tests</h3>
    <table class="vc-table">
      <thead>
        <tr>
          <th scope="col">Test</th><th scope="col" class="r">Iterations</th>
          <th scope="col" class="r">rps</th><th scope="col" class="r">p50</th>
          <th scope="col" class="r">p95</th><th scope="col" class="r">p99</th>
          <th scope="col" class="r">Errors</th>
        </tr>
      </thead>
      <tbody>
        {#each report.tests as t (t.test)}
          <tr>
            <td class="vc-mono">{t.test}</td>
            <td class="r">{whole(t.iterations)}</td>
            <td class="r">{whole(t.rps)}</td>
            <td class="r">{whole(t.latency.p50)} ms</td>
            <td class="r">{whole(t.latency.p95)} ms</td>
            <td class="r">{whole(t.latency.p99)} ms</td>
            <td class="r">{share(t.errors, t.iterations)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </section>
  {#each report.notices as notice (notice)}
    <p class="st-secondary wide" role="note">{notice}</p>
  {/each}
</div>

<style>
  .report {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 16px 24px;
  }
  .report > section {
    display: grid;
    gap: 10px;
    align-content: start;
    overflow-x: auto;
  }
  .wide {
    grid-column: 1 / -1;
  }
  h3,
  p {
    margin: 0;
  }
  .figures {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
    gap: 10px 16px;
    margin: 0;
  }
  dt {
    font: var(--st-type-caption);
    color: var(--st-text-secondary);
  }
  dd {
    margin: 2px 0 0;
    font-size: var(--vc-size-callout);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .rows {
    display: grid;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .rows li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 10px;
    padding: 8px 0;
    border-top: 1px solid var(--st-chrome-separator);
  }
  .rows li:first-child {
    border-top: 0;
  }
  .blocked {
    color: var(--vc-fail-ink);
    font-weight: 600;
  }
  .r {
    text-align: right;
  }
</style>
