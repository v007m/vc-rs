<script lang="ts">
  import type { RunSession } from './session.svelte.ts';

  /**
   * Stop for a running run (D552). Once stopped, the run says its cleanups are running, and offers
   * **Stop now**, a second Stop that ends the grace period of the work in flight, and **Skip
   * cleanups**, which abandons them.
   */
  let { session }: { session: RunSession } = $props();
</script>

{#if session.stopState === 'none'}
  <button type="button" class="st-btn" onclick={() => session.stop()}>Stop</button>
{:else}
  <span class="st-secondary" role="status">
    {session.stopState === 'stopping' ? 'Stopping: running the cleanups' : 'Skipping the cleanups'}
  </span>
  {#if session.stopState === 'stopping'}
    <button type="button" class="st-btn" onclick={() => session.stop()}>Stop now</button>
    <button type="button" class="st-btn vc-btn--destructive" onclick={() => session.skipCleanups()}>
      Skip cleanups
    </button>
  {/if}
{/if}
