<script lang="ts">
  import CheckList from '#lib/results/CheckList.svelte';
  import ExchangeView from '#lib/results/ExchangeView.svelte';
  import { testTime } from '#lib/results/status.ts';
  import TestList from '#lib/results/TestList.svelte';
  import Tabs from '#lib/ui/Tabs.svelte';
  import { lens } from '#lib/ui/segmented.ts';
  import HistoryStrip from './HistoryStrip.svelte';
  import type { TestDetail } from './port.ts';
  import type { TestHistoryPort } from './seams.ts';

  /**
   * A run's tests and the one open (D634): the same view for a live run, as its tests finish,
   * and for a stored one. The open test shows why it did not pass, its checks and exchanges, and
   * its history strip.
   */
  let {
    label,
    tests,
    current = $bindable(),
    history,
  }: {
    label: string;
    tests: readonly TestDetail[];
    /** The key of the open test; the first that did not pass when unset. */
    current?: string | undefined;
    history: TestHistoryPort;
  } = $props();

  const uid = $props.id();
  let only = $state(false);
  let tab = $state('checks');
  const bad = $derived(
    tests.filter(({ row }) => row.status !== 'passed' && row.status !== 'skipped'),
  );
  const rows = $derived((only ? bad : tests).map((test) => test.row));
  const open = $derived(tests.find(({ row }) => row.key === current) ?? bad[0] ?? tests[0]);
</script>

{#if tests.length}
  <div class="detail-grid">
    <section class="list" aria-label={label}>
      <div class="st-segmented" role="group" aria-label="Show" {@attach lens}>
        <button type="button" aria-pressed={!only} onclick={() => (only = false)}
          >All {tests.length}</button
        >
        <button type="button" aria-pressed={only} onclick={() => (only = true)}
          >Not passed {bad.length}</button
        >
      </div>
      <TestList {label} {rows} current={open?.row.key} onopen={(key) => (current = key)} />
    </section>
    {#if open}
      <section class="test" aria-labelledby="{uid}-t">
        <h3 class="st-title3" id="{uid}-t">{open.row.label}</h3>
        <p class="st-secondary vc-mono">
          {open.id} · {open.row.status} · {testTime(open.row.status, open.row.ms)}{open.attempts > 1
            ? ` · ${open.attempts} attempts`
            : ''}
        </p>
        {#if open.blockedBy}<p>Blocked by <span class="vc-mono">{open.blockedBy}</span></p>{/if}
        {#if open.skipReason}<p>Skipped: {open.skipReason}</p>{/if}
        {#if open.failure}<pre class="message vc-mono">{open.failure}</pre>{/if}
        <Tabs
          label="Test result"
          tabs={[
            {
              id: 'checks',
              label: 'Checks',
              count: String(open.checks.length),
              bad: open.checks.some((c) => !c.passed),
            },
            { id: 'exchanges', label: 'Exchanges', count: String(open.exchanges.length) },
          ]}
          bind:selected={tab}
        >
          {#snippet panel(id)}
            {#if id === 'checks'}
              {#if open.checks.length}<CheckList checks={open.checks} />{:else}<p
                  class="st-secondary"
                >
                  No checks ran.
                </p>{/if}
            {:else}
              {#each open.exchanges as exchange, i (i)}<ExchangeView {exchange} />{:else}<p
                  class="st-secondary"
                >
                  No requests were sent.
                </p>{/each}
            {/if}
          {/snippet}
        </Tabs>
        <HistoryStrip id={open.id} port={history} />
      </section>
    {/if}
  </div>
{:else}
  <p class="st-secondary">No test has finished yet.</p>
{/if}

<style>
  .detail-grid {
    display: grid;
    grid-template-columns: minmax(240px, 2fr) minmax(0, 3fr);
    gap: 20px;
    min-height: 360px;
  }
  .list {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    gap: 8px;
    min-height: 0;
  }
  .st-segmented {
    justify-self: start;
  }
  .test {
    display: grid;
    align-content: start;
    gap: 10px;
    min-width: 0;
  }
  h3,
  p {
    margin: 0;
  }
  .message {
    margin: 0;
    padding: 8px 10px;
    border-radius: var(--vc-radius-inner);
    background: var(--vc-fail-soft);
    color: var(--vc-fail-ink);
    white-space: pre-wrap;
  }
  @container card (width < 720px) {
    .detail-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
