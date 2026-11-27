<script lang="ts">
  import type { Diagnostic, Fix } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
  import type { RunSession } from '#lib/run/session.svelte.ts';
  import CodeView from './CodeView.svelte';
  import DiffView from './DiffView.svelte';
  import EditorBar, { type FileAction } from './EditorBar.svelte';
  import ExplainPanel from './ExplainPanel.svelte';
  import FileTree from './FileTree.svelte';
  import { baseName, folders } from './files.ts';
  import { marks } from './marks.ts';
  import NewFileDialog from './NewFileDialog.svelte';
  import PlanDialog from './PlanDialog.svelte';
  import Problems from './Problems.svelte';
  import RenameDialog from './RenameDialog.svelte';
  import { noEditorSeams, type EditorSeams } from './seams.ts';
  import type { EditorSession } from './session.svelte.ts';
  import { indexAt, lineDiff } from './text.ts';
  import TrashDialog from './TrashDialog.svelte';

  let {
    editor,
    session,
    seams = noEditorSeams,
  }: { editor: EditorSession; session: RunSession; seams?: EditorSeams } = $props();

  let filter = $state('');
  let showDiff = $state(false);
  /** A file asked for while the open one has unsaved changes. */
  let pending: string | undefined = $state();
  /** The file action whose dialog is open. */
  let dialog: Exclude<FileAction, 'format'> | undefined = $state();
  /** What the last quick file action said, such as why Format is not there yet. */
  let notice: string | undefined = $state();
  /** The code the explain panel shows, while it is open (D651). */
  let explaining: string | undefined = $state();
  let view: ReturnType<typeof CodeView> | undefined = $state();
  let buffer = $derived(editor.buffer);
  let shown = $derived(folders(editor.files, filter));
  let placed = $derived(buffer ? marks(buffer.text, buffer.path, editor.fileDiagnostics) : []);
  let changes = $derived.by(() => {
    const lines = buffer ? lineDiff(buffer.original, buffer.text) : [];
    return {
      add: lines.filter((l) => l.kind === 'add').length,
      del: lines.filter((l) => l.kind === 'del').length,
    };
  });

  function choose(path: string): void {
    if (path === buffer?.path) return;
    notice = undefined;
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
    if (!buffer || !span?.startLine) return;
    showDiff = false;
    const start = indexAt(buffer.text, span.startLine, span.startColumn || 1);
    const end = span.endLine ? indexAt(buffer.text, span.endLine, span.endColumn || 1) : start;
    queueMicrotask(() => view?.reveal(start, Math.max(start, end)));
  }

  async function format(): Promise<void> {
    if (!buffer) return;
    const got = await seams.files.format(buffer.path, buffer.text);
    notice = got.available ? undefined : got.reason;
    if (got.available) editor.edit(got.value);
  }

  /** A fix from a squiggle or a problem, or Fix all when `chosen` is unset (D652). */
  async function fix(chosen?: Readonly<{ fix: Fix; code: string }>): Promise<void> {
    notice = await editor.fix(seams.fixes, chosen);
  }

  function act(action: FileAction): void {
    notice = undefined;
    if (action === 'format') void format();
    else dialog = action;
  }

  /** After a create or rename: list the files again and open `path`. */
  async function reopen(path: string): Promise<void> {
    dialog = undefined;
    await editor.loadFiles();
    await editor.open(path);
  }

  async function trashed(to: string): Promise<void> {
    dialog = undefined;
    editor.close();
    notice = to ? `Moved to Trash: ${to}` : 'Moved to Trash';
    await editor.loadFiles();
  }
</script>

<svelte:window {onkeydown} />

