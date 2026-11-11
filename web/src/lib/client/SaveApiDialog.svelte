<script lang="ts">
  import { onMount } from 'svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import type { ApiRequest } from './request.svelte.ts';

  /** Reviews the form's edits as a masked diff of the api file, then writes them. */
  let {
    request,
    api,
    onclose,
    onsaved,
  }: {
    request: ApiRequest;
    api: string;
    onclose: () => void;
    onsaved: (what: string) => void;
  } = $props();

  const busy = $derived(request.state === 'reviewing' || request.state === 'saving');
  const path = $derived(request.explained?.path ?? '');

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
  <p class="help">
    Method, path, query, headers and body are written back to <code>{path}</code>, keeping its
    comments. Call-level vars and added checks stay in this tab.
  </p>
  {#if request.state === 'conflict'}
    <div class="banner" role="alert">
      <span class="grow">Not saved: <code>{path}</code> changed on disk since you opened it.</span>
      <button type="button" class="btn sm" onclick={() => void request.load()}
        >Reload from disk</button
      >
    </div>
  {/if}
  {#if request.error}
    <p class="error" role="alert">{request.error}</p>
  {/if}
  {#if request.diff !== undefined}
    <pre class="diff mono" aria-label="Changes to {path}">{request.diff ||
        'No change to the file.'}</pre>
  {:else if request.state === 'reviewing'}
    <p class="help">Reviewing…</p>
  {/if}
  <div class="actions">
    <button type="button" class="btn" onclick={onclose}>Cancel</button>
    <button
      type="button"
      class="btn primary"
      disabled={busy || !request.dirty || request.state === 'conflict'}
      onclick={() => void save()}>{request.state === 'saving' ? 'Saving…' : 'Save'}</button
    >
  </div>
</Dialog>

<style>
  .help {
    margin: 0 0 10px;
    color: var(--ink2);
  }
  .banner,
  .actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .banner {
    margin-bottom: 10px;
    padding: 8px 12px;
    border: 1px solid var(--fail);
    border-radius: 6px;
  }
  .actions {
    justify-content: flex-end;
    margin-top: 10px;
  }
  .diff {
    max-height: 320px;
    margin: 0;
    padding: 8px 12px;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 6px;
    white-space: pre;
  }
  .error {
    margin: 0 0 10px;
    color: var(--fail);
  }
</style>
