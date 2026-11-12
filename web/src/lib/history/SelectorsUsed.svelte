<script lang="ts">
  import type { RunHistory } from './history.svelte.ts';

  /**
   * The selectors the recent runs used, each a filter on the list of them. The history keeps a
   * selector as one line that may drop parts of it, so it filters runs and is never run again.
   */
  let { history }: { history: RunHistory } = $props();
</script>

{#if history.selectors.length > 1}
  <section aria-labelledby="selectors-title">
    <h2 class="h2" id="selectors-title">Selectors used</h2>
    <ul class="selectors" aria-label="Selectors used">
      {#each history.selectors as use (use.selector)}
        {@const name = use.selector || 'all tests'}
        <li>
          <button
            type="button"
            class="selector"
            aria-pressed={history.selector === use.selector}
            aria-label="{name}, {use.runs} {use.runs === 1 ? 'run' : 'runs'}"
            onclick={() => history.filter(use.selector)}
          >
            <span class="what" class:mono={use.selector !== ''}>{name}</span>
            <span class="muted">{use.runs}</span>
          </button>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  .selectors {
    display: grid;
    gap: 2px;
    margin: 6px 0 16px;
    padding: 0;
    list-style: none;
  }
  .selector {
    display: flex;
    align-items: baseline;
    gap: 8px;
    width: 100%;
    padding: 4px 8px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .selector:hover,
  .selector[aria-pressed='true'] {
    background: var(--hover, rgb(0 0 0 / 6%));
  }
  .what {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
</style>
