<script lang="ts">
  import { ConnectError } from '@connectrpc/connect';
  import { onMount } from 'svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import type { PlanPort, PlanPreview } from './seams.ts';

  /** `vc plan` of the open file: its summary line, its tree and its notices, as the CLI prints them (D655). */
  let { plan, path, onclose }: { plan: PlanPort; path: string; onclose: () => void } = $props();

  let preview: PlanPreview | undefined = $state();
  let note: string | undefined = $state();

  onMount(() => {
    plan
      .plan([path])
      .then((got) => {
        if (got.available) preview = got.value;
        else note = got.reason;
      })
      .catch((error: unknown) => (note = ConnectError.from(error).rawMessage));
  });
</script>

<Dialog title="Plan of {path}" {onclose}>
  {#if note}
    <p class="st-secondary" role="status">{note}</p>
  {:else if !preview}
    <p class="st-secondary" role="status">Planning…</p>
  {:else}
    <p class="line">{preview.line}</p>
    {#if preview.tree}<pre class="vc-mono tree">{preview.tree}</pre>{/if}
    {#each preview.notices as notice (notice)}<p class="st-secondary note">{notice}</p>{/each}
  {/if}
</Dialog>

<style>
  .line {
    margin: 0 0 8px;
  }
  .tree {
    max-height: 360px;
    margin: 0;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--st-surface-wash);
    font-size: 12.5px;
    overflow: auto;
    white-space: pre;
  }
  .note {
    margin: 8px 0 0;
    font-size: 12px;
  }
</style>
