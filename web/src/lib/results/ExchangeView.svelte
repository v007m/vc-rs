<script lang="ts">
  import BodyView from './BodyView.svelte';
  import { ms } from './status.ts';
  import Waterfall from './Waterfall.svelte';
  import type { ExchangeLine } from './types.ts';

  let { exchange }: { exchange: ExchangeLine } = $props();
  let open = $state(false);
  const status = $derived(exchange.status === undefined ? 'no response' : String(exchange.status));
  const time = $derived(exchange.ms === undefined ? '' : ` · ${ms(exchange.ms)} ms`);
  const parts = $derived([
    { title: 'Request', headers: exchange.requestHeaders, body: exchange.requestBody },
    { title: 'Response', headers: exchange.responseHeaders, body: exchange.responseBody },
  ]);
</script>

<details class="exchange" bind:open>
  <summary>
    <span class="mono">{exchange.method} {exchange.url}</span>
    <span class="muted">→ {status}{time}</span>
  </summary>
  {#if open}
    {#if exchange.timing}<Waterfall timing={exchange.timing} />{/if}
    {#each parts as part (part.title)}
      {#if part.headers.length}
        <table class="mono" aria-label="{part.title} headers">
          <tbody>
            {#each part.headers as [name, value], i (i)}
              <tr><th scope="row">{name}</th><td>{value}</td></tr>
            {/each}
          </tbody>
        </table>
      {/if}
      {#if part.body}<BodyView title="{part.title} body" load={part.body} />{/if}
    {/each}
    {#if exchange.truncated}
      <p class="muted">Bodies were cut at history.maxBodySize.</p>
    {/if}
  {/if}
</details>

<style>
  .exchange {
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 8px 12px;
    background: var(--panel);
  }
  summary {
    cursor: pointer;
    display: flex;
    flex-wrap: wrap;
    gap: 4px 8px;
    overflow-wrap: anywhere;
  }
  table {
    width: 100%;
    margin-top: 10px;
    border-collapse: collapse;
  }
  th,
  td {
    padding: 3px 6px;
    border-bottom: 1px solid var(--line);
    text-align: left;
    vertical-align: top;
    overflow-wrap: anywhere;
  }
  th {
    font-weight: 500;
    color: var(--ink2);
    width: 35%;
  }
  .muted {
    color: var(--ink2);
  }
  p {
    margin: 8px 0 0;
  }
</style>
