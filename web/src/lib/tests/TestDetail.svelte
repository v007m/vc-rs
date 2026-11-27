<script lang="ts">
  import { resolve } from '$app/paths';
  import type { SkipPort } from '#lib/skip/port.ts';
  import SkipForm from '#lib/skip/SkipForm.svelte';
  import { SkipChange } from '#lib/skip/skip.svelte.ts';
  import type { TestRow } from './catalog.ts';

  let {
    row,
    running,
    onrun,
    skips,
    onchanged,
  }: {
    row: TestRow;
    running: boolean;
    onrun: (ids: string[]) => void;
    skips: SkipPort;
    /** After a skip is written or removed, so the list is read again. */
    onchanged: () => void;
  } = $props();

  let skipping: SkipChange | undefined = $state();
  // Another test opened: its skip form starts closed.
  $effect.pre(() => {
    if (skipping && skipping.target !== row.id) skipping = undefined;
  });

  let editorHref = $derived(`${resolve('/tests/editor')}?file=${encodeURIComponent(row.path)}`);
  let where = $derived(row.line ? `${row.path}:${row.line}` : row.path);
</script>

<section class="detail" aria-labelledby="detail-h">
  <h2 id="detail-h" class="st-title3">{row.name}</h2>
  <p class="vc-mono id">{row.id}</p>
  <p class="vc-mono path">{where}</p>
  {#if row.steps}<p class="st-secondary steps">A flow of {row.steps} steps</p>{/if}
  {#if row.skipped}
    <p class="note">This test is skipped. It stays in reports with its reason.</p>
  {/if}
  {#if row.tags.length}
    <ul class="tags" aria-label="Tags">
      {#each row.tags as tag (tag)}<li class="st-badge">{tag}</li>{/each}
    </ul>
  {/if}
  {#if row.calls.length}
    <h3 class="calls-h" id="calls-h">Calls</h3>
    <ol class="calls" aria-labelledby="calls-h">
      {#each row.calls as call, i (i)}
        <li>
          <span class="method vc-mono">{call.method}</span>
          <span class="vc-mono">{call.path}</span>
          <span class="vc-mono api">{call.api}</span>
        </li>
      {/each}
    </ol>
  {/if}
  <div class="actions">
    <button
      class="st-btn st-btn--primary st-btn--sm"
      type="button"
      disabled={running}
      onclick={() => onrun([row.id])}
    >
      Run this test
    </button>
    <!-- eslint-disable-next-line svelte/no-navigation-without-resolve -- editorHref is resolve('/tests/editor') plus a query -->
    <a class="st-btn st-btn--sm" href={editorHref}>Open in editor</a>
    <button
      class="st-btn st-btn--sm"
      type="button"
      aria-expanded={skipping !== undefined}
      onclick={() => (skipping = skipping ? undefined : new SkipChange(skips, row.id, row.skipped))}
      >{row.skipped ? 'Unskip…' : 'Skip…'}</button
    >
  </div>
  {#if skipping}
    <SkipForm
      change={skipping}
      oncancel={() => (skipping = undefined)}
      onwritten={() => {
        skipping = undefined;
        onchanged();
      }}
    />
  {/if}
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
    color: var(--st-text-secondary);
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
    color: var(--st-text-secondary);
  }
  .note {
    margin: 0;
    color: var(--vc-blocked);
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
