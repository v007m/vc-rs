<script lang="ts" module>
  /** A file action from the bar's menu; the screen opens its dialog. */
  export type FileAction = 'new' | 'rename' | 'format' | 'trash' | 'plan';
</script>

<script lang="ts">
  import type { RunSession } from '#lib/run/session.svelte.ts';
  import StopControls from '#lib/run/StopControls.svelte';
  import Menu, { type MenuItem } from '#lib/ui/Menu.svelte';
  import { watchWidth } from '#lib/ui/width.ts';
  import { barFold } from './barFold.ts';
  import { baseName, dirName, runnable } from './files.ts';
  import type { EditorSession } from './session.svelte.ts';

  /**
   * The open file's bar: its path and save state, then Diff, Revert, Fix all, Check, Run file
   * (Stop while it runs), Save, and the file menu. Run file is a check run (`record: false`,
   * D670): it writes no history entry, report or alert. The bar never wraps: on a narrow
   * pane the lowest-priority actions fold into the file menu (D669).
   */
  let {
    editor,
    session,
    showDiff = $bindable(),
    changes,
    onaction,
    onfixall,
  }: {
    editor: EditorSession;
    session: RunSession;
    showDiff: boolean;
    changes: Readonly<{ add: number; del: number }>;
    onaction: (action: FileAction) => void;
    /** Fix all: the screen fixes through the engine when it can (D652). */
    onfixall: () => void;
  } = $props();

  const actions = [
    { id: 'new', label: 'New file…' },
    { id: 'rename', label: 'Rename or move…' },
    { id: 'format', label: 'Format' },
    { id: 'plan', label: 'Plan preview…' },
    { id: 'trash', label: 'Move to Trash…' },
  ] as const;

  let width = $state(0);
  let fold = $derived(barFold(width));
  let path = $derived(editor.buffer?.path ?? '');
  let errors = $derived(editor.errorCount);
  let fixable = $derived(editor.fixableCount);
  let status = $derived.by(() => {
    if (editor.state === 'saving') return 'Saving…';
    const unsaved = editor.dirty ? 'Unsaved' : 'Saved';
    return errors ? `${unsaved}, ${errors} ${errors === 1 ? 'error' : 'errors'}` : unsaved;
  });

  let canRun = $derived(!session.running && runnable(path));
  let folded = $derived.by((): MenuItem[] => {
    const out: MenuItem[] = [];
    if (fold >= 2) out.push({ id: 'diff', label: showDiff ? 'Hide diff' : 'Show diff' });
    if (fold >= 1) out.push({ id: 'revert', label: 'Revert', disabled: !editor.dirty });
    if (fold >= 2 && fixable) out.push({ id: 'fixall', label: `Fix all (${fixable})` });
    if (fold >= 1) out.push({ id: 'check', label: 'Check' });
    if (fold >= 2 && canRun) out.push({ id: 'run', label: 'Run file' });
    return out;
  });

  function runFile(): void {
    void session.start({ selection: { paths: [path] }, record: false });
  }

  const barActions: Record<string, () => void> = {
    diff: () => (showDiff = !showDiff),
    revert: () => editor.revert(),
    fixall: () => onfixall(),
    check: () => void editor.check(),
    run: runFile,
  };

  function pick(id: string): void {
    const action = actions.find((a) => a.id === id);
    if (action) onaction(action.id);
    else barActions[id]?.();
  }
</script>

<div class="head vc-toolbar" {@attach watchWidth((w) => (width = w))}>
  <p class="path vc-mono" title={path}>
    <span class="dir">{dirName(path) && `${dirName(path)}/`}</span>{baseName(path)}
  </p>
  <span
    class="state"
    class:is-dirty={editor.dirty && !errors}
    class:is-fail={errors > 0}
    role="status">{status}</span
  >
  <span class="vc-grow"></span>
  {#if fold < 2}
    <button
      type="button"
      class="st-btn st-btn--sm"
      aria-pressed={showDiff}
      onclick={() => (showDiff = !showDiff)}
    >
      Diff{#if changes.add || changes.del}<span class="plus">+{changes.add}</span><span
          class="minus">−{changes.del}</span
        >{/if}
    </button>
  {/if}
  {#if fold < 1}
    <button
      type="button"
      class="st-btn st-btn--sm"
      disabled={!editor.dirty}
      onclick={() => editor.revert()}>Revert</button
    >
  {/if}
  {#if fixable && fold < 2}
    <button type="button" class="st-btn st-btn--sm" onclick={onfixall}>Fix all ({fixable})</button>
  {/if}
  {#if fold < 1}
    <button type="button" class="st-btn st-btn--sm" onclick={() => void editor.check()}
      >Check</button
    >
  {/if}
  {#if session.running}
    <StopControls {session} />
  {:else if canRun && fold < 2}
    <button
      type="button"
      class="st-btn st-btn--sm"
      title="A check run: its results show here and are not kept in Runs"
      onclick={runFile}>Run file</button
    >
  {/if}
  <button
    type="button"
    class="st-btn st-btn--sm st-btn--primary"
    disabled={!editor.canSave}
    title="Save (Ctrl+S or Cmd+S)"
    onclick={() => void editor.save()}>Save</button
  >
  <Menu
    label="File actions"
    items={[...folded, ...actions]}
    button="st-icon-btn st-icon-btn--sm"
    onpick={pick}
  >
    <span aria-hidden="true">⋯</span>
  </Menu>
</div>

<style>
  /* one row always (D669): the path gives way, and actions fold into the menu */
  .head {
    display: flex;
    flex-wrap: nowrap;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .head > :global(*) {
    flex-shrink: 0;
  }
  .head > .path {
    flex-shrink: 1;
    min-width: 0;
  }
  .path {
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dir {
    color: var(--st-text-tertiary);
  }
  /* save state as text in the bar, not a badge: badges never sit in a control row (D611) */
  .state {
    color: var(--st-text-secondary);
    font-size: var(--vc-size-caption);
    white-space: nowrap;
  }
  .state.is-dirty {
    color: var(--st-accent);
  }
  .state.is-fail {
    color: var(--vc-fail-ink);
  }
  .plus {
    color: var(--vc-pass);
  }
  .minus {
    color: var(--vc-fail);
  }
</style>
