<script lang="ts">
  import { ConnectError } from '@connectrpc/connect';
  import { onMount } from 'svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import type { FileOps, FileUses } from './seams.ts';

  /**
   * Delete moves the file to the OS Trash, after listing what refers to it (ui.md §5, D654). It
   * sends the digest the file was read as, so a file changed since is never trashed.
   */
  let {
    files,
    path,
    digest,
    onclose,
    ontrashed,
  }: {
    files: FileOps;
    path: string;
    digest: string;
    onclose: () => void;
    /** Given where the file went; empty when the engine does not say. */
    ontrashed: (to: string) => void;
  } = $props();

  let uses: FileUses | undefined = $state.raw();
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
      if (got.available) uses = got.value;
      else note = got.reason;
    });
  });

  const trash = () =>
    step(async () => {
      const done = await files.trash(path, digest);
      if (done.available) ontrashed(done.value);
      else note = done.reason;
    });
</script>

<Dialog title="Move to Trash" {onclose}>
  <p>Move <code class="vc-mono">{path}</code> to the Trash?</p>
  {#if note}
    <p class="st-secondary" role="status">{note}</p>
  {:else if uses === undefined}
    <p class="st-secondary" role="status">Looking for tests that use it…</p>
  {:else if uses.uses.length}
    {@const n = uses.uses.length}
    <p role="alert">{n} {n === 1 ? 'reference' : 'references'} to it will break:</p>
    <ul class="refs vc-mono">
      {#each uses.uses as use (use)}<li>{use}</li>{/each}
    </ul>
  {:else}
    <p class="st-secondary" role="status">No other file refers to it.</p>
  {/if}
  {#if uses?.unread.length}
    <p class="st-secondary">
      Not searched, as {uses.unread.length === 1 ? 'it does' : 'they do'} not parse:
      <span class="vc-mono">{uses.unread.join(', ')}</span>
    </p>
  {/if}
  <div class="actions">
    <button type="button" class="st-btn" onclick={onclose}>Cancel</button>
    <button
      type="button"
      class="st-btn vc-btn--destructive"
      disabled={busy || uses === undefined}
      onclick={() => void trash()}>Move to Trash</button
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
