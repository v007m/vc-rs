<script lang="ts">
  import CheckList from '#lib/results/CheckList.svelte';
  import ExchangeView from '#lib/results/ExchangeView.svelte';
  import RunProgress from '#lib/results/RunProgress.svelte';
  import { ms } from '#lib/results/status.ts';
  import TestList from '#lib/results/TestList.svelte';
  import type { RunSession } from '#lib/run/session.svelte.ts';
  import type { RunHistory } from './history.svelte.ts';
  import { outcomeClass, outcomeLabel } from './outcome.ts';
  import type { StoredRun } from './port.ts';

  /**
   * A run from the history (`GetRun`): who ran what, its totals, and each test's checks and
   * exchanges, already masked by the engine. Its failed tests can be run again.
   */
  let {
    stored,
    history,
    session,
  }: { stored: StoredRun; history: RunHistory; session: RunSession } = $props();

  const uid = $props.id();
  const run = $derived(stored.run);
  const rows = $derived(stored.tests.map((test) => test.row));
  const failed = $derived(history.failedIds);
  const meta = $derived.by(() => {
    const parts = [`Selector ${run.selector || 'all tests'}`];
    if (run.profiles.length) parts.unshift(`Profile ${run.profiles.join(', ')}`);
    parts.push(`started ${new Date(run.startedMs).toLocaleString()}`);
    if (run.durationMs !== undefined) parts.push(`${ms(run.durationMs)} ms`);
    if (run.branch || run.commit) {
      parts.push([run.branch, run.commit?.slice(0, 7)].filter(Boolean).join(' @ '));
    }
    return parts.join(' · ');
  });
</script>

<div class="head">
  <h2 class="h2">Run #{run.id}</h2>
  <span class="pill {outcomeClass[run.outcome]}">{outcomeLabel[run.outcome]}</span>
  <span class="grow"></span>
  {#if failed.length}
    <button
      type="button"
      class="btn"
      disabled={session.running}
      onclick={() => {
        const ids = failed;
        history.close();
        void session.start({ selection: { ids } });
      }}>Re-run failed</button
    >
  {/if}
  <button type="button" class="btn" onclick={() => history.close()}>Back to the current run</button>
</div>
<p class="muted meta">{meta}</p>
<RunProgress counts={run.counts} planned={0} />
{#if rows.length}
  <div class="layout">
    <section class="list" aria-label="Tests in run #{run.id}">
      <TestList
        label="Tests"
        {rows}
        current={history.current}
        onopen={(key) => (history.current = key)}
      />
    </section>
    {#if history.opened}
      {@const test = history.opened}
      <section class="detail" aria-labelledby="{uid}-test">
        <h3 class="h3" id="{uid}-test">{test.row.label}</h3>
        <p class="muted mono">
          {test.id} · {test.row.status} · {ms(test.row.ms)} ms{test.attempts > 1
            ? ` · ${test.attempts} attempts`
            : ''}
        </p>
        {#if test.blockedBy}<p>Blocked by {test.blockedBy}</p>{/if}
        {#if test.skipReason}<p>Skipped: {test.skipReason}</p>{/if}
        {#if test.failure}<pre class="message mono">{test.failure}</pre>{/if}
        {#if test.checks.length}
          <h4>Checks</h4>
          <CheckList checks={test.checks} />
        {/if}
        {#if test.exchanges.length}
          <h4>Exchanges</h4>
          {#each test.exchanges as exchange, i (i)}<ExchangeView {exchange} />{/each}
        {/if}
      </section>
    {/if}
  </div>
{:else}
  <p class="muted">This run kept no test results.</p>
{/if}

<style>
  .head {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
  }
  .meta {
    margin: 0;
  }
  .layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    gap: 16px;
    min-height: 360px;
  }
  .list {
    display: grid;
    min-height: 0;
  }
  .detail {
    min-width: 0;
  }
  .h3 {
    margin: 0;
    font-size: 14px;
    font-weight: 600;
  }
  h4 {
    margin: 12px 0 4px;
    font-size: 13px;
    font-weight: 600;
  }
  .message {
    white-space: pre-wrap;
    color: var(--fail);
  }
  @media (width < 720px) {
    .layout {
      grid-template-columns: 1fr;
    }
  }
</style>
