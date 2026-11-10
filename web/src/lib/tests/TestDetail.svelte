<script lang="ts">
  import { resolve } from '$app/paths';
  import type { TestRow } from './catalog.ts';

  let { row, running, onrun }: { row: TestRow; running: boolean; onrun: (ids: string[]) => void } =
    $props();

  let editorHref = $derived(`${resolve('/editor')}?file=${encodeURIComponent(row.path)}`);
  let where = $derived(row.line ? `${row.path}:${row.line}` : row.path);
</script>

<section class="detail" aria-labelledby="detail-h">
  <h2 id="detail-h" class="h2">{row.name}</h2>
  <p class="mono id">{row.id}</p>
  <p class="mono path">{where}</p>
  {#if row.steps}<p class="muted steps">A flow of {row.steps} steps</p>{/if}
  {#if row.skipped}
    <p class="note">This test is skipped. It stays in reports with its reason.</p>
  {/if}
  {#if row.tags.length}
    <ul class="tags" aria-label="Tags">
      {#each row.tags as tag (tag)}<li class="pill">{tag}</li>{/each}
    </ul>
  {/if}
  {#if row.calls.length}
    <h3 class="calls-h" id="calls-h">Calls</h3>
    <ol class="calls" aria-labelledby="calls-h">
      {#each row.calls as call, i (i)}
        <li>
          <span class="method mono">{call.method}</span>
          <span class="mono">{call.path}</span>
          <span class="mono api">{call.api}</span>
        </li>
      {/each}
    </ol>
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
  .steps {
    margin: 0;
  }
  .calls-h {
    margin: 8px 0 0;
    font-size: 13px;
    font-weight: 600;
  }
  .calls {
    display: grid;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: 12.5px;
  }
  .calls li {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    overflow-wrap: anywhere;
  }
  .method {
    min-width: 52px;
    font-weight: 600;
  }
  .api {
    color: var(--ink2);
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
