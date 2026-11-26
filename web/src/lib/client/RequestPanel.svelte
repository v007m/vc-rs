<script lang="ts">
  import Tabs from '#lib/ui/Tabs.svelte';
  import ChecksEditor from './ChecksEditor.svelte';
  import RequestForm from './RequestForm.svelte';
  import SaveDraftDialog from './SaveDraftDialog.svelte';
  import type { ClientSession } from './session.svelte.ts';
  import VarsPanel from './VarsPanel.svelte';

  /**
   * The request card of an api tab (ui.md §4): params, headers with the inherited ones
   * read-only, auth, body, vars, the checks to expect and the call's options. The method and
   * path sit in the request bar above.
   */
  let { client, onsaved }: { client: ClientSession; onsaved: (what: string) => void } = $props();

  let tab = $state('params');
  let saving = $state(false);
  const request = $derived(client.request);
  const auth = $derived(request?.explained?.auth);
  const tabs = $derived([
    { id: 'params', label: 'Params', ...count(request?.form.query.length) },
    { id: 'headers', label: 'Headers', ...count(request?.form.headers.length) },
    { id: 'auth', label: 'Auth' },
    { id: 'body', label: 'Body' },
    { id: 'vars', label: 'Vars' },
    { id: 'expect', label: 'Expect', ...count(request?.checks.length) },
    { id: 'options', label: 'Options' },
  ]);

  function count(n: number | undefined): { count?: string } {
    return n ? { count: String(n) } : {};
  }
</script>

<section class="glass" aria-labelledby="request-title">
  {#if client.current}
    <div class="card-head">
      <span class="method" data-m={request?.form.method}>{request?.form.method}</span>
      <h2 class="card-title" id="request-title">{client.current.name}</h2>
      <span class="file-ref grow">{client.current.id}</span>
      <button
        type="button"
        class="btn btn-sm"
        disabled={!request?.dirty || request.problem !== undefined}
        onclick={() => (saving = true)}>Save changes to api</button
      >
    </div>
    {#if request?.error || client.error}
      <p class="error" role="alert">{request?.error ?? client.error}</p>
    {/if}
    {#if request?.state === 'conflict'}
      <div class="banner" role="alert">
        <span class="grow"
          >Not sent or saved: <code>{request.written?.path}</code> changed on disk since it was read.</span
        >
        <button type="button" class="btn btn-sm" onclick={() => void request.load()}
          >Reload from disk</button
        >
      </div>
    {/if}
    {#if request && request.notices.length > 0}
      <ul class="notices" aria-label="Not saved">
        {#each request.notices as notice (notice)}<li>{notice}</li>{/each}
      </ul>
    {/if}
    <Tabs label="Request" {tabs} bind:selected={tab}>
      {#snippet panel(id)}
        {#if !request?.written}
          <p class="muted">{request?.state === 'loading' ? 'Reading the api…' : ''}</p>
        {:else if id === 'params' || id === 'headers' || id === 'body'}
          <RequestForm {request} part={id} />
        {:else if id === 'auth'}
          {@render authPanel()}
        {:else if id === 'vars'}
          <VarsPanel {client} />
        {:else if id === 'expect'}
          <ChecksEditor {request} />
        {:else}
          <label class="check">
            <input type="checkbox" bind:checked={client.keep} />
            Keep what the call creates (skip the api's cleanup, like <code>--keep</code>)
          </label>
        {/if}
      {/snippet}
    </Tabs>
  {:else}
    <h2 class="sr" id="request-title">Request</h2>
    <p class="resp-empty grow">
      Pick an api to call it with the project's auth and active profile.
    </p>
  {/if}
</section>

{#snippet authPanel()}
  {#if auth}
    <table class="client-table readonly" aria-label="Auth">
      <tbody>
        <tr><th scope="row">Auth</th><td class="mono">{auth.id}</td></tr>
        <tr><th scope="row">Type</th><td class="mono">{auth.type}</td></tr>
        {#if auth.tokenUrl}<tr
            ><th scope="row">Token URL</th><td class="mono">{auth.tokenUrl}</td></tr
          >{/if}
        {#if auth.secrets.length}<tr
            ><th scope="row">Secrets</th><td class="mono">{auth.secrets.join(', ')}</td></tr
          >{/if}
      </tbody>
    </table>
    <p class="muted">From the project's config. Change it there; secrets are never shown.</p>
  {:else}
    <p class="muted">No auth applies to this api under the active profile.</p>
  {/if}
{/snippet}

{#if saving && request && client.current}
  <SaveDraftDialog
    {request}
    api={client.current.id}
    onclose={() => (saving = false)}
    onsaved={(what) => {
      saving = false;
      onsaved(what);
    }}
  >
    {#snippet children(path: string)}
      Method, path, query, headers and body are written back to <code>{path}</code>, keeping its
      comments. Call-level vars and added checks stay in this tab.
    {/snippet}
  </SaveDraftDialog>
{/if}

<style>
  .banner {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin: 0 var(--card-pad) var(--sp-3);
    padding: 8px 12px;
    border: 1px solid var(--st-fail);
    border-radius: var(--r-md);
  }
  .error {
    margin: 0 var(--card-pad) var(--sp-3);
    color: var(--st-fail-ink);
  }
  .notices {
    margin: 0 var(--card-pad) var(--sp-3);
    padding-left: 18px;
    color: var(--ink-2);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
  }
</style>
