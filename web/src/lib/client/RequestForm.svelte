<script lang="ts">
  import { MASKED } from '#lib/settings/port.ts';
  import { METHODS, type BodyMode } from './apiForm.ts';
  import type { ApiRequest } from './request.svelte.ts';
  import RowsEditor from './RowsEditor.svelte';

  /** The open api's request as its file writes it: method, path, query, headers and body. */
  let { request }: { request: ApiRequest } = $props();

  const modes: readonly { id: BodyMode; label: string }[] = [
    { id: 'none', label: 'None' },
    { id: 'json', label: 'JSON' },
    { id: 'text', label: 'Text' },
  ];
  const inherited = $derived(request.explained?.headers ?? []);
  const masked = $derived(request.form.body.includes(MASKED));
</script>

<div class="line">
  <label class="sr" for="request-method">Method</label>
  <select id="request-method" class="input mono" bind:value={request.form.method}>
    {#each METHODS as method (method)}
      <option value={method}>{method}</option>
    {/each}
  </select>
  <label class="sr" for="request-path">Path</label>
  <input
    id="request-path"
    class="input mono grow"
    autocomplete="off"
    spellcheck="false"
    readonly={request.form.path.includes(MASKED)}
    bind:value={request.form.path}
  />
</div>

<h3 class="h3">Query parameters</h3>
<RowsEditor {request} list="query" noun="Query parameter" empty="No query parameters." />

<h3 class="h3">Headers</h3>
{#if inherited.length > 0}
  <table class="inherited" aria-label="Inherited headers">
    <caption>Added at send time. A request header with the same name overrides one.</caption>
    <thead>
      <tr>
        <th scope="col">Header</th>
        <th scope="col">Value</th>
        <th scope="col">From</th>
      </tr>
    </thead>
    <tbody>
      {#each inherited as header (header.name)}
        <tr>
          <th scope="row" class="mono">{header.name}</th>
          <td class="mono">{header.value}</td>
          <td class="mono">{header.from}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}
<RowsEditor
  {request}
  list="headers"
  noun="Header"
  empty={inherited.length
    ? 'No request headers. The inherited ones above are sent.'
    : 'No request headers.'}
/>

<h3 class="h3">Body</h3>
<div class="seg" role="group" aria-label="Body">
  {#each modes as mode (mode.id)}
    <button
      type="button"
      aria-pressed={request.form.bodyMode === mode.id}
      disabled={masked}
      onclick={() => (request.form.bodyMode = mode.id)}>{mode.label}</button
    >
  {/each}
</div>
{#if request.form.bodyMode === 'none'}
  <p class="hint">No body is sent with this request.</p>
{:else}
  <label class="sr" for="request-body"
    >{request.form.bodyMode === 'json' ? 'JSON body' : 'Text body'}</label
  >
  <textarea
    id="request-body"
    class="input mono body"
    rows="8"
    spellcheck="false"
    readonly={masked}
    aria-invalid={request.problem !== undefined}
    aria-describedby="request-body-help"
    bind:value={request.form.body}></textarea>
  <p class="hint" id="request-body-help">
    {#if request.problem}
      <span class="error">{request.problem}</span>
    {:else if masked}
      The body reads a secret, so it is shown masked; change it in the api file.
    {:else}
      Templates like <code>${'{var.name}'}</code> resolve at send time.
    {/if}
  </p>
{/if}

<style>
  .line {
    display: flex;
    gap: 8px;
  }
  .h3 {
    margin: 14px 0 4px;
    font-size: 13px;
    font-weight: 600;
  }
  .inherited {
    width: 100%;
    margin-bottom: 6px;
    border-collapse: collapse;
    color: var(--ink2);
  }
  .inherited caption {
    text-align: left;
  }
  .inherited th {
    font-weight: 500;
    text-align: left;
  }
  .body {
    width: 100%;
    margin-top: 6px;
    resize: vertical;
  }
  .hint {
    margin: 4px 0 0;
    color: var(--ink2);
  }
  .error {
    color: var(--fail);
  }
</style>
