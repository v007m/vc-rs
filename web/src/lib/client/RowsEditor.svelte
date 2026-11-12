<script lang="ts">
  import { MASKED } from '#lib/settings/port.ts';
  import type { ApiRequest, RowList } from './request.svelte.ts';

  /**
   * The query parameters or headers of the open api, a row each. A row that is off is left out
   * of the call and the file; a value that reads a secret shows masked and cannot be edited.
   */
  let {
    request,
    list,
    noun,
    empty,
  }: { request: ApiRequest; list: RowList; noun: string; empty: string } = $props();

  const rows = $derived(request.form[list]);
</script>

<table class="rows" aria-label={`${noun}s`}>
  <thead>
    <tr>
      <th scope="col">On</th>
      <th scope="col">Name</th>
      <th scope="col">Value</th>
      <th><span class="sr">Actions</span></th>
    </tr>
  </thead>
  <tbody>
    {#each rows as row, i (i)}
      <tr>
        <td>
          <input type="checkbox" aria-label="Send {row.name || noun}" bind:checked={row.on} />
        </td>
        <td>
          <input
            class="input mono"
            autocomplete="off"
            spellcheck="false"
            aria-label="{noun} name"
            readonly={row.value.includes(MASKED)}
            bind:value={row.name}
          />
        </td>
        <td>
          {#if row.value.includes(MASKED)}
            <span class="mono masked"
              >{row.value}<span class="sr"> (reads a secret; change it in the api file)</span></span
            >
          {:else}
            <input
              class="input mono"
              autocomplete="off"
              spellcheck="false"
              aria-label="{row.name || noun} value"
              bind:value={row.value}
            />
          {/if}
        </td>
        <td>
          <button
            type="button"
            class="btn sm"
            aria-label="Remove {row.name || noun}"
            onclick={() => request.removeRow(list, i)}>Remove</button
          >
        </td>
      </tr>
    {:else}
      <tr><td colspan="4" class="empty">{empty}</td></tr>
    {/each}
  </tbody>
</table>
<button type="button" class="btn sm" onclick={() => request.addRow(list)}>Add {noun}</button>

<style>
  .rows {
    width: 100%;
    margin: 6px 0;
    border-collapse: collapse;
  }
  th {
    font-weight: 500;
    text-align: left;
    color: var(--ink2);
  }
  td {
    padding: 2px 6px 2px 0;
  }
  td .input {
    width: 100%;
  }
  .masked {
    letter-spacing: 1px;
  }
  .empty {
    color: var(--ink2);
  }
</style>
