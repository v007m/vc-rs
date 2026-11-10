<script lang="ts">
  import { resolve } from '$app/paths';
  import type { Diagnostic } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
  import type { RunSession } from '#lib/run/session.svelte.ts';
  import DiffView from './DiffView.svelte';
  import FileTree from './FileTree.svelte';
  import { baseName, dirName, folders, runnable } from './files.ts';
  import Problems from './Problems.svelte';
  import type { EditorSession } from './session.svelte.ts';
  import { indexAt, lineDiff } from './text.ts';

  let { editor, session }: { editor: EditorSession; session: RunSession } = $props();

  let filter = $state('');
  let showDiff = $state(false);
  /** A file asked for while the open one has unsaved changes. */
  let pending: string | undefined = $state();
  let area: HTMLTextAreaElement | undefined = $state();
  let buffer = $derived(editor.buffer);
  let shown = $derived(folders(editor.files, filter));
  let changes = $derived.by(() => {
    const lines = buffer ? lineDiff(buffer.original, buffer.text) : [];
    return {
      add: lines.filter((l) => l.kind === 'add').length,
      del: lines.filter((l) => l.kind === 'del').length,
    };
  });
  let status = $derived.by(() => {
    if (editor.state === 'saving') return 'Saving…';
    if (!editor.dirty) return 'Saved';
    const n = editor.errorCount;
    return n ? `Fix ${n} ${n === 1 ? 'error' : 'errors'} to save` : 'Unsaved changes';
  });

  function choose(path: string): void {
    if (path === buffer?.path) return;
    if (editor.dirty) pending = path;
    else void editor.open(path);
  }

  function discard(): void {
    const path = pending;
    pending = undefined;
    if (path) void editor.open(path);
  }

  function onkeydown(event: KeyboardEvent): void {
    if (event.key !== 's' || !(event.metaKey || event.ctrlKey)) return;
    event.preventDefault();
    void editor.save();
  }

  function go(d: Diagnostic): void {
    const span = d.primary?.span;
    if (!area || !buffer || !span?.startLine) return;
    showDiff = false;
    const text = buffer.text;
    const start = indexAt(text, span.startLine, span.startColumn || 1);
    const end = span.endLine ? indexAt(text, span.endLine, span.endColumn || 1) : start;
    area.focus();
    area.setSelectionRange(start, Math.max(start, end));
  }
</script>

<svelte:window {onkeydown} />

<div class="editor">
  <aside class="files" aria-label="Project files">
    <label class="sr" for="file-filter">Filter files</label>
    <input
      id="file-filter"
      class="input"
      type="search"
      placeholder="Filter files"
      bind:value={filter}
    />
    {#if shown.length}
      <FileTree folders={shown} current={buffer?.path} onopen={choose} />
    {:else}
      <p class="muted">{editor.files.length ? 'No file matches.' : 'No files yet.'}</p>
    {/if}
  </aside>

  <section class="main" aria-label="Editor">
    {#if editor.error}<p class="banner err" role="alert">{editor.error}</p>{/if}
    {#if pending}
      <div class="banner warn" role="alert">
        <span class="grow">Discard your unsaved changes to {baseName(buffer?.path ?? '')}?</span>
        <button type="button" class="btn sm" onclick={discard}>Discard and open</button>
        <button type="button" class="btn sm" onclick={() => (pending = undefined)}
          >Keep editing</button
        >
      </div>
    {/if}
    {#if !buffer}
      <p class="empty">{editor.state === 'loading' ? 'Opening…' : 'Pick a file to edit.'}</p>
    {:else}
      <div class="head">
        <p class="path mono" title={buffer.path}>
          <span class="dir">{dirName(buffer.path) && `${dirName(buffer.path)}/`}</span>{baseName(
            buffer.path,
          )}
        </p>
        <span class="pill" class:accent={editor.dirty} role="status">{status}</span>
        <span class="grow"></span>
        <button
          type="button"
          class="btn sm"
          aria-pressed={showDiff}
          onclick={() => (showDiff = !showDiff)}
        >
          Diff{#if changes.add || changes.del}<span class="plus">+{changes.add}</span><span
              class="minus">−{changes.del}</span
            >{/if}
        </button>
        <button
          type="button"
          class="btn sm"
          disabled={!editor.dirty}
          onclick={() => editor.revert()}>Revert</button
        >
        {#if runnable(buffer.path)}
          <button
            type="button"
            class="btn sm"
            disabled={session.running}
            onclick={() => void session.start({ selection: { paths: [buffer.path] } })}
            >Run file</button
          >
        {/if}
        <button
          type="button"
          class="btn sm primary"
          disabled={!editor.canSave}
          title="Save (Ctrl+S or Cmd+S)"
          onclick={() => void editor.save()}>Save</button
        >
      </div>
      {#if editor.state === 'conflict'}
        <div class="banner err" role="alert">
          <span class="grow">Not saved: this file changed on disk since you opened it.</span>
          <button type="button" class="btn sm" onclick={() => void editor.reload()}
            >Reload from disk</button
          >
          <button type="button" class="btn sm" onclick={() => void editor.keepMine()}
            >Keep my version</button
          >
        </div>
      {/if}
      {#if session.running}
        <p class="muted" role="status">
          A run is going. <a href={resolve('/runs')}>Watch it in Runs</a>
        </p>
      {/if}
      {#if showDiff}
        <DiffView before={buffer.original} after={buffer.text} />
      {:else}
        <textarea
          class="text mono"
          aria-label={buffer.path}
          spellcheck="false"
          autocapitalize="off"
          bind:this={area}
          value={buffer.text}
          oninput={(event) => editor.edit(event.currentTarget.value)}></textarea>
      {/if}
      <Problems
        diagnostics={editor.fileDiagnostics}
        failure={editor.checkError}
        live={false}
        ongo={go}
        onfix={(fix) => editor.applyFix(fix)}
      />
    {/if}
  </section>
</div>

<style>
  .editor {
    display: grid;
    grid-template-columns: minmax(200px, 260px) minmax(0, 1fr);
    gap: 20px;
    min-height: 0;
  }
  .files,
  .main {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
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
  .dir,
  .muted,
  .empty {
    color: var(--ink3);
  }
  .plus {
    color: var(--pass);
  }
  .minus {
    color: var(--fail);
  }
  .text {
    min-height: 420px;
    resize: vertical;
    padding: 12px;
    border: 1px solid var(--line2);
    border-radius: 10px;
    background: var(--panel);
    color: var(--ink);
    line-height: 20px;
    tab-size: 2;
  }
  .text:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 0;
  }
  .banner {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin: 0;
    padding: 8px 12px;
    border-radius: 10px;
  }
  .err {
    background: var(--failSoft);
    color: var(--fail);
  }
  .warn {
    background: var(--blockSoft);
    color: var(--block);
  }
  @media (max-width: 760px) {
    .editor {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
