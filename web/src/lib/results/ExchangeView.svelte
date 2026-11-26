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
    <span class="vc-mono">{exchange.method} {exchange.url}</span>
    <span class="st-secondary">→ {status}{time}</span>
  </summary>
  {#if open}
    {#if exchange.timing}<Waterfall timing={exchange.timing} />{/if}
    {#each parts as part (part.title)}
      {#if part.headers.length}
        <table class="vc-mono" aria-label="{part.title} headers">
          <tbody>
            {#each part.headers as [name, value], i (i)}
              <tr><th scope="row">{name}</th><td>{value}</td></tr>
            {/each}
          </tbody>
        </table>
      {/if}
      {#if part.body}<BodyView title="{part.title} body" load={part.body} />{/if}
    {/each}
    {#if exchange.messages}<BodyView title="Messages" load={exchange.messages} />{/if}
    {#if exchange.truncated}
      <p class="st-secondary">
        {exchange.messages ? 'Bodies and messages were' : 'Bodies were'} cut at history.maxBodySize.
      </p>
    {/if}
  {/if}
</details>

<style>
  .exchange {
    padding: 8px 12px;
    border-radius: var(--vc-radius-inner);
    background: var(--st-surface-wash);
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
    border-bottom: 1px solid var(--st-chrome-separator);
    text-align: left;
    vertical-align: top;
    overflow-wrap: anywhere;
  }
  th {
    font-weight: 500;
    color: var(--st-text-secondary);
    width: 35%;
  }
  .st-secondary {
    color: var(--st-text-secondary);
  }
  p {
    margin: 8px 0 0;
  }
</style>
