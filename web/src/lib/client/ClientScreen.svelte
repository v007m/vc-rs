<script lang="ts">
  import { resolve } from '$app/paths';
  import type { RunSession } from '#lib/run/session.svelte.ts';
  import GroupTree from '#lib/tree/GroupTree.svelte';
  import { apiGroups } from './apis.ts';
  import HarDialog from './HarDialog.svelte';
  import { HarImport } from './har.svelte.ts';
  import type { ClientPort } from './port.ts';
  import RequestPanel from './RequestPanel.svelte';
  import ResponsePanel from './ResponsePanel.svelte';
  import SaveAsTest from './SaveAsTest.svelte';
  import type { ClientSession } from './session.svelte.ts';

  let {
    client,
    port,
    session,
  }: { client: ClientSession; port: Pick<ClientPort, 'importHar'>; session: RunSession } = $props();

  let mode: 'request' | 'flow' = $state('request');
  let filter = $state('');
  let dialog: 'save' | 'har' | undefined = $state();
  let har: HarImport | undefined = $state();
  let said = $state('');
  const groups = $derived(apiGroups(client.apis, filter));

  function openHar(): void {
    har = new HarImport(port);
    dialog = 'har';
  }

  function done(what: string): void {
    dialog = undefined;
    said = what;
  }
</script>

<div class="client">
  <aside class="side" aria-label="Apis and flows">
    <div class="seg" role="group" aria-label="Mode">
      <button type="button" aria-pressed={mode === 'request'} onclick={() => (mode = 'request')}
        >Request</button
      >
      <button type="button" aria-pressed={mode === 'flow'} onclick={() => (mode = 'flow')}
        >Flow</button
      >
    </div>
    <button type="button" class="btn sm" onclick={openHar}>Import HAR</button>
    {#if mode === 'request'}
      <input
        class="input"
        type="search"
        aria-label="Filter apis"
        placeholder="Filter apis"
        bind:value={filter}
      />
      <GroupTree
        label="Apis"
        {groups}
        current={client.current?.id}
        onopen={(id) => client.open(id)}
      />
      {#if groups.length === 0}
        <p class="empty">{client.apis.length ? `No api matches “${filter}”.` : 'No apis yet.'}</p>
      {/if}
    {/if}
  </aside>

  <div class="main">
    {#if client.error && !dialog}
      <p class="error" role="alert">{client.error}</p>
    {/if}
    {#if mode === 'request'}
      <RequestPanel {client} />
      <ResponsePanel {client} onsave={() => (dialog = 'save')} />
    {:else}
      <section aria-labelledby="flows-title">
        <h2 class="h2" id="flows-title">Flow tests</h2>
        <p class="empty">
          A flow is a test whose <code>steps</code> call apis in order, saving values between them.
        </p>
        <ul class="flows" aria-label="Flow tests">
          {#each client.flows as flow (flow.id)}
            <li>
              <span class="grow"
                ><span class="mono">{flow.id}</span> · {flow.name} · {flow.steps} steps</span
              >
              <button
                type="button"
                class="btn sm"
                disabled={session.running}
                onclick={() => session.start({ selection: { ids: [flow.id] } })}>Run</button
              >
            </li>
          {:else}
            <li class="empty">No flow tests yet. Import a HAR with a flow test to record one.</li>
          {/each}
        </ul>
        {#if session.running}
          <a href={resolve('/runs')}>See the running flow in Runs</a>
        {/if}
      </section>
    {/if}
    <p class="said" role="status">{said}</p>
  </div>
</div>

{#if dialog === 'save'}
  <SaveAsTest {client} onclose={() => (dialog = undefined)} onsaved={done} />
{:else if dialog === 'har' && har}
  <HarDialog
    {har}
    onclose={() => (dialog = undefined)}
    onwritten={(titles) => {
      done(titles.join(' · ') || 'Imported');
      void client.load();
    }}
  />
{/if}

<style>
  .client {
    display: grid;
    grid-template-columns: minmax(220px, 280px) 1fr;
    gap: 20px;
    min-height: 0;
  }
  .side {
    display: grid;
    align-content: start;
    gap: 10px;
  }
  .main {
    display: grid;
    align-content: start;
    gap: 20px;
    min-width: 0;
  }
  .empty {
    color: var(--ink2);
  }
  .error {
    margin: 0;
    color: var(--fail);
  }
  .flows {
    display: grid;
    gap: 6px;
    padding: 0;
    list-style: none;
  }
  .flows li {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .said:empty {
    display: none;
  }
  @media (width < 720px) {
    .client {
      grid-template-columns: 1fr;
    }
  }
</style>
