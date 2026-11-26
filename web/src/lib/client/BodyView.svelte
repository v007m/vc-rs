<script lang="ts">
  import type { AddedCheck } from './apiForm.ts';
  import { filterBody, jsonRows } from './json.ts';
  import JsonTree from './JsonTree.svelte';
  import type { CallResponse } from './response.ts';

  /**
   * A response body three ways (D560): a tree to fold, filter and check; the raw text; and a
   * preview, where an HTML body renders in a sandbox with scripts and forms off.
   */
  let {
    response,
    oncheck,
  }: {
    response: CallResponse;
    oncheck?: (check: AddedCheck) => string | undefined;
  } = $props();

  type View = 'tree' | 'raw' | 'preview';
  const views: readonly { id: View; label: string }[] = [
    { id: 'tree', label: 'Tree' },
    { id: 'raw', label: 'Raw' },
    { id: 'preview', label: 'Preview' },
  ];
  let view: View = $state('tree');
  let query = $state('');
  const body = $derived(response.body);
  const filter = $derived(filterBody(jsonRows(body), query));
  const contentType = $derived(
    response.headers.find(([name]) => name.toLowerCase() === 'content-type')?.[1] ?? '',
  );
  const html = $derived(
    typeof body === 'string' && contentType.includes('html') ? body : undefined,
  );

  function raw(value: unknown): string {
    return typeof value === 'string' ? value : JSON.stringify(value, null, 2);
  }
</script>

<div class="body-tools">
  <div class="st-segmented" role="group" aria-label="Body view">
    {#each views as v (v.id)}
      <button type="button" aria-pressed={view === v.id} onclick={() => (view = v.id)}
        >{v.label}</button
      >
    {/each}
  </div>
  {#if view === 'tree'}
    <input
      class="st-input"
      type="search"
      autocomplete="off"
      spellcheck="false"
      aria-label="Filter the body with a JSONPath"
      placeholder="Filter: $.orders[*].id"
      bind:value={query}
    />
  {/if}
</div>

<div class="body-view vc-well">
  {#if view === 'raw'}
    <pre class="raw vc-mono">{raw(body)}</pre>
  {:else if view === 'preview'}
    {#if html !== undefined}
      <iframe class="preview" title="HTML preview" sandbox="" srcdoc={html}></iframe>
    {:else}
      <p class="note">
        Preview renders HTML responses, sandboxed with scripts off. This one is not HTML.
      </p>
    {/if}
  {:else}
    {#if filter && 'cel' in filter}
      <p class="note" role="status">
        That reads as CEL, which the engine evaluates; filter with a JSONPath such as
        <code>$.items[*].id</code> here.
      </p>
    {:else if filter}
      <p class="note" role="status">
        <b>{filter.exact.size}</b>
        {filter.exact.size === 1 ? 'match' : 'matches'} for <b class="vc-mono">{query.trim()}</b>
      </p>
    {/if}
    <JsonTree {body} {filter} {oncheck} />
  {/if}
</div>

<style>
  .raw {
    margin: 0;
    padding: var(--st-space-12) var(--st-space-16);
    font-size: var(--vc-size-body);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .preview {
    display: block;
    width: 100%;
    min-height: 320px;
    border: 0;
    background: white;
  }
  .note {
    margin: 0;
    padding: var(--st-space-12) var(--st-space-16) 0;
    color: var(--st-text-secondary);
    font-size: var(--vc-size-caption);
  }
</style>
