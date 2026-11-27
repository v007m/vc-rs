<script lang="ts">
  import { lens } from '#lib/ui/segmented.ts';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import HostsPanel from './HostsPanel.svelte';
  import LineChart from './LineChart.svelte';
  import LoadReportView from './LoadReportView.svelte';
  import type { LoadSession } from './session.svelte.ts';
  import { charts, kpis, phaseClass, phaseLabel, thresholdFor } from './view.ts';

  /**
   * A load run (D639), live or stored: its header with Stop or Run again, the KPIs, the three
   * charts with their thresholds and the Hosts panel; **Report** once it has finished.
   */
  let { session }: { session: LoadSession } = $props();

  const uid = $props.id();
  let view = $state<'live' | 'report'>('live');
  let table = $state(false);
  let picked = $state('');
  const run = $derived(session.run);
  const shown = $derived(run?.report ? view : 'live');
  /** The x axis's end: the planned length, or the ticks so far when it is not known. */
  const tMax = $derived(run ? Math.max(run.plannedS, run.ticks.at(-1)?.t ?? 0, 10) : 0);
  const scenario = $derived(picked || session.scenarios[0] || '');
</script>

{#if !run}
  <EmptyState
    icon="runs"
    title="No load run is open"
    caption={session.note ??
      (session.scenarios.length
        ? 'Start a load scenario, or open a load run from All runs.'
        : 'Open a load run from All runs; it shows here.')}
  >
    {#snippet action()}
      {#if session.scenarios.length}
        <div class="start">
          <label class="vc-sr" for="{uid}-scenario">Load scenario</label>
          <select id="{uid}-scenario" class="st-input vc-mono" bind:value={picked}>
            {#each session.scenarios as id (id)}<option value={id}>{id}</option>{/each}
          </select>
          <button
            type="button"
            class="st-btn st-btn--primary"
            disabled={!scenario}
            onclick={() => void session.start(scenario)}>Start</button
          >
        </div>
      {/if}
    {/snippet}
  </EmptyState>
{:else}
  <header class="head">
    <h2 class="st-title2 vc-mono">{run.file}</h2>
    <span class="st-badge {phaseClass[run.phase]}">{phaseLabel[run.phase]}</span>
    <span class="st-secondary">{run.profile || 'default'}{run.id ? ` · #${run.id}` : ''}</span>
    <span class="vc-grow"></span>
    {#if run.report}
      <div class="st-segmented" role="group" aria-label="Show" {@attach lens}>
        <button type="button" aria-pressed={shown === 'live'} onclick={() => (view = 'live')}
          >Charts</button
        >
        <button type="button" aria-pressed={shown === 'report'} onclick={() => (view = 'report')}
          >Report</button
        >
      </div>
    {/if}
    {#if shown === 'live'}
      <label class="tbl"
        ><input type="checkbox" class="st-switch" bind:checked={table} /> Table</label
      >
    {/if}
    {#if session.running}
      <button type="button" class="st-btn vc-btn--destructive" onclick={() => session.stop()}
        >Stop</button
      >
    {:else}
      <button type="button" class="st-btn" onclick={() => void session.start(run.file)}
        >Run again</button
      >
    {/if}
  </header>
  {#if session.note}<p class="st-secondary" role="status">{session.note}</p>{/if}
  {#if !session.running && !run.ticks.length && !run.report}
    <p class="st-secondary" role="note">
      The history keeps a load run's verdict, not its ticks. Run it again to watch one.
    </p>
  {/if}
  {#if shown === 'report' && run.report}
    <LoadReportView report={run.report} />
  {:else}
    <dl class="kpis">
      {#each kpis(run) as [label, value] (label)}
        <div class="st-card">
          <dt>{label}</dt>
          <dd>{value}</dd>
        </div>
      {/each}
    </dl>
    <div class="grid">
      {#each charts as chart (chart.title)}
        <LineChart
          {...chart}
          ticks={run.ticks}
          {tMax}
          threshold={thresholdFor(run, chart)}
          {table}
        />
      {/each}
      <HostsPanel host={run.ticks.at(-1)?.host} />
    </div>
  {/if}
{/if}

<style>
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
  }
  h2 {
    margin: 0;
  }
  .start {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .start select {
    width: auto;
    min-width: 200px;
  }
  .tbl {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .kpis {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 10px;
    margin: 0;
  }
  .kpis > div {
    padding: 10px 14px;
  }
  dt {
    font: var(--st-type-caption);
    color: var(--st-text-tertiary);
  }
  dd {
    margin: 2px 0 0;
    font-size: var(--vc-size-large-title);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(420px, 1fr));
    gap: 12px;
  }
  @media (max-width: 1100px) {
    .grid {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
