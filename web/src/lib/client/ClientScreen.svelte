<script lang="ts">
  import { resolve } from '$app/paths';
  import type { RunSession } from '#lib/run/session.svelte.ts';
  import { getShellSlots } from '#lib/shell/slots.svelte.ts';
  import ClientSidebar from './ClientSidebar.svelte';
  import ClientTabStrip from './ClientTabStrip.svelte';
  import { FlowEditor } from './flow.svelte.ts';
  import FlowPanel from './FlowPanel.svelte';
  import HarDialog from './HarDialog.svelte';
  import { HarImport } from './har.svelte.ts';
  import type { ClientLayout } from './layout.svelte.ts';
  import type { ClientPort } from './port.ts';
  import RequestBar from './RequestBar.svelte';
  import RequestPanel from './RequestPanel.svelte';
  import ResponsePanel from './ResponsePanel.svelte';
  import SaveAsTest from './SaveAsTest.svelte';
  import ScratchPanel from './ScratchPanel.svelte';
  import type { HistorySource } from './seams.ts';
  import type { ClientSession } from './session.svelte.ts';
  import Split from './Split.svelte';
  import StreamPanel from './StreamPanel.svelte';

  /**
   * The Client space (ui.md §4): its sidebar and tab strip fill the shell's slots (D537); the
   * stage holds the request bar over the request and response cards, or a flow test's editor.
   */
  let {
    client,
    port,
    session,
    layout,
    history,
  }: {
    client: ClientSession;
    port: Pick<ClientPort, 'importHar' | 'readDefinition' | 'editFile'>;
    session: RunSession;
    layout: ClientLayout;
    history: HistorySource;
  } = $props();

  let mode: 'request' | 'flow' = $state('request');
  let dialog: 'save' | 'har' | undefined = $state();
  let har: HarImport | undefined = $state();
  let said = $state('');
  let flow: FlowEditor | undefined = $state();
  const tab = $derived(client.tab);
  const scratch = $derived(tab?.kind === 'scratch' ? tab : undefined);

  $effect(() => getShellSlots().fill({ sidebar, top, bare: true }));

  function openHar(): void {
    har = new HarImport(port);
    dialog = 'har';
  }

  function edit(id: string): void {
    mode = 'flow';
    if (flow?.id === id) return;
    flow = new FlowEditor(port, id);
    void flow.load();
  }

  function done(what: string): void {
    dialog = undefined;
    said = what;
  }
</script>

{#snippet sidebar()}
  <ClientSidebar
    {client}
    {history}
    flow={mode === 'flow' ? flow?.id : undefined}
    onhar={openHar}
    onapi={() => (mode = 'request')}
    onflow={edit}
  />
{/snippet}

{#snippet top()}
  <ClientTabStrip {client} onpick={() => (mode = 'request')} />
{/snippet}

<h1 class="sr">API client</h1>
{#if client.error && !client.current && !dialog}
  <p class="error" role="alert">{client.error}</p>
{/if}
{#if mode === 'request'}
  <div class="client">
    <RequestBar {client} {layout} />
    <Split {layout}>
      {#snippet first()}
        {#if scratch}
          <ScratchPanel tab={scratch} />
        {:else}
          <RequestPanel {client} onsaved={(what) => (said = what)} />
        {/if}
      {/snippet}
      {#snippet second()}
        {#if scratch && scratch.type !== 'http'}
          <StreamPanel stream={scratch.stream} websocket={scratch.type === 'websocket'} />
        {:else if scratch}
          <ResponsePanel output={scratch.output} sending={scratch.sending} />
        {:else}
          <ResponsePanel
            output={client.result?.output}
            sending={client.sending}
            oncheck={client.request ? (check) => client.request?.addCheck(check) : undefined}
            onsave={client.call ? () => (dialog = 'save') : undefined}
          />
        {/if}
      {/snippet}
    </Split>
  </div>
{:else}
  <div class="flows-view">
    <section class="glass flows-card" aria-labelledby="flows-title">
      <div class="card-head">
        <h2 class="card-title grow" id="flows-title">Flow tests</h2>
        <button type="button" class="btn btn-sm" onclick={() => (mode = 'request')}
          >Back to requests</button
        >
      </div>
      <p class="muted pad">
        A flow is a test whose <code>steps</code> call apis in order, saving values between them.
      </p>
      <ul class="flows pad" aria-label="Flow tests">
        {#each client.flows as listed (listed.id)}
          <li>
            <span class="grow"
              ><span class="mono">{listed.id}</span> · {listed.name} · {listed.steps} steps</span
            >
            <button
              type="button"
              class="btn btn-sm"
              aria-label="Edit {listed.id}"
              aria-pressed={flow?.id === listed.id}
              onclick={() => edit(listed.id)}>Edit</button
            >
            <button
              type="button"
              class="btn btn-sm"
              aria-label="Run {listed.id}"
              disabled={session.running || (flow?.id === listed.id && flow.dirty)}
              onclick={() => session.start({ selection: { ids: [listed.id] } })}>Run</button
            >
          </li>
        {:else}
          <li class="muted">No flow tests yet. Import a HAR with a flow test to record one.</li>
        {/each}
      </ul>
      {#if session.running}
        <a class="pad" href={resolve('/runs')}>See the running flow in Runs</a>
      {/if}
    </section>
    {#if flow}
      <div class="glass flow-card">
        <FlowPanel
          {flow}
          apis={client.apis}
          onsaved={(what) => {
            said = what;
            void client.load();
          }}
        />
      </div>
    {/if}
  </div>
{/if}
<p class="said" role="status">{said}</p>

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
  .error {
    margin: 0;
    color: var(--st-fail-ink);
  }
  .said {
    margin: 0;
    color: var(--ink-2);
  }
  .said:empty {
    display: none;
  }
  .flows-view {
    display: grid;
    align-content: start;
    gap: var(--sp-4);
    min-width: 0;
    overflow: auto;
  }
  .flow-card {
    padding: var(--card-pad);
  }
  .pad {
    margin: 0;
    padding: 0 var(--card-pad) var(--sp-3);
  }
  .flows {
    display: grid;
    gap: var(--sp-2);
    list-style: none;
  }
  .flows li {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
  }
</style>
