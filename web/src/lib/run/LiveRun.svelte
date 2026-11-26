<script lang="ts">
  import { goto } from '$app/navigation';
  import { runHref } from '#lib/history/links.ts';
  import { testDetail } from '#lib/history/port.ts';
  import RunDetail from '#lib/history/RunDetail.svelte';
  import type { TestHistoryPort } from '#lib/history/seams.ts';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import RunPanel from './RunPanel.svelte';
  import type { RunSession } from './session.svelte.ts';

  /**
   * The live run (ui.md §6): its head with Stop, Stop now and Skip cleanups, its progress, plan
   * and events, and each finished test in the same view a stored run has (D634).
   */
  let {
    session,
    history,
    onrun,
  }: { session: RunSession; history: TestHistoryPort; onrun: () => void } = $props();

  const tests = $derived(session.view.finished.map(testDetail));
</script>

{#if session.view.phase === 'idle'}
  <EmptyState icon="runs" title="No run is going" caption="Start one here, or with Run in Tests.">
    {#snippet action()}
      <button type="button" class="st-btn st-btn--primary" onclick={onrun}>Run…</button>
    {/snippet}
  </EmptyState>
{:else}
  <div class="live">
    <RunPanel {session} onopen={(id) => void goto(runHref({ id, kind: 'run' }))} />
    <RunDetail label="Finished tests" {tests} {history} />
  </div>
{/if}

<style>
  .live {
    display: grid;
    gap: 16px;
  }
</style>
