<script lang="ts">
  import Playground from './Playground.svelte';
  import { examples } from './playground/examples';

  let current = $state(examples[0].id);
  const active = $derived(examples.find((e) => e.id === current) ?? examples[0]);

  function inlineCode(text: string): string {
    const escaped = text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
    return escaped.replace(/`([^`]+)`/g, '<code>$1</code>');
  }
</script>

<div class="pgf not-content">
  <div class="pgf-tabs" role="tablist" aria-label="Starter examples">
    {#each examples as ex (ex.id)}
      <button
        type="button"
        role="tab"
        aria-selected={ex.id === current}
        class:active={ex.id === current}
        onclick={() => (current = ex.id)}
      >
        <span class="pgf-kind">{ex.kind}</span>{ex.label}
      </button>
    {/each}
  </div>
  <p class="pgf-blurb">{@html inlineCode(active.blurb)}</p>
  {#key active.id}
    <Playground kind={active.kind} code={active.code} title={active.file} height={460} />
  {/key}
</div>

<style>
  .pgf-tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }
  .pgf-tabs button {
    font: inherit;
    font-size: 0.85rem;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    border: 1px solid var(--sl-color-gray-5);
    background: var(--sl-color-bg);
    color: var(--sl-color-gray-2);
    border-radius: 999px;
    padding: 0.3rem 0.8rem;
    min-height: 2.25rem;
  }
  .pgf-tabs button:hover {
    border-color: var(--sl-color-accent);
  }
  .pgf-tabs button.active {
    background: var(--sl-color-accent-low);
    border-color: var(--sl-color-accent);
    color: var(--sl-color-accent-high);
  }
  .pgf-kind {
    font-family: var(--sl-font-mono);
    font-size: 0.65rem;
    text-transform: uppercase;
    opacity: 0.75;
  }
  .pgf-blurb :global(code) {
    font-family: var(--sl-font-mono);
    font-size: 0.85em;
    background: var(--sl-color-gray-6);
    padding: 0.05em 0.3em;
    border-radius: 0.25em;
  }
  .pgf-blurb {
    margin: 1rem 0 0;
    color: var(--sl-color-gray-2);
  }
</style>
