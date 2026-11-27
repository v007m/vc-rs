<script lang="ts">
  import { initNotYet } from './port.ts';

  /** A step's note: quiet when this vc cannot set up a project here, an alert when a step failed. */
  let { note }: { note: string | undefined } = $props();
</script>

{#if note === initNotYet}
  <p class="quiet" role="status">{note}</p>
{:else if note}
  <p class="error" role="alert">{note}</p>
{/if}

<style>
  p {
    margin: 0;
  }
  .quiet {
    color: var(--st-text-secondary);
  }
  .error {
    color: var(--vc-fail);
  }
</style>
