<script lang="ts">
  import EventLog from './EventLog.svelte';
  import PlanTable from './PlanTable.svelte';
  import RunProgress from '#lib/results/RunProgress.svelte';
  import type { RunPhase } from './model.ts';
  import type { RunSession } from './session.svelte.ts';

  let { session }: { session: RunSession } = $props();

  const pills: Record<RunPhase, [string, string]> = {
    idle: ['', 'Not started'],
    running: ['accent', 'Running'],
    passed: ['pass', 'Passed'],
    failed: ['fail', 'Failed'],
    cancelled: ['block', 'Cancelled'],
    error: ['fail', 'Stopped by an error'],
  };
  let view = $derived(session.view);
  let pill = $derived(pills[view.phase]);
  let title = $derived(view.runId === undefined ? 'Current run' : `Run #${view.runId}`);
  let canRerun = $derived(view.phase === 'failed' && view.counts.failed > 0);
  let started = $derived(
    view.startedMs === undefined ? undefined : new Date(Number(view.startedMs)),
  );
</script>

{#if view.phase === 'idle'}
  <div class="empty">
    <p class="muted">No run yet. Start one from the Tests screen, or run every test here.</p>
    <button type="button" class="btn primary" onclick={() => session.start({})}>
      Run all tests
    </button>
  </div>
{:else}
  <div class="head">
    <h2 class="h2">{title}</h2>
    <span class="pill {pill[0]}" role="status">{pill[1]}</span>
    {#if started}
      <span class="muted">
        Started <time datetime={started.toISOString()}>{started.toLocaleTimeString()}</time>
      </span>
    {/if}
    {#if view.error}<span class="muted">{view.error}</span>{/if}
    <span class="grow"></span>
    {#if session.running}
      <button type="button" class="btn" onclick={() => session.stop()}>Stop and tear down</button>
    {:else}
      {#if canRerun}
        <button type="button" class="btn" onclick={() => session.rerunFailed()}
          >Re-run failed</button
        >
      {/if}
      <button
        type="button"
        class="btn"
        onclick={() => session.request && session.start(session.request)}
      >
        Run again
      </button>
    {/if}
  </div>
  <RunProgress counts={view.counts} planned={view.planned} />
  {#if view.plan.length}<PlanTable plan={view.plan} />{/if}
  <EventLog events={view.events} dropped={view.dropped} />
{/if}

<style>
  .empty {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 12px;
  }
  .empty p {
    margin: 0;
  }
  .head {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
    padding: 0 20px;
  }
</style>
