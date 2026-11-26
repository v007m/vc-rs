<script lang="ts">
  import Tabs from '#lib/ui/Tabs.svelte';
  import RowsEditor from './RowsEditor.svelte';
  import type { ScratchTab } from './scratch.svelte.ts';

  /**
   * The request card of a scratch tab (D539): a request with no api. Its URL and type sit in
   * the request bar; its params, headers and body here. A stream sends no body; a WebSocket
   * sends its messages from the stream card.
   */
  let { tab }: { tab: ScratchTab } = $props();

  let part = $state('params');
  const tabs = $derived([
    { id: 'params', label: 'Params', ...count(tab.query.length) },
    { id: 'headers', label: 'Headers', ...count(tab.headers.length) },
    ...(tab.type === 'http' ? [{ id: 'body', label: 'Body' }] : []),
  ]);

  function count(n: number): { count?: string } {
    return n ? { count: String(n) } : {};
  }
</script>

<section class="glass" aria-labelledby="scratch-title">
  <div class="card-head">
    <span class="method" data-m={tab.badge}>{tab.badge}</span>
    <h2 class="card-title grow" id="scratch-title">
      <label class="sr" for="scratch-name">Tab name</label>
      <input id="scratch-name" class="title-input" autocomplete="off" bind:value={tab.title} />
    </h2>
  </div>
  {#if tab.unavailable}
    <p class="note" role="status">{tab.unavailable}</p>
  {:else if tab.error}
    <p class="note error" role="alert">{tab.error}</p>
  {/if}
  <Tabs
    label="Request"
    {tabs}
    bind:selected={() => (tabs.some((t) => t.id === part) ? part : 'params'), (id) => (part = id)}
  >
    {#snippet panel(id)}
      {#if id === 'params'}
        <RowsEditor
          rows={tab.query}
          noun="Query parameter"
          empty="No query parameters."
          onadd={() => tab.addRow('query')}
          onremove={(at) => tab.removeRow('query', at)}
        />
      {:else if id === 'headers'}
        <RowsEditor
          rows={tab.headers}
          noun="Header"
          empty="No headers. The profile's headers and auth are added at send time."
          onadd={() => tab.addRow('headers')}
          onremove={(at) => tab.removeRow('headers', at)}
        />
      {:else}
        <label class="sr" for="scratch-body">Body</label>
        <textarea
          id="scratch-body"
          class="textarea mono"
          rows="10"
          spellcheck="false"
          placeholder={'{ "name": "${userName}" }'}
          bind:value={tab.body}></textarea>
        <p class="muted">Templates like <code>${'{var.name}'}</code> resolve at send time.</p>
      {/if}
    {/snippet}
  </Tabs>
</section>

<style>
  .title-input {
    width: 100%;
    min-width: 0;
    padding: 0;
    border: 0;
    border-radius: var(--r-xs);
    background: transparent;
    color: inherit;
    font: inherit;
    letter-spacing: inherit;
  }
  .title-input:hover {
    background: var(--row-hover);
  }
  .note {
    margin: 0 var(--card-pad) var(--sp-3);
    color: var(--ink-2);
  }
  .error {
    color: var(--st-fail-ink);
  }
  textarea {
    width: 100%;
    resize: vertical;
  }
</style>
