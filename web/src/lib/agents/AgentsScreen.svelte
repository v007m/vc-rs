<script lang="ts">
  import AgentLog from './AgentLog.svelte';
  import ApprovalQueue from './ApprovalQueue.svelte';
  import ConnectPanel from './ConnectPanel.svelte';
  import type { AgentDesk } from './desk.svelte.ts';

  let { desk, every = 2000 }: { desk: AgentDesk; every?: number } = $props();
  let now = $state(Date.now());

  $effect(() => {
    const stop = desk.poll(every);
    const clock = setInterval(() => (now = Date.now()), 15_000);
    return () => {
      stop();
      clearInterval(clock);
    };
  });
</script>

{#if desk.error}
  <p class="error" role="alert">Could not reach the engine: {desk.error}</p>
{/if}
<div class="agents">
  <ApprovalQueue {desk} {now} />
  <ConnectPanel />
  <AgentLog actions={desk.actions} />
</div>

<style>
  .agents {
    display: grid;
    gap: 32px;
    max-width: 960px;
  }
  .error {
    color: var(--fail);
  }
</style>
