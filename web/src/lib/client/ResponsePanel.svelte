<script lang="ts">
  import CheckList from '#lib/results/CheckList.svelte';
  import Waterfall from '#lib/results/Waterfall.svelte';
  import Tabs from '#lib/ui/Tabs.svelte';
  import { headline } from './response.ts';
  import type { ClientSession } from './session.svelte.ts';

  let { client, onsave }: { client: ClientSession; onsave: () => void } = $props();

  let tab = $state('body');
  const result = $derived(client.result);
  const timing = $derived(result?.output.exchange?.timing);
  const tabs = $derived([
    { id: 'body', label: 'Body' },
    { id: 'headers', label: 'Headers' },
    ...(timing ? [{ id: 'timing', label: 'Timing' }] : []),
    { id: 'checks', label: 'Checks' },
    { id: 'saved', label: 'Saved' },
    { id: 'output', label: 'Output' },
  ]);
  /** The tabs that show what the call itself reported, with or without a response. */
  const reported = new Set(['checks', 'saved', 'output']);

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
    <Tabs
      label="Response"
      {tabs}
      bind:selected={() => (tabs.some((t) => t.id === tab) ? tab : 'body'), (id) => (tab = id)}
    >
      {#snippet panel(id)}
        {#if !result.response && !reported.has(id)}
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
        {:else if id === 'timing' && timing}
          <Waterfall {timing} />
        {:else if id === 'checks'}
          {#if result.output.checks.length}
            <CheckList checks={result.output.checks} />
          {:else}
            <p class="empty">No checks ran. Add an expect to the api to check its responses.</p>
          {/if}
        {:else if id === 'saved'}
          {#if result.output.saved.length}
            <table class="saved mono" aria-label="Saved values">
              <thead
                ><tr
                  ><th scope="col">Name</th><th scope="col">From</th><th scope="col">Value</th></tr
                ></thead
              >
              <tbody>
                {#each result.output.saved as value (value.name)}
                  <tr
                    ><th scope="row">{value.name}</th><td>{value.from}</td><td>{value.value}</td
                    ></tr
                  >
                {/each}
              </tbody>
            </table>
          {:else}
            <p class="empty">The api saves nothing from its response.</p>
          {/if}
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
  .saved {
    width: 100%;
    border-collapse: collapse;
  }
  .saved th,
  .saved td {
    padding: 4px 8px;
    border-bottom: 1px solid var(--line);
    text-align: left;
    vertical-align: top;
    overflow-wrap: anywhere;
  }
  .saved thead th {
    color: var(--ink2);
    font-weight: 500;
  }
</style>
