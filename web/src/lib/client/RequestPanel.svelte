<script lang="ts">
  import Tabs from '#lib/ui/Tabs.svelte';
  import type { ClientSession } from './session.svelte.ts';

  let { client }: { client: ClientSession } = $props();

  let tab = $state('vars');
  const tabs = [
    { id: 'vars', label: 'Vars' },
    { id: 'options', label: 'Options' },
  ];
  const declared = $derived(new Set(client.current?.vars ?? []));
</script>

<section class="request" aria-labelledby="request-title">
  {#if client.current}
    <div class="head">
      <div class="grow">
        <h2 class="h2" id="request-title">{client.current.name}</h2>
        <p class="mono id">{client.current.id}</p>
      </div>
      <button
        type="button"
        class="btn primary"
        title="Send (Ctrl+Enter or Cmd+Enter)"
        disabled={client.sending}
        onclick={() => client.send()}>{client.sending ? 'Sending…' : 'Send'}</button
      >
    </div>
    <Tabs label="Request" {tabs} bind:selected={tab}>
      {#snippet panel(id)}
        {#if id === 'vars'}
          <p class="hint">
            Empty vars keep the api's defaults. Vars are sent like <code>--var</code>.
          </p>
          <ul class="vars">
            {#each client.vars as row, i (i)}
              <li>
                {#if declared.has(row.name)}
                  <label class="mono name" for="var-{i}">{row.name}</label>
                {:else}
                  <input class="input mono name" aria-label="Var name" bind:value={row.name} />
                {/if}
                <input
                  class="input mono grow"
                  id="var-{i}"
                  aria-label={declared.has(row.name) ? undefined : 'Var value'}
                  bind:value={row.value}
                />
                {#if !declared.has(row.name)}
                  <button type="button" class="btn sm" onclick={() => client.removeVar(i)}
                    >Remove</button
                  >
                {/if}
              </li>
            {/each}
          </ul>
          <button type="button" class="btn sm" onclick={() => client.addVar()}>Add var</button>
        {:else}
          <label class="check">
            <input type="checkbox" bind:checked={client.keep} />
            Keep what the call creates (skip the api's cleanup, like <code>--keep</code>)
          </label>
        {/if}
      {/snippet}
    </Tabs>
  {:else}
    <h2 class="sr" id="request-title">Request</h2>
    <p class="empty">Pick an api to call it with the project's auth and active profile.</p>
  {/if}
</section>

<svelte:window
  onkeydown={(event) => {
    if (event.key === 'Enter' && (event.ctrlKey || event.metaKey) && client.current) {
      event.preventDefault();
      void client.send();
    }
  }}
/>

<style>
  .head {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    margin-bottom: 8px;
  }
  .id,
  .hint,
  .empty {
    margin: 2px 0 0;
    color: var(--ink2);
  }
  .vars {
    display: grid;
    gap: 6px;
    margin: 8px 0;
    padding: 0;
    list-style: none;
  }
  .vars li {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .name {
    width: 160px;
    flex: none;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
  }
</style>
