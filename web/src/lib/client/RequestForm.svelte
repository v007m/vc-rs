<script lang="ts">
  import { MASKED } from '#lib/settings/port.ts';
  import type { BodyMode } from './apiForm.ts';
  import type { ApiRequest } from './request.svelte.ts';
  import RowsEditor from './RowsEditor.svelte';

  /**
   * One part of the open api's request as its file writes it: its query parameters, its
   * headers (with the inherited ones read-only above them) or its body. The method and path sit
   * in the request bar.
   */
  let { request, part }: { request: ApiRequest; part: 'params' | 'headers' | 'body' } = $props();

  const modes: readonly { id: BodyMode; label: string }[] = [
    { id: 'none', label: 'None' },
    { id: 'json', label: 'JSON' },
    { id: 'text', label: 'Text' },
  ];
  const inherited = $derived(request.explained?.headers ?? []);
  const masked = $derived(request.form.body.includes(MASKED));
</script>

{#if part === 'params'}
  <RowsEditor
    rows={request.form.query}
    noun="Query parameter"
    empty="No query parameters."
    onadd={() => request.addRow('query')}
    onremove={(at) => request.removeRow('query', at)}
  />
{:else if part === 'headers'}
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
    rows={request.form.headers}
    noun="Header"
    empty={inherited.length
      ? 'No request headers. The inherited ones above are sent.'
      : 'No request headers.'}
    onadd={() => request.addRow('headers')}
    onremove={(at) => request.removeRow('headers', at)}
  />
{:else}
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
      class="textarea mono body"
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
{/if}

<style>
  .inherited {
    width: 100%;
    margin-bottom: 6px;
    border-collapse: collapse;
    color: var(--ink-2);
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
    color: var(--ink-2);
  }
  .error {
    color: var(--st-fail-ink);
  }
</style>
