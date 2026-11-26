<script lang="ts">
  import { ConnectError } from '@connectrpc/connect';
  import { onMount } from 'svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import type { PlanPort, PlanPreview } from './seams.ts';

  /** `vc plan --explain` for the open file: its instances, the lifecycle order, the merged expects. */
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
    {@const n = preview.instances.length}
    <h3 class="h3">{n} {n === 1 ? 'instance' : 'instances'}</h3>
    <ul class="list">
      {#each preview.instances as instance (instance.id)}
        <li>
          <span class="vc-mono">{instance.id}</span>
          <span class="st-secondary">{instance.name}</span>
          {#if instance.expects.length}
            <ul class="expects vc-mono" aria-label="Expects of {instance.id}">
              {#each instance.expects as expect, i (i)}<li>{expect}</li>{/each}
            </ul>
          {/if}
        </li>
      {/each}
    </ul>
    <h3 class="h3">Lifecycle order</h3>
    <ol class="list vc-mono">
      {#each preview.order as step, i (i)}
        <li><span class="phase">{step.phase}</span> {step.target}</li>
      {/each}
    </ol>
  {/if}
</Dialog>

<style>
  .h3 {
    margin: 12px 0 6px;
    font-size: 13px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding-left: 20px;
    font-size: 12.5px;
  }
  .expects {
    margin: 2px 0 0;
    padding-left: 16px;
    color: var(--st-text-secondary);
  }
  .phase {
    color: var(--vc-syn-key);
  }
</style>
