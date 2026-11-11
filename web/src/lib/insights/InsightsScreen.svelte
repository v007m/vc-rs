<script lang="ts">
  import { SvelteMap } from 'svelte/reactivity';
  import type { SkipPort } from '#lib/skip/port.ts';
  import SkipForm from '#lib/skip/SkipForm.svelte';
  import { SkipChange } from '#lib/skip/skip.svelte.ts';
  import BarChart, { type Bar } from './BarChart.svelte';
  import { windows, type Insights } from './insights.svelte.ts';
  import type { RunSummary } from './port.ts';
  import { duration, flakiest, mean, median, niceMax, passRate, when } from './stats.ts';

  let { insights, skips }: { insights: Insights; skips: SkipPort } = $props();

  /** The skip being written or removed, one at a time. */
  let skipping: SkipChange | undefined = $state();
  /** What this page skipped, by test ID, until it is undone. */
  const skipped = new SvelteMap<string, string>();

  function openSkip(id: string, removing = false): void {
    skipping =
      skipping?.target === id && !removing ? undefined : new SkipChange(skips, id, removing);
  }

  function written(change: SkipChange): void {
    if (change.removing) skipped.delete(change.target);
    else skipped.set(change.target, change.until ? `skipped until ${change.until}` : 'skipped');
    skipping = undefined;
  }

  const pct = (value: number | undefined) => (value === undefined ? '—' : `${Math.round(value)}%`);
  const rates = $derived(insights.runs.map(passRate).filter((r): r is number => r !== undefined));
  const timed = $derived(insights.runs.filter((r) => r.durationMs !== undefined));
  const durations = $derived(timed.map((r) => r.durationMs ?? 0));
  const midDuration = $derived(median(durations));
  const worst = $derived(flakiest(insights.trends));
  const flakyTests = $derived(insights.trends.filter((t) => t.flaky > 0).length);
  const recent = $derived([...insights.runs].reverse().slice(0, 10));

  function rateLabel(run: RunSummary): string {
    const rate = passRate(run);
    if (rate === undefined) return `Run #${run.id}: no tests ran`;
    const blocked = run.blocked > 0 ? ` · ${run.blocked} blocked` : '';
    return `Run #${run.id}: ${rate}% passed${blocked} · ${when(run.startedMs)}`;
  }
  const rateBars: Bar[] = $derived(
    insights.runs.map((run) => ({
      key: String(run.id),
      value: passRate(run) ?? 0,
      label: rateLabel(run),
      marked: run.blocked > 0,
    })),
  );
  const durationBars: Bar[] = $derived(
    timed.map((run) => ({
      key: String(run.id),
      value: run.durationMs ?? 0,
      label: `Run #${run.id} took ${duration(run.durationMs ?? 0)} · ${when(run.startedMs)}`,
    })),
  );
  const rangeLabel = $derived(`last ${insights.window} runs`);
</script>