<div class="editor" class:explaining>
  <aside class="files" aria-label="Project files">
    <label class="vc-sr" for="file-filter">Filter files</label>
    <input
      id="file-filter"
      class="st-input"
      type="search"
      placeholder="Filter files"
      bind:value={filter}
    />
    {#if shown.length}
      <FileTree folders={shown} current={buffer?.path} onopen={choose} />
    {:else}
      <p class="st-secondary">{editor.files.length ? 'No file matches.' : 'No files yet.'}</p>
    {/if}
    <button type="button" class="st-btn st-btn--sm" onclick={() => (dialog = 'new')}
      >New file…</button
    >
  </aside>

  <section class="main" aria-label="Editor">
    {#if editor.error}<p class="vc-banner" role="alert">{editor.error}</p>{/if}
    {#if pending}
      <div class="vc-banner vc-banner--warn" role="alert">
        <span class="vc-grow">Discard your unsaved changes to {baseName(buffer?.path ?? '')}?</span>
        <button type="button" class="st-btn st-btn--sm" onclick={discard}>Discard and open</button>
        <button type="button" class="st-btn st-btn--sm" onclick={() => (pending = undefined)}
          >Keep editing</button
        >
      </div>
    {/if}
    {#if !buffer && notice}<p class="st-secondary" role="status">{notice}</p>{/if}
    {#if !buffer}
      <p class="empty">{editor.state === 'loading' ? 'Opening…' : 'Pick a file to edit.'}</p>
    {:else}
      <EditorBar
        {editor}
        {session}
        {changes}
        bind:showDiff
        onaction={act}
        onfixall={() => void fix()}
      />
      {#if notice}<p class="st-secondary" role="status">{notice}</p>{/if}
      {#if editor.confirming}
        <div class="vc-banner" role="alert">
          <span class="vc-grow"
            >This file has {editor.errorCount}
            {editor.errorCount === 1 ? 'error' : 'errors'}. Save it anyway?</span
          >
          <button
            type="button"
            class="st-btn st-btn--sm vc-btn--destructive"
            onclick={() => void editor.saveAnyway()}>Save anyway</button
          >
          <button type="button" class="st-btn st-btn--sm" onclick={() => editor.cancelSave()}
            >Keep editing</button
          >
        </div>
      {/if}
      {#if editor.state === 'conflict'}
        <div class="vc-banner" role="alert">
          <span class="vc-grow">Not saved: this file changed on disk since you opened it.</span>
          <button type="button" class="st-btn st-btn--sm" onclick={() => void editor.reload()}
            >Reload from disk</button
          >
          <button type="button" class="st-btn st-btn--sm" onclick={() => void editor.keepMine()}
            >Keep my version</button
          >
        </div>
      {/if}
      {#if showDiff}
        <DiffView before={buffer.original} after={buffer.text} />
      {:else}
        <!-- A view per file, so undo never reaches into the file open before. -->
        {#key buffer.path}
          <CodeView
            bind:this={view}
            text={buffer.text}
            path={buffer.path}
            label={buffer.path}
            marks={placed}
            assist={seams.assist}
            onedit={(text) => editor.edit(text)}
            onfix={(f, code) => void fix({ fix: f, code })}
          />
        {/key}
      {/if}
      <Problems
        diagnostics={editor.fileDiagnostics}
        failure={editor.checkError}
        live={false}
        ongo={go}
        onfix={(f, code) => void fix({ fix: f, code })}
        onexplain={(code) => (explaining = code)}
      />
    {/if}
  </section>
  {#if explaining}
    <ExplainPanel
      port={seams.explain}
      target={explaining}
      onclose={() => (explaining = undefined)}
    />
  {/if}
</div>

{#if dialog === 'new'}
  <NewFileDialog
    files={seams.files}
    service={buffer?.path.split('/')[1] ?? ''}
    onclose={() => (dialog = undefined)}
    oncreated={(path) => void reopen(path)}
  />
{:else if buffer && dialog === 'rename'}
  <RenameDialog
    files={seams.files}
    path={buffer.path}
    id={editor.fileId(buffer.path)}
    onclose={() => (dialog = undefined)}
    onrenamed={(path) => void reopen(path)}
  />
{:else if buffer && dialog === 'trash'}
  <TrashDialog
    files={seams.files}
    path={buffer.path}
    digest={buffer.digest}
    onclose={() => (dialog = undefined)}
    ontrashed={(to) => void trashed(to)}
  />
{:else if buffer && dialog === 'plan'}
  <PlanDialog plan={seams.plan} path={buffer.path} onclose={() => (dialog = undefined)} />
{/if}

<style>
  .editor {
    display: grid;
    grid-template-columns: minmax(200px, 260px) minmax(0, 1fr);
    gap: 20px;
    min-height: 0;
  }
  .editor.explaining {
    grid-template-columns: minmax(200px, 260px) minmax(0, 1fr) minmax(240px, 320px);
  }
  .files,
  .main {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }
  .files .st-btn {
    align-self: flex-start;
  }
  /* a flex column would squash the field below its height */
  .files > .st-input {
    flex: none;
  }
  .st-secondary,
  .empty {
    color: var(--st-text-tertiary);
  }
  @container card (width < 960px) {
    .editor.explaining {
      grid-template-columns: minmax(200px, 260px) minmax(0, 1fr);
    }
    .editor.explaining > :global(.explain) {
      grid-column: 2;
    }
  }
  @container card (width < 660px) {
    .editor,
    .editor.explaining {
      grid-template-columns: minmax(0, 1fr);
    }
    .editor.explaining > :global(.explain) {
      grid-column: auto;
    }
  }
</style>
