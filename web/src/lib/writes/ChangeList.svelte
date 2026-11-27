<script lang="ts">
  import DiffView from '#lib/editor/DiffView.svelte';
  import type { FileChange } from '#lib/editor/seams.ts';

  /** The files a write would change, each as a diff (ui.md §5): a new file is all added lines. */
  let { files }: { files: readonly FileChange[] } = $props();
</script>

<p class="st-secondary">
  {files.length}
  {files.length === 1 ? 'file changes' : 'files change'}.
</p>
{#each files as change (change.path)}
  <h3 class="vc-mono file">
    {change.movedFrom ? `${change.movedFrom} → ${change.path}` : change.path}
  </h3>
  <DiffView
    before={change.before}
    after={change.after}
    label="Changes to {change.path}"
    empty="No change to this file."
  />
{/each}

<style>
  .st-secondary {
    margin: 0;
  }
  .file {
    margin: 12px 0 6px;
    font-size: 12.5px;
  }
</style>