<div class="insights">
  <div class="seg" role="group" aria-label="Time range">
    {#each windows as w (w)}
      <button
        type="button"
        aria-pressed={insights.window === w}
        onclick={() => void insights.choose(w)}>Last {w} runs</button
      >
    {/each}
  </div>
  <div class="filters">
    <div class="seg" role="group" aria-label="Profile">
      <button
        type="button"
        aria-pressed={insights.profile === undefined}
        onclick={() => void insights.scope('profile', undefined)}>All profiles</button
      >
      {#each insights.profiles as p (p)}
        <button
          type="button"
          aria-pressed={insights.profile === p}
          onclick={() => void insights.scope('profile', p)}>{p}</button
        >
      {/each}
    </div>
    {#if insights.branches.length}
      <div class="seg" role="group" aria-label="Branch">
        <button
          type="button"
          aria-pressed={insights.branch === undefined}
          onclick={() => void insights.scope('branch', undefined)}>All branches</button
        >
        {#each insights.branches as b (b)}
          <button
            type="button"
            class="mono"
            aria-pressed={insights.branch === b}
            onclick={() => void insights.scope('branch', b)}>{b}</button
          >
        {/each}
      </div>
    {/if}
  </div>
  {#if insights.error}
    <p class="error" role="alert">{insights.error}</p>
  {/if}

  <dl class="kpis" aria-busy={insights.loading}>
    <div>
      <dt>Last run pass rate</dt>
      <dd>{pct(rates.at(-1))}</dd>
    </div>
    <div>
      <dt>Average pass rate</dt>
      <dd>{pct(mean(rates))}</dd>
    </div>
    <div>
      <dt>Median run duration</dt>
      <dd>
        {midDuration === undefined ? '—' : duration(midDuration)}
      </dd>
    </div>
    <div>
      <dt>Runs</dt>
      <dd>{insights.runs.length}</dd>
    </div>
    <div>
      <dt>Flaky tests</dt>
      <dd>{flakyTests}</dd>
    </div>
  </dl>

  <div class="charts">
    <BarChart
      title="Pass rate per run"
      subtitle="{rangeLabel} · passed (flaky included) ÷ tests that ran"
      bars={rateBars}
      max={100}
      tick={(v) => `${v}%`}
    >
      {#snippet legend()}
        {#if rateBars.some((b) => b.marked)}
          <p class="legend"><span class="swatch" aria-hidden="true"></span>run had blocked tests</p>
        {/if}
      {/snippet}
    </BarChart>
    <BarChart
      title="Run duration"
      subtitle="{rangeLabel} · whole suite, wall clock"
      bars={durationBars}
      max={niceMax(Math.max(0, ...durations))}
      tick={(v) => (v === 0 ? '0' : duration(v))}
    />
  </div>

  <table>
    <caption class="h2">Last 10 runs in the range</caption>
    <thead>
      <tr>
        <th scope="col">Run</th><th scope="col">Date</th><th scope="col">Pass rate</th><th
          scope="col">Duration</th
        ><th scope="col">Blocked tests</th>
      </tr>
    </thead>
    <tbody>
      {#each recent as run (run.id)}
        <tr>
          <th scope="row">#{run.id}</th>
          <td>{when(run.startedMs)}</td>
          <td>{pct(passRate(run))}</td>
          <td>{run.durationMs === undefined ? '—' : duration(run.durationMs)}</td>
          <td>{run.blocked}</td>
        </tr>
      {:else}
        <tr><td colspan="5" class="muted">No runs yet. Start one from Tests or Runs.</td></tr>
      {/each}
    </tbody>
  </table>

  <section aria-labelledby="flakiest" class="flakiest">
    <h2 class="h2" id="flakiest">Flakiest tests</h2>
    <p class="muted">Flaky or failed results per test · {rangeLabel}</p>
    {#if worst.length === 0}
      <p class="muted">No flaky or failing tests in this range.</p>
    {:else}
      <ol>
        {#each worst as t (t.testId)}
          {@const done = skipped.get(t.testId)}
          <li>
            <span class="mono">{t.testId}</span>
            <span class="muted">{t.flaky} flaky · {t.failed} failed of {t.results} results</span>
            <span class="acts">
              {#if done}
                <span class="pill">{done}</span>
                <button
                  type="button"
                  class="btn sm"
                  aria-label="Undo the skip of {t.testId}"
                  onclick={() => openSkip(t.testId, true)}>Undo</button
                >
              {:else}
                <button
                  type="button"
                  class="btn sm"
                  aria-label="Skip {t.testId} until…"
                  aria-expanded={skipping?.target === t.testId}
                  onclick={() => openSkip(t.testId)}>Skip until…</button
                >
              {/if}
            </span>
            {#if skipping?.target === t.testId}
              {@const change = skipping}
              <SkipForm
                {change}
                oncancel={() => (skipping = undefined)}
                onwritten={() => written(change)}
              />
            {/if}
          </li>
        {/each}
      </ol>
      <p class="muted note">
        Skip writes <code>skip: &lbrace; reason, until &rbrace;</code> into the test: it is reported as
        skipped with its reason, and runs again after the date.
      </p>
    {/if}
  </section>
</div>

<style>
  .insights {
    display: grid;
    gap: 20px;
    max-width: 1100px;
  }
  .seg {
    justify-self: start;
  }
  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 16px;
  }
  .kpis {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 12px;
    margin: 0;
  }
  .kpis div {
    padding: 12px;
    border: 1px solid var(--line);
    border-radius: 10px;
    background: var(--panel2);
  }
  dt {
    font-size: 13px;
    color: var(--ink2);
  }
  dd {
    margin: 4px 0 0;
    font-size: 22px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .charts {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 24px;
  }
  .legend {
    display: flex;
    gap: 6px;
    align-items: center;
    margin: 0;
    font-size: 13px;
    color: var(--ink2);
  }
  .swatch {
    width: 10px;
    height: 10px;
    border-radius: 2px;
    background: var(--block);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-variant-numeric: tabular-nums;
  }
  caption {
    text-align: left;
    margin-bottom: 8px;
  }
  th,
  td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--line);
    text-align: left;
  }
  thead th {
    color: var(--ink2);
    font-weight: 500;
  }
  tbody th {
    font-weight: 400;
  }
  .flakiest ol {
    display: grid;
    gap: 6px;
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
  }
  .flakiest li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 12px;
  }
  .acts {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-left: auto;
  }
  .note {
    font-size: 12px;
  }
  .flakiest p {
    margin: 4px 0 0;
  }
  .muted {
    color: var(--ink2);
  }
  .error {
    margin: 0;
    color: var(--fail);
  }
</style>
