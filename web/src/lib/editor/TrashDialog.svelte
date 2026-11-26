<script lang="ts">
  import { ConnectError } from '@connectrpc/connect';
  import { onMount } from 'svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import type { FileOps } from './seams.ts';

  /** Delete moves the file to the OS trash, after listing the tests that use it (ui.md §5). */
  let {
    files,
    path,
    onclose,
    ontrashed,
  }: { files: FileOps; path: string; onclose: () => void; ontrashed: () => void } = $props();

  let references: readonly string[] | undefined = $state();
  let note: string | undefined = $state();
  let busy = $state(false);

  async function step(work: () => Promise<void>): Promise<void> {
    busy = true;
    try {
      await work();
    } catch (error) {
      note = ConnectError.from(error).rawMessage;
    } finally {
      busy = false;
    }
  }

  onMount(() => {
    void step(async () => {
      const got = await files.references(path);
      if (got.available) references = got.value;
      else note = got.reason;
    });
  });

  const trash = () =>
    step(async () => {
      const done = await files.trash(path);
      if (done.available) ontrashed();
      else note = done.reason;
    });
</script>

<Dialog title="Move to trash" {onclose}>
  <p>Move <code class="mono">{path}</code> to the trash? You can restore it from there.</p>
  {#if note}
    <p class="muted" role="status">{note}</p>
  {:else if references === undefined}
    <p class="muted" role="status">Looking for tests that use it…</p>
  {:else if references.length}
    <p role="alert">
      {references.length}
      {references.length === 1 ? 'test refers' : 'tests refer'} to it and will break:
    </p>
    <ul class="refs mono">
      {#each references as id (id)}<li>{id}</li>{/each}
    </ul>
  {:else}
    <p class="muted" role="status">No other test refers to it.</p>
  {/if}
  <div class="actions">
    <button type="button" class="btn" onclick={onclose}>Cancel</button>
    <button
      type="button"
      class="btn btn-danger"
      disabled={busy || references === undefined}
      onclick={() => void trash()}>Move to trash</button
    >
  </div>
</Dialog>

<style>
  .refs {
    max-height: 200px;
    overflow: auto;
    margin: 0;
    padding-left: 20px;
    font-size: 12.5px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
</style>
