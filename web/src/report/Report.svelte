<script lang="ts">
  import CheckList from '#lib/results/CheckList.svelte';
  import ExchangeView from '#lib/results/ExchangeView.svelte';
  import RunProgress from '#lib/results/RunProgress.svelte';
  import { ms, statusClass, statuses } from '#lib/results/status.ts';
  import TestList from '#lib/results/TestList.svelte';
  import { counts } from './data.ts';
  import type { Report } from './report.svelte.ts';

  let { report }: { report: Report } = $props();
  const uid = $props.id();
  const totals = $derived(counts(report.summary?.totals ?? {}));
  const meta = $derived.by(() => {
    const run = report.summary?.run;
    if (!run) return '';
    const parts = [new Date(run.startedMs).toLocaleString(), `${ms(run.durationMs)} ms`];
    if (run.profiles.length) parts.push(`profiles: ${run.profiles.join(', ')}`);
    if (run.seed !== null) parts.push(`seed ${run.seed}`);
    return parts.join(' · ');
  });

  // The budget marks the report spec reads: first paint, then select-to-detail per test.
  $effect(() => {
    if (report.summary)
      requestAnimationFrame(() => setTimeout(() => performance.mark('vib:painted')));
  });
  $effect(() => {
    if (report.opened) performance.measure('vib:open', 'vib:select');
  });
</script>

{#if report.summary}
  {@const run = report.summary.run}
  <header class="top">
    <h1>Run #{run.id}</h1>
    <span class="pill {statusClass[run.status as keyof typeof statusClass] ?? ''}">{run.status}</span>
    <p class="muted">{meta}</p>
  </header>
  <section class="card" aria-label="Totals">
    <RunProgress counts={totals} planned={0} />
  </section>
  {#if report.summary.lifecycle.length}
    <section class="card lifecycle" aria-labelledby="{uid}-life">
      <h2 class="h2" id="{uid}-life">Lifecycle and cleanups</h2>
      <ul>
        {#each report.summary.lifecycle as line, i (i)}
          <li><b>{line.status}</b> {line.label}{line.message ? `: ${line.message}` : ''}</li>
        {/each}
      </ul>
    </section>
  {/if}
  <main class="layout">
    <section class="card list" aria-label="Test list">
      <div class="filters">
        <input
          class="input"
          type="search"
          placeholder="Filter by ID, name, file or tag"
          aria-label="Filter tests"
          bind:value={report.text}
        />
        <div class="seg" role="group" aria-label="Status">
          {#each statuses as status (status)}
            <button
              type="button"
              aria-pressed={report.only.has(status)}
              onclick={() => report.toggle(status)}>{status}</button
            >
          {/each}
        </div>
      </div>
      <TestList
        label="Tests"
        rows={report.shown}
        current={report.current}
        onopen={(key) => void report.open(key)}
      />
      <p class="count muted" aria-live="polite">
        {report.shown.length} of {report.rows.length} tests
      </p>
    </section>
    {#if report.opened}
      {@const { row, test, checks, exchanges } = report.opened}
      <section class="card detail" aria-labelledby="{uid}-test">
        <h2 class="h2" id="{uid}-test">{row.label}</h2>
        <p class="muted mono">{row.key} · {row.status} · {ms(row.ms)} ms</p>
        {#if test.location}<p class="muted mono">{test.location}</p>{/if}
        {#if row.tags.length}<p class="muted">tags: {row.tags.join(', ')}</p>{/if}
        {#if test.blockedBy}<p>blocked by {test.blockedBy}</p>{/if}
        {#if test.skipReason}<p>skipped: {test.skipReason}</p>{/if}
        {#if test.message}<pre class="message mono">{test.message}</pre>{/if}
        {#if checks.length}
          <h3>Checks</h3>
          <CheckList {checks} />
        {/if}
        {#if exchanges.length}
          <h3>Exchanges</h3>
          {#each exchanges as exchange, i (i)}<ExchangeView {exchange} />{/each}
        {/if}
      </section>
    {:else}
      <section class="card detail empty">
        <p class="muted">Select a test to see its checks and exchanges.</p>
      </section>
    {/if}
  </main>
{:else if report.error}
  <p class="card failed" role="alert">This report could not be read: {report.error}</p>
{:else}
  <p class="muted">Loading the report…</p>
{/if}
{#if report.summary && report.error}
  <p class="card failed" role="alert">{report.error}</p>
{/if}
