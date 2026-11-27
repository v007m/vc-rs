<script lang="ts">
  import { ConnectError } from '@connectrpc/connect';
  import { untrack } from 'svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import DiffView from './DiffView.svelte';
  import { movedTo, type FileOps, type FilePreview } from './seams.ts';

  /**
   * Rename or move a file with `vc rename` of its file ID: a preview diffs every file it changes,
   * references included, before anything is written, and the write sends the preview's digest
   * back (ui.md §5, D653).
   */
  let {
    files,
    path,
    id,
    onclose,
    onrenamed,
  }: {
    files: FileOps;
    path: string;
    /** The file's ID (`users.crud`), the name `vc rename` takes. */
    id: string;
    onclose: () => void;
    /** Given the file's new path. */
    onrenamed: (to: string) => void;
  } = $props();

  const uid = $props.id();
  let to = $state(untrack(() => id));
  let preview: FilePreview | undefined = $state.raw();
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
      const got = await files.previewRename(id, to.trim());
      if (got.available) preview = got.value;
      else note = got.reason;
    });

  const rename = () =>
    step(async () => {
      if (!preview) return;
      const done = await files.rename(id, to.trim(), preview.digest);
      if (done.available) onrenamed(movedTo(done.value, path));
      else note = done.reason;
    });
</script>

<Dialog title="Rename or move" {onclose}>
  <label for="{uid}-to">New file ID</label>
  <input
    id="{uid}-to"
    class="st-input vc-mono"
    bind:value={to}
    oninput={() => (preview = undefined)}
  />
  {#if note}<p class="st-secondary" role="status">{note}</p>{/if}
  {#if preview}
    {@const n = preview.files.length}
    <p class="st-secondary">{n} {n === 1 ? 'file changes' : 'files change'}.</p>
    {#each preview.files as change (change.path)}
      <h3 class="vc-mono file">
        {change.movedFrom ? `${change.movedFrom} → ${change.path}` : change.path}
      </h3>
      <DiffView before={change.before} after={change.after} />
    {/each}
  {/if}
  {#snippet actions()}
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
        disabled={busy || to.trim() === '' || to.trim() === id}
        onclick={() => void review()}>Preview</button
      >
    {/if}
  {/snippet}
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
</style>
