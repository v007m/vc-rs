<script lang="ts">
  import { pretty, type LazyBody } from './types.ts';

  let { title, load }: { title: string; load: LazyBody } = $props();
  // Fetched when the exchange is first opened, and again only if `load` changes.
  const text = $derived(load());
</script>

<div class="label">{title}</div>
{#await text}
  <pre class="vc-mono">…</pre>
{:then body}
  <pre class="vc-mono">{pretty(body)}</pre>
{:catch}
  <p class="label">The body could not be loaded.</p>
{/await}

<style>
  .label {
    margin: 10px 0 4px;
    font-size: 12px;
    color: var(--st-text-secondary);
  }
  pre {
    margin: 0;
    padding: 10px;
    max-height: 360px;
    overflow: auto;
    border-radius: 8px;
    background: var(--st-surface-input-on-card);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
