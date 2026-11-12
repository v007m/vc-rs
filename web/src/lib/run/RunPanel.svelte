<script lang="ts">
  import EventLog from './EventLog.svelte';
  import PlanTable from './PlanTable.svelte';
  import RunProgress from '#lib/results/RunProgress.svelte';
  import type { RunPhase } from './model.ts';
  import type { RunSession } from './session.svelte.ts';

  /**
   * The live run. Once it ends, **Open report** opens its record from the history through
   * `onopen`, and the HTML report it wrote is named as text to copy; the file is never served
   * (D320).
   */
  let { session, onopen }: { session: RunSession; onopen?: (id: number) => void } = $props();

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
  let copied = $state(false);

  async function copy(path: string): Promise<void> {
    await navigator.clipboard.writeText(path);
    copied = true;
  }
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
      {#if onopen && view.runId !== undefined}
        {@const id = Number(view.runId)}
        <button type="button" class="btn" onclick={() => onopen(id)}>Open report</button>
      {/if}
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
  {#if !session.running && view.htmlReport}
    {@const path = view.htmlReport}
    <p class="file">
      <span class="muted">HTML report</span>
      <code class="mono path">{path}</code>
      <button
        type="button"
        class="btn"
        onclick={() => void copy(path)}
        onblur={() => (copied = false)}>{copied ? 'Copied' : 'Copy path'}</button
      >
    </p>
  {/if}
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
  .head,
  .file {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 10px;
    padding: 0 20px;
  }
  .file {
    margin: 0;
  }
  .path {
    min-width: 0;
    overflow-wrap: anywhere;
    user-select: all;
  }
</style>
