<script lang="ts">
  import { ConnectError } from '@connectrpc/connect';
  import { untrack } from 'svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import DiffView from './DiffView.svelte';
  import type { FileChange, FileOps } from './seams.ts';

  /**
   * Rename or move a file with `vc rename`: a preview diffs every file it changes, references
   * included, before anything is written (ui.md §5).
   */
  let {
    files,
    path,
    onclose,
    onrenamed,
  }: {
    files: FileOps;
    path: string;
    onclose: () => void;
    onrenamed: (to: string) => void;
  } = $props();

  const uid = $props.id();
  let to = $state(untrack(() => path));
  let preview: readonly FileChange[] | undefined = $state();
  let busy = $state(false);
  let note: string | undefined = $state();

  async function step(work: () => Promise<void>): Promise<void> {
    busy = true;
    note = undefined;
    try {
      await work();
    } catch (error) {
      note = ConnectError.from(error).rawMessage;
    } finally {
      busy = false;
    }
  }

  const review = () =>
    step(async () => {
      const got = await files.previewRename(path, to.trim());
      if (got.available) preview = got.value;
      else note = got.reason;
    });

  const rename = () =>
    step(async () => {
      const done = await files.rename(path, to.trim());
      if (done.available) onrenamed(to.trim());
      else note = done.reason;
    });
</script>

<Dialog title="Rename or move" {onclose}>
  <label for="{uid}-to">New path</label>
  <input
    id="{uid}-to"
    class="st-input vc-mono"
    bind:value={to}
    oninput={() => (preview = undefined)}
  />
  {#if note}<p class="st-secondary" role="status">{note}</p>{/if}
  {#if preview}
    <p class="st-secondary">
      {preview.length}
      {preview.length === 1 ? 'file changes' : 'files change'}.
    </p>
    {#each preview as change (change.path)}
      <h3 class="vc-mono file">{change.path}</h3>
      <DiffView before={change.before} after={change.after} />
    {/each}
  {/if}
  <div class="actions">
    <button type="button" class="st-btn" onclick={onclose}>Cancel</button>
    {#if preview}
      <button
        type="button"
        class="st-btn st-btn--primary"
        disabled={busy}
        onclick={() => void rename()}>Rename</button
      >
    {:else}
      <button
        type="button"
        class="st-btn st-btn--primary"
        disabled={busy || to.trim() === '' || to.trim() === path}
        onclick={() => void review()}>Preview</button
      >
    {/if}
  </div>
</Dialog>

<style>
  .st-input {
    width: 100%;
    margin-top: 6px;
  }
  .file {
    margin: 12px 0 6px;
    font-size: 12.5px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
</style>
