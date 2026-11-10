<script lang="ts">
  import { resolve } from '$app/paths';
  import type { TestRow } from './catalog.ts';

  let { row, running, onrun }: { row: TestRow; running: boolean; onrun: (ids: string[]) => void } =
    $props();

  let editorHref = $derived(`${resolve('/editor')}?file=${encodeURIComponent(row.path)}`);
</script>

<section class="detail" aria-labelledby="detail-h">
  <h2 id="detail-h" class="h2">{row.name}</h2>
  <p class="mono id">{row.id}</p>
  <p class="mono path">{row.path}</p>
  {#if row.skipped}
    <p class="note">This test is skipped. It stays in reports with its reason.</p>
  {/if}
  {#if row.tags.length}
    <ul class="tags" aria-label="Tags">
      {#each row.tags as tag (tag)}<li class="pill">{tag}</li>{/each}
    </ul>
  {/if}
  <div class="actions">
    <button class="btn primary sm" type="button" disabled={running} onclick={() => onrun([row.id])}>
      Run this test
    </button>
    <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- editorHref is resolve('/editor') plus a query -->
    <a class="btn sm" href={editorHref}>Open in editor</a>
  </div>
</section>

<style>
  .detail {
    display: grid;
    gap: 8px;
    align-content: start;
  }
  .id,
  .path {
    margin: 0;
    color: var(--ink2);
    overflow-wrap: anywhere;
  }
  .note {
    margin: 0;
    color: var(--block);
  }
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .actions {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }
</style>
