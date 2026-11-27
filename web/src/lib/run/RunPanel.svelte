<script lang="ts">
  import Glyph from '#lib/ui/Glyph.svelte';
  import EventLog from './EventLog.svelte';
  import PlanTable from './PlanTable.svelte';
  import RunProgress from '#lib/results/RunProgress.svelte';
  import StopControls from './StopControls.svelte';
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
    running: ['vc-accent', 'Running'],
    passed: ['vc-pass', 'Passed'],
    failed: ['vc-fail', 'Failed'],
    cancelled: ['vc-blocked', 'Cancelled'],
    error: ['vc-fail', 'Stopped by an error'],
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
    <p class="st-secondary">No run yet. Start one from the Tests screen, or run every test here.</p>
    <button type="button" class="st-btn st-btn--primary" onclick={() => session.start({})}>
      Run all tests
    </button>
  </div>
{:else}
  <div class="head">
    <h2 class="st-title3">{title}</h2>
    <span class="st-badge {pill[0]}" role="status">{pill[1]}</span>
    {#if started}
      <span class="st-secondary">
        Started <time datetime={started.toISOString()}>{started.toLocaleTimeString()}</time>
      </span>
    {/if}
    {#if view.error}<span class="st-secondary">{view.error}</span>{/if}
    <span class="vc-grow"></span>
    {#if session.running}
      <StopControls {session} />
    {:else}
      {#if onopen && view.runId !== undefined}
        {@const id = Number(view.runId)}
        <button type="button" class="st-btn" onclick={() => onopen(id)}>Open report</button>
      {/if}
      {#if canRerun}
        <button type="button" class="st-btn" onclick={() => session.rerunFailed()}
          >Re-run failed</button
        >
      {/if}
      <button
        type="button"
        class="st-btn"
        onclick={() => session.request && session.start(session.request)}
      >
        Run again
      </button>
    {/if}
  </div>
  {#if !session.running && view.htmlReport}
    {@const path = view.htmlReport}
    <p class="file">
      <span class="st-secondary">HTML report</span>
      <code class="vc-mono path">{path}</code>
      <button
        type="button"
        class="st-btn"
        onclick={() => void copy(path)}
        onblur={() => (copied = false)}
        ><Glyph name={copied ? 'check' : 'copy'} />{copied ? 'Copied' : 'Copy path'}</button
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
