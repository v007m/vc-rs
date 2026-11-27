<script lang="ts">
  import { onMount } from 'svelte';
  import type { Snippet } from 'svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import type { DefinitionDraft } from './draft.svelte.ts';

  /**
   * Reviews a form's edits as a masked diff of its file, then writes them; `children` says
   * what is written back to `path`.
   */
  let {
    request,
    api,
    onclose,
    onsaved,
    children,
  }: {
    request: DefinitionDraft;
    api: string;
    onclose: () => void;
    onsaved: (what: string) => void;
    children: Snippet<[path: string]>;
  } = $props();

  const busy = $derived(request.state === 'reviewing' || request.state === 'saving');
  const path = $derived(request.written?.path ?? '');

  onMount(() => void request.review());

  function said(): string {
    const saved = request.saved;
    if (saved?.checkError) return `Saved ${api}, but the project could not be checked.`;
    if (!saved || saved.problems === 0) return `Saved ${api}.`;
    return `Saved ${api}. The project has ${saved.problems} problem${saved.problems === 1 ? '' : 's'}.`;
  }

  async function save(): Promise<void> {
    await request.save();
    if (request.state === 'idle' && request.saved) onsaved(said());
  }
</script>

<Dialog title="Save changes to {api}" {onclose}>
  <p class="help">{@render children(path)}</p>
  {#if request.state === 'conflict'}
    <div class="vc-banner" role="alert">
      <span class="vc-grow"
        >Not saved: <code>{path}</code> changed on disk since you opened it.</span
      >
      <button type="button" class="st-btn st-btn--sm" onclick={() => void request.load()}
        >Reload from disk</button
      >
    </div>
  {/if}
  {#if request.error}
    <p class="error" role="alert">{request.error}</p>
  {/if}
  {#if request.diff !== undefined}
    <pre class="diff vc-mono" aria-label="Changes to {path}">{request.diff ||
        'No change to the file.'}</pre>
  {:else if request.state === 'reviewing'}
    <p class="help">Reviewing…</p>
  {/if}
  {#snippet actions()}
    <button type="button" class="st-btn" onclick={onclose}>Cancel</button>
    <button
      type="button"
      class="st-btn st-btn--primary"
      disabled={busy || !request.dirty || request.state === 'conflict'}
      onclick={() => void save()}>{request.state === 'saving' ? 'Saving…' : 'Save'}</button
    >
  {/snippet}
</Dialog>

<style>
  .help {
    margin: 0 0 10px;
    color: var(--st-text-secondary);
  }
  .vc-banner {
    margin-bottom: 10px;
  }
  .diff {
    max-height: 320px;
    margin: 0;
    padding: 8px 12px;
    overflow: auto;
    border-radius: var(--vc-radius-dialog-inner);
    background: var(--st-surface-wash);
    white-space: pre;
  }
  .error {
    margin: 0 0 10px;
    color: var(--vc-fail);
  }
</style>
