<script lang="ts">
  import Dialog from '#lib/ui/Dialog.svelte';
  import type { Toasts } from '#lib/ui/toasts.svelte.ts';
  import ChangeList from '#lib/writes/ChangeList.svelte';
  import type { Plugins } from './plugins.svelte.ts';

  /** Remove a plugin (D715): the config and lock changes previewed, then written with their digest. */
  let { plugins, id, toasts }: { plugins: Plugins; id: string; toasts: Toasts } = $props();

  let staged = $derived(plugins.removing);

  async function remove(): Promise<void> {
    const removed = id;
    if (await plugins.remove()) toasts.show(`Removed the plugin ${removed}.`);
    else if (staged.note) toasts.show(staged.note, 'error');
  }
</script>

<Dialog title="Remove {id}?" onclose={() => plugins.closeRemove()}>
  <div class="body">
    <p class="intro">
      Removes the plugin from the config and <code>vibranium.lock</code>, then uninstalls it.
    </p>
    {#if staged.note}<p class="error" role="alert">{staged.note}</p>{/if}
    {#if staged.shown}
      <ChangeList files={staged.shown.files} />
    {:else if staged.busy}
      <p class="intro" role="status">Previewing…</p>
    {/if}
    <div class="actions">
      <button type="button" class="st-btn" onclick={() => plugins.closeRemove()}>Cancel</button>
      <button
        type="button"
        class="st-btn st-btn--danger"
        disabled={staged.busy || !staged.shown}
        onclick={() => void remove()}>{staged.busy && staged.shown ? 'Removing…' : 'Remove'}</button
      >
    </div>
  </div>
</Dialog>

<style>
  .body {
    display: grid;
    gap: 12px;
  }
  .intro {
    margin: 0;
    color: var(--st-text-secondary);
  }
  .error {
    margin: 0;
    color: var(--vc-fail-ink);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 4px;
  }
</style>
