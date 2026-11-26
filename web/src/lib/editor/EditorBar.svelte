<script lang="ts" module>
  /** A file action from the bar's menu; the screen opens its dialog. */
  export type FileAction = 'new' | 'rename' | 'format' | 'trash' | 'plan';
</script>

<script lang="ts">
  import type { RunSession } from '#lib/run/session.svelte.ts';
  import StopControls from '#lib/run/StopControls.svelte';
  import Menu from '#lib/ui/Menu.svelte';
  import { baseName, dirName, runnable } from './files.ts';
  import type { EditorSession } from './session.svelte.ts';

  /**
   * The open file's bar: its path and save state, then Diff, Revert, Fix all, Check, Run file
   * (Stop while it runs), Save, and the file menu.
   */
  let {
    editor,
    session,
    showDiff = $bindable(),
    changes,
    onaction,
  }: {
    editor: EditorSession;
    session: RunSession;
    showDiff: boolean;
    changes: Readonly<{ add: number; del: number }>;
    onaction: (action: FileAction) => void;
  } = $props();

  const actions = [
    { id: 'new', label: 'New file…' },
    { id: 'rename', label: 'Rename or move…' },
    { id: 'format', label: 'Format' },
    { id: 'plan', label: 'Plan preview…' },
    { id: 'trash', label: 'Move to trash…' },
  ] as const;

  let path = $derived(editor.buffer?.path ?? '');
  let errors = $derived(editor.errorCount);
  let fixable = $derived(editor.fixableCount);
  let status = $derived.by(() => {
    if (editor.state === 'saving') return 'Saving…';
    const unsaved = editor.dirty ? 'Unsaved' : 'Saved';
    return errors ? `${unsaved}, ${errors} ${errors === 1 ? 'error' : 'errors'}` : unsaved;
  });

  function pick(id: string): void {
    const action = actions.find((a) => a.id === id);
    if (action) onaction(action.id);
  }
</script>

<div class="head">
  <p class="path mono" title={path}>
    <span class="dir">{dirName(path) && `${dirName(path)}/`}</span>{baseName(path)}
  </p>
  <span class="pill" class:accent={editor.dirty && !errors} class:fail={errors > 0} role="status"
    >{status}</span
  >
  <span class="grow"></span>
  <button
    type="button"
    class="btn sm"
    aria-pressed={showDiff}
    onclick={() => (showDiff = !showDiff)}
  >
    Diff{#if changes.add || changes.del}<span class="plus">+{changes.add}</span><span class="minus"
        >−{changes.del}</span
      >{/if}
  </button>
  <button type="button" class="btn sm" disabled={!editor.dirty} onclick={() => editor.revert()}
    >Revert</button
  >
  {#if fixable}
    <button type="button" class="btn sm" onclick={() => editor.fixAll()}>Fix all ({fixable})</button
    >
  {/if}
  <button type="button" class="btn sm" onclick={() => void editor.check()}>Check</button>
  {#if session.running}
    <StopControls {session} />
  {:else if runnable(path)}
    <button
      type="button"
      class="btn sm"
      onclick={() => void session.start({ selection: { paths: [path] } })}>Run file</button
    >
  {/if}
  <button
    type="button"
    class="btn sm primary"
    disabled={!editor.canSave}
    title="Save (Ctrl+S or Cmd+S)"
    onclick={() => void editor.save()}>Save</button
  >
  <Menu label="File actions" items={actions} button="btn btn-ghost btn-icon btn-sm" onpick={pick}>
    <span aria-hidden="true">⋯</span>
  </Menu>
</div>

<style>
  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .path {
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dir {
    color: var(--ink3);
  }
  .plus {
    color: var(--pass);
  }
  .minus {
    color: var(--fail);
  }
</style>
