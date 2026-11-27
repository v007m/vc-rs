<script lang="ts">
  import { blocks, type Span } from './markdown.ts';

  /** The engine's Markdown (an explain page): text only, so nothing in it becomes markup. */
  let { text }: { text: string } = $props();

  const shown = $derived(blocks(text));
</script>

{#snippet runs(items: readonly Span[])}{#each items as run, i (i)}{#if run.code}<code
        class="vc-mono">{run.text}</code
      >{:else}{run.text}{/if}{/each}{/snippet}

<div class="md">
  {#each shown as item, i (i)}
    {#if item.kind === 'heading'}
      {#if item.level === 1}<h3>{@render runs(item.spans)}</h3>
      {:else}<h4>{@render runs(item.spans)}</h4>{/if}
    {:else if item.kind === 'paragraph'}
      <p>{@render runs(item.spans)}</p>
    {:else if item.kind === 'list'}
      <ul>
        {#each item.items as entry, j (j)}<li>{@render runs(entry)}</li>{/each}
      </ul>
    {:else}
      <pre class="vc-mono"><code>{item.text}</code></pre>
    {/if}
  {/each}
</div>

<style>
  .md {
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 13px;
    line-height: 1.5;
  }
  h3,
  h4,
  p,
  ul,
  pre {
    margin: 0;
  }
  h3 {
    font: var(--st-type-title3);
  }
  h4 {
    font-size: 13px;
    font-weight: 600;
  }
  ul {
    padding-left: 18px;
  }
  pre {
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--st-surface-wash);
    font-size: 12.5px;
    overflow: auto;
    white-space: pre;
  }
  code {
    font-size: 12.5px;
  }
</style>
