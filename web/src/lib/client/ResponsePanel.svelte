<script lang="ts">
  import Tabs from '#lib/ui/Tabs.svelte';
  import { headline } from './response.ts';
  import type { ClientSession } from './session.svelte.ts';

  let { client, onsave }: { client: ClientSession; onsave: () => void } = $props();

  let tab = $state('body');
  const tabs = [
    { id: 'body', label: 'Body' },
    { id: 'headers', label: 'Headers' },
    { id: 'output', label: 'Output' },
  ];
  const result = $derived(client.result);

  function pretty(body: unknown): string {
    return typeof body === 'string' ? body : JSON.stringify(body, null, 2);
  }
</script>

<section class="response" aria-labelledby="response-title">
  <div class="head">
    <h2 class="h2 grow" id="response-title">Response</h2>
    {#if result}
      <button type="button" class="btn sm" disabled={!result.response} onclick={onsave}
        >Save as test</button
      >
    {/if}
  </div>
  {#if !result}
    <p class="empty">Send the request to see its response.</p>
  {:else}
    <p class="status">
      <span class="dot {result.output.exit === 0 ? 'pass' : 'fail'}" aria-hidden="true"></span>
      <span class="mono">{headline(result.output.text)}</span>
      <span class="sr">{result.output.exit === 0 ? 'Expect held' : 'Expect failed'}</span>
    </p>
    <Tabs label="Response" {tabs} bind:selected={tab}>
      {#snippet panel(id)}
        {#if !result.response && id !== 'output'}
          <p class="empty">No response came back; the Output tab says why.</p>
        {:else if id === 'body' && result.response}
          <pre class="mono code">{pretty(result.response.body)}</pre>
        {:else if id === 'headers' && result.response}
          <dl class="headers mono">
            {#each result.response.headers as [name, value] (name)}
              <dt>{name}</dt>
              <dd>{value}</dd>
            {/each}
          </dl>
        {:else}
          <pre class="mono code">{result.output.text}</pre>
          {#each result.output.notices as notice, i (i)}
            <p class="notice">{notice}</p>
          {/each}
        {/if}
      {/snippet}
    </Tabs>
  {/if}
</section>

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 8px 0;
  }
  .empty,
  .notice {
    color: var(--ink2);
  }
  .code {
    max-height: 420px;
    margin: 0;
    overflow: auto;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .headers {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px 16px;
    margin: 0;
  }
  .headers dt {
    color: var(--ink2);
  }
  .headers dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
