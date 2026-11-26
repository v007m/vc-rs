<script lang="ts">
  import { LOOPS, type Stage } from './flowForm.ts';

  /**
   * How often one stage runs: once, `repeat` N times, once per `forEach` item, or `poll` until
   * a status. A loop the form cannot show is left as its file writes it.
   */
  let { stage, n }: { stage: Stage; n: number } = $props();
</script>

{#if stage.locked}
  <span class="vc-mono">{stage.loop.kind}</span>
  <span class="hint">As written in the file; change it in the editor.</span>
{:else}
  <div class="loop">
    <select class="st-input" aria-label="Stage {n} runs" bind:value={stage.loop.kind}>
      {#each LOOPS as loop (loop.id)}
        <option value={loop.id}>{loop.label}</option>
      {/each}
    </select>
    {#if stage.loop.kind === 'repeat'}
      <input
        class="st-input vc-mono short"
        inputmode="numeric"
        aria-label="Stage {n} times"
        bind:value={stage.loop.times}
      />
    {:else if stage.loop.kind === 'forEach'}
      <input
        class="st-input vc-mono"
        spellcheck="false"
        aria-label="Stage {n} items"
        placeholder={'${var.ids}'}
        bind:value={stage.loop.over}
      />
    {:else if stage.loop.kind === 'poll'}
      <input
        class="st-input vc-mono short"
        inputmode="numeric"
        aria-label="Stage {n} until status"
        bind:value={stage.loop.until}
      />
      <input
        class="st-input vc-mono short"
        aria-label="Stage {n} every"
        bind:value={stage.loop.every}
      />
      <input
        class="st-input vc-mono short"
        aria-label="Stage {n} timeout"
        bind:value={stage.loop.timeout}
      />
    {/if}
  </div>
{/if}

<style>
  .loop {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .short {
    width: 5em;
  }
  .hint {
    display: block;
    color: var(--st-text-secondary);
  }
</style>
