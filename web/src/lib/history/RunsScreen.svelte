<script lang="ts">
  import RunPanel from '#lib/run/RunPanel.svelte';
  import type { RunSession } from '#lib/run/session.svelte.ts';
  import type { RunHistory } from './history.svelte.ts';
  import RecentRuns from './RecentRuns.svelte';
  import StoredRunView from './StoredRunView.svelte';

  /** The live run, or a run from the history picked from the recent runs beside it. */
  let { history, session }: { history: RunHistory; session: RunSession } = $props();

  let wasRunning = false;
  $effect(() => {
    const running = session.running;
    if (wasRunning && !running) void history.load();
    wasRunning = running;
  });
</script>

<div class="runs">
  <aside class="side">
    <RecentRuns
      runs={history.runs}
      current={history.open?.run.id}
      onopen={(id) => void history.show(id)}
    />
  </aside>
  <div class="main">
    {#if history.error}
      <p class="error" role="alert">{history.error}</p>
    {/if}
    {#if history.open}
      <StoredRunView stored={history.open} {history} {session} />
    {:else}
      <RunPanel {session} />
    {/if}
  </div>
</div>

<style>
  .runs {
    display: grid;
    grid-template-columns: minmax(200px, 260px) 1fr;
    gap: 20px;
    min-height: 0;
  }
  .main {
    display: grid;
    align-content: start;
    gap: 12px;
    min-width: 0;
  }
  .error {
    margin: 0;
    color: var(--fail);
  }
  @media (width < 720px) {
    .runs {
      grid-template-columns: 1fr;
    }
  }
</style>
