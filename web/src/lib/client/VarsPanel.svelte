<script lang="ts">
  import type { ClientSession } from './session.svelte.ts';

  /** The call's vars, sent like `--var`, and what the project's layers resolve each to. */
  let { client }: { client: ClientSession } = $props();

  const declared = $derived(new Set(client.current?.vars ?? []));
  const layered = $derived(client.request?.explained?.vars ?? []);
</script>

<p class="st-secondary">
  Empty vars keep the api's defaults. Vars are sent like <code>--var</code>.
</p>
<ul class="vars">
  {#each client.vars as row, i (i)}
    <li>
      {#if declared.has(row.name)}
        <label class="vc-mono name" for="var-{i}">{row.name}</label>
      {:else}
        <input class="st-input vc-mono name" aria-label="Var name" bind:value={row.name} />
      {/if}
      <input
        class="st-input vc-mono vc-grow"
        id="var-{i}"
        aria-label={declared.has(row.name) ? undefined : 'Var value'}
        bind:value={row.value}
      />
      {#if !declared.has(row.name)}
        <button
          type="button"
          class="st-btn st-btn--ghost st-btn--sm vc-btn--row-danger"
          onclick={() => client.removeVar(i)}>Remove</button
        >
      {/if}
    </li>
  {/each}
</ul>
<div>
  <button type="button" class="st-btn st-btn--sm" onclick={() => client.addVar()}>Add var</button>
</div>
{#if layered.length > 0}
  <table class="client-table readonly" aria-label="Vars from the project">
    <thead>
      <tr>
        <th scope="col">Var</th>
        <th scope="col">Resolves to</th>
        <th scope="col">Set by</th>
      </tr>
    </thead>
    <tbody>
      {#each layered as v (v.key)}
        <tr>
          <th scope="row" class="vc-mono">{v.key}</th>
          <td class="vc-mono">{v.layers[0]?.value ?? ''}</td>
          <td>{v.layers[0]?.source ?? ''}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}

<style>
  .vars {
    display: grid;
    gap: 6px;
    margin: 0;
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
</style>
