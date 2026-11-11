<script lang="ts">
  import Tabs from '#lib/ui/Tabs.svelte';
  import ChecksEditor from './ChecksEditor.svelte';
  import CurlDialog from './CurlDialog.svelte';
  import RequestForm from './RequestForm.svelte';
  import SaveDraftDialog from './SaveDraftDialog.svelte';
  import type { ClientSession } from './session.svelte.ts';

  let { client, onsaved }: { client: ClientSession; onsaved: (what: string) => void } = $props();

  let tab = $state('request');
  let dialog: 'save' | 'curl' | undefined = $state();
  const tabs = [
    { id: 'request', label: 'Request' },
    { id: 'vars', label: 'Vars' },
    { id: 'checks', label: 'Checks' },
    { id: 'options', label: 'Options' },
  ];
  const declared = $derived(new Set(client.current?.vars ?? []));
  const request = $derived(client.request);
  const layered = $derived(request?.explained?.vars ?? []);
</script>

<section class="request" aria-labelledby="request-title">
  {#if client.current}
    <div class="head">
      <div class="grow">
        <h2 class="h2" id="request-title">{client.current.name}</h2>
        <p class="mono id">{client.current.id}</p>
      </div>
      {#if request?.explained?.curl}
        <button type="button" class="btn" onclick={() => (dialog = 'curl')}>Copy as cURL</button>
      {/if}
      <button
        type="button"
        class="btn"
        disabled={!request?.dirty || request.problem !== undefined}
        onclick={() => (dialog = 'save')}>Save changes to api</button
      >
      <button
        type="button"
        class="btn primary"
        title="Send (Ctrl+Enter or Cmd+Enter)"
        disabled={client.sending}
        onclick={() => client.send()}>{client.sending ? 'Sending…' : 'Send'}</button
      >
    </div>
    {#if request?.error}
      <p class="error" role="alert">{request.error}</p>
    {/if}
    {#if request?.state === 'conflict'}
      <div class="banner" role="alert">
        <span class="grow"
          >Not sent or saved: <code>{request.written?.path}</code> changed on disk since it was read.</span
        >
        <button type="button" class="btn sm" onclick={() => void request.load()}
          >Reload from disk</button
        >
      </div>
    {/if}
    <Tabs label="Request" {tabs} bind:selected={tab}>
      {#snippet panel(id)}
        {#if id === 'request'}
          {#if request?.written}
            <RequestForm {request} />
          {:else}
            <p class="hint">{request?.state === 'loading' ? 'Reading the api…' : ''}</p>
          {/if}
        {:else if id === 'checks'}
          {#if request?.written}<ChecksEditor {request} />{/if}
        {:else if id === 'vars'}
          <p class="hint">
            Empty vars keep the api's defaults. Vars are sent like <code>--var</code>.
          </p>
          <ul class="vars">
            {#each client.vars as row, i (i)}
              <li>
                {#if declared.has(row.name)}
                  <label class="mono name" for="var-{i}">{row.name}</label>
                {:else}
                  <input class="input mono name" aria-label="Var name" bind:value={row.name} />
                {/if}
                <input
                  class="input mono grow"
                  id="var-{i}"
                  aria-label={declared.has(row.name) ? undefined : 'Var value'}
                  bind:value={row.value}
                />
                {#if !declared.has(row.name)}
                  <button type="button" class="btn sm" onclick={() => client.removeVar(i)}
                    >Remove</button
                  >
                {/if}
              </li>
            {/each}
          </ul>
          <button type="button" class="btn sm" onclick={() => client.addVar()}>Add var</button>
          {#if layered.length > 0}
            <table class="layers" aria-label="Vars from the project">
              <thead>
                <tr>
                  <th scope="col">Var</th>
                  <th scope="col">Resolves to</th>
                  <th scope="col">Set by</th>
                </tr>
              </thead>
              <tbody>
                {#each layered as v (v.key)}
                  <tr>
                    <th scope="row" class="mono">{v.key}</th>
                    <td class="mono">{v.layers[0]?.value ?? ''}</td>
                    <td>{v.layers[0]?.source ?? ''}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
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
    <p class="empty">Pick an api to call it with the project's auth and active profile.</p>
  {/if}
</section>

{#if dialog === 'save' && request && client.current}
  <SaveDraftDialog
    {request}
    api={client.current.id}
    onclose={() => (dialog = undefined)}
    onsaved={(what) => {
      dialog = undefined;
      onsaved(what);
    }}
  >
    {#snippet children(path: string)}
      Method, path, query, headers and body are written back to <code>{path}</code>, keeping its
      comments. Call-level vars and added checks stay in this tab.
    {/snippet}
  </SaveDraftDialog>
{:else if dialog === 'curl' && request?.explained}
  <CurlDialog curl={request.explained.curl} onclose={() => (dialog = undefined)} />
{/if}

<svelte:window
  onkeydown={(event) => {
    if (event.key === 'Enter' && (event.ctrlKey || event.metaKey) && client.current) {
      event.preventDefault();
      void client.send();
    }
  }}
/>

<style>
  .head {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    margin-bottom: 8px;
  }
  .id,
  .hint,
  .empty {
    margin: 2px 0 0;
    color: var(--ink2);
  }
  .vars {
    display: grid;
    gap: 6px;
    margin: 8px 0;
    padding: 0;
    list-style: none;
  }
  .vars li {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .name {
    width: 160px;
    flex: none;
  }
  .layers {
    width: 100%;
    margin-top: 12px;
    border-collapse: collapse;
    color: var(--ink2);
  }
  .layers th {
    font-weight: 500;
    text-align: left;
  }
  .banner {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
    padding: 8px 12px;
    border: 1px solid var(--fail);
    border-radius: 6px;
  }
  .error {
    margin: 0 0 8px;
    color: var(--fail);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
  }
</style>
