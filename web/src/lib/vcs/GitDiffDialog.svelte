<script lang="ts">
  import Dialog from '#lib/ui/Dialog.svelte';
  import { gitError, letters, type ChangedFile } from './port.ts';
  import type { VcsSession } from './session.svelte.ts';

  /**
   * One changed file's diff against the last commit (D547), already masked by the engine. Each
   * line keeps its `+`, `-` or space, so the change never rests on colour alone.
   */
  let { vcs, file, onclose }: { vcs: VcsSession; file: ChangedFile; onclose: () => void } =
    $props();

  let diff: string | undefined = $state();
  let failed: string | undefined = $state();

  $effect(() => {
    vcs.diff(file.path).then(
      (text) => (diff = text),
      (error: unknown) => (failed = gitError(error)),
    );
  });

  function tone(line: string): string {
    if (line.startsWith('+++') || line.startsWith('---')) return 'meta';
    if (line.startsWith('@@')) return 'hunk';
    if (line.startsWith('+')) return 'add';
    if (line.startsWith('-')) return 'del';
    return '';
  }
</script>

<Dialog title={file.path} {onclose}>
  <p class="muted">{letters[file.change]} · {file.change} · against the last commit</p>
  {#if failed}
    <p role="alert">{failed}</p>
  {:else if diff === undefined}
    <p class="muted" role="status">Loading the diff…</p>
  {:else if diff === ''}
    <p class="muted">No changes in this file.</p>
  {:else}
    <pre
      class="diff mono"
      aria-label="Diff of {file.path}">{#each diff.split('\n') as line, i (i)}<span
          class={tone(line)}>{line || ' '}</span
        >{/each}</pre>
  {/if}
</Dialog>

<style>
  .diff {
    max-height: 60vh;
    overflow: auto;
    margin: 0;
    padding: var(--sp-2) 0;
    border-radius: var(--r-md);
    background: var(--well-bg);
    font-size: var(--fs-1);
    line-height: 1.5;
  }
  .diff span {
    display: block;
    padding: 0 var(--sp-3);
    white-space: pre;
  }
  .add {
    background: var(--diff-add-bg);
    color: var(--diff-add-ink);
  }
  .del {
    background: var(--diff-del-bg);
    color: var(--diff-del-ink);
  }
  .hunk,
  .meta {
    color: var(--ink-3);
  }
</style>
