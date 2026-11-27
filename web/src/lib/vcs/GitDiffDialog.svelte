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
  <p class="st-secondary">{letters[file.change]} · {file.change} · against the last commit</p>
  {#if failed}
    <p role="alert">{failed}</p>
  {:else if diff === undefined}
    <p class="st-secondary" role="status">Loading the diff…</p>
  {:else if diff === ''}
    <p class="st-secondary">No changes in this file.</p>
  {:else}
    <pre
      class="diff vc-mono"
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
    padding: var(--st-space-8) 0;
    border-radius: var(--vc-radius-dialog-inner);
    background: var(--st-surface-input-on-card);
    font-size: var(--vc-size-caption);
    line-height: 1.5;
  }
  .diff span {
    display: block;
    padding: 0 var(--st-space-12);
    white-space: pre;
  }
  .add {
    background: var(--vc-diff-add-bg);
    color: var(--vc-diff-add-ink);
  }
  .del {
    background: var(--vc-diff-del-bg);
    color: var(--vc-diff-del-ink);
  }
  .hunk,
  .meta {
    color: var(--st-text-tertiary);
  }
</style>
