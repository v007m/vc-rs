<script lang="ts">
  import { onMount } from 'svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import ChangeList from '#lib/writes/ChangeList.svelte';
  import { Staged } from '#lib/writes/staged.svelte.ts';

  /**
   * A write with nothing to fill in, Remove or Export (D706, D707): it opens on the preview's
   * diff, and the action writes it with the preview's digest. `onwrite` gives what the dialog
   * says once written, or `undefined` to close it.
   */
  let {
    title,
    intro,
    action,
    danger = false,
    onreview,
    onwrite,
    onclose,
  }: {
    title: string;
    intro: string;
    /** The write button's words. */
    action: string;
    danger?: boolean;
    onreview: (staged: Staged) => Promise<void>;
    onwrite: (staged: Staged) => Promise<string | undefined>;
    onclose: () => void;
  } = $props();

  const staged = new Staged();
  let done: string | undefined = $state();

  onMount(() => void onreview(staged));

  async function write(): Promise<void> {
    const said = await onwrite(staged);
    if (said === undefined) {
      if (!staged.note) onclose();
    } else done = said;
  }
</script>

<Dialog {title} {onclose}>
  {#if done}
    <p class="said" role="status">{done}</p>
    <div class="actions">
      <button type="button" class="st-btn st-btn--primary" onclick={onclose}>Done</button>
    </div>
  {:else}
    <p class="intro">{intro}</p>
    {#if staged.note}<p class="note" role="status">{staged.note}</p>{/if}
    {#if staged.shown}
      <ChangeList files={staged.shown.files} />
    {:else if staged.busy}
      <p class="note" role="status">Previewing…</p>
    {/if}
    <div class="actions">
      <button type="button" class="st-btn" onclick={onclose}>Cancel</button>
      <button
        type="button"
        class="st-btn {danger ? 'st-btn--danger' : 'st-btn--primary'}"
        disabled={staged.busy || !staged.shown}
        onclick={() => void write()}>{action}</button
      >
    </div>
  {/if}
</Dialog>

<style>
  .intro,
  .said {
    margin: 0 0 12px;
  }
  .note {
    margin: 0 0 12px;
    color: var(--st-text-secondary);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
</style>
