<script lang="ts">
  import { resolve } from '$app/paths';
  import { SvelteSet } from 'svelte/reactivity';
  import type { RunSession } from '#lib/run/session.svelte.ts';
  import type { SkipPort } from '#lib/skip/port.ts';
  import {
    changedSince,
    facets,
    filterKeys,
    groupByFile,
    matches,
    parseQuery,
    toggleFilter,
    type FilterContext,
    type FilterKey,
  } from './catalog.ts';
  import type { Catalog } from './catalog.svelte.ts';
  import TestDetail from './TestDetail.svelte';
  import TestTree from './TestTree.svelte';

  let { catalog, session, skips }: { catalog: Catalog; session: RunSession; skips: SkipPort } =
    $props();

  let text = $state('');
  let current: string | undefined = $state();
  const selected = new SvelteSet<string>();
  const context: FilterContext = { changed: (since) => catalog.changed.get(since) };
  let query = $derived(parseQuery(text));
  let sinces = $derived(changedSince(query));
  let finding = $derived(sinces.filter((since) => !catalog.changed.has(since)));
  let failed = $derived(
    finding.flatMap((since) => {
      const reason = catalog.changedErrors.get(since);
      return reason === undefined ? [] : [`${since}: ${reason}`];
    }),
  );
  let waiting = $derived(finding.filter((since) => !catalog.changedErrors.has(since)));
  let shown = $derived(catalog.rows.filter((row) => matches(row, query, context)));
  let groups = $derived(groupByFile(shown));
  let fileCount = $derived(groupByFile(catalog.rows).length);
  let currentRow = $derived(catalog.rows.find((row) => row.id === current));
  let facetLists = $derived([
    { key: 'service' as const, label: 'Services', items: facets(catalog.rows, (r) => [r.service]) },
    { key: 'tag' as const, label: 'Tags', items: facets(catalog.rows, (r) => r.tags) },
  ]);

  // Waits for typing to pause, so `changed:main` asks once rather than once per letter.
  $effect(() => {
    const wanted = finding;
    if (!wanted.length) return;
    const timer = setTimeout(() => {
      for (const since of wanted) void catalog.loadChanged(since);
    }, 300);
    return () => clearTimeout(timer);
  });

  function count(n: number, word: string): string {
    return `${n} ${word}${n === 1 ? '' : 's'}`;
  }

  function pressed(key: FilterKey, value: string): boolean {
    return query.filters.some((f) => f.key === key && f.value === value);
  }

  function toggle(id: string): void {
    if (!selected.delete(id)) selected.add(id);
  }

  function run(ids: string[]): void {
    void session.start({ selection: { ids } });
  }
</script>

<div class="explorer">
  <aside class="facets" aria-label="Filters">
    {#each facetLists as list (list.key)}
      <h2 class="h2">{list.label}</h2>
      <ul>
        {#each list.items as item (item.value)}
          <li>
            <button
              type="button"
              class="facet"
              aria-pressed={pressed(list.key, item.value)}
              onclick={() => (text = toggleFilter(text, list.key, item.value))}
            >
              <span class="grow">{item.value}</span><span class="count">{item.count}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/each}
  </aside>

  <section class="list" aria-labelledby="list-h">
    <h2 id="list-h" class="sr">Test list</h2>
    <p class="summary">{count(catalog.rows.length, 'test')} in {count(fileCount, 'file')}</p>
    <div class="query">
      <label class="sr" for="test-query">Filter tests with query syntax</label>
      <input
        id="test-query"
        class="input grow mono"
        type="search"
        placeholder="tag:smoke service:users"
        bind:value={text}
      />
      {#if text}<button class="btn sm" type="button" onclick={() => (text = '')}>Clear</button>{/if}
    </div>
    <p class="hint">
      Filters: {filterKeys.map((k) => `${k}:`).join(' ')} · they combine with AND; quote values with spaces
    </p>
    {#if catalog.error}
      <p class="error" role="alert">Could not load the tests: {catalog.error}</p>
    {:else if catalog.loading && !catalog.rows.length}
      <p class="empty">Loading tests…</p>
    {:else if failed.length}
      <p class="error" role="alert">Could not find the changed tests: {failed.join('; ')}</p>
    {:else if waiting.length}
      <p class="empty" role="status">Finding the tests changed since {waiting.join(', ')}…</p>
    {:else if !shown.length}
      <div class="empty">
        <p>{text ? `No test matches “${text}”.` : 'This project has no tests yet.'}</p>
        {#if text}<button class="btn sm" type="button" onclick={() => (text = '')}
            >Clear filters</button
          >{/if}
      </div>
    {:else}
      <TestTree {groups} {selected} {current} onopen={(id) => (current = id)} ontoggle={toggle} />
    {/if}
    {#if selected.size}
      <div class="selbar" role="region" aria-label="Selection">
        <span class="grow">{selected.size} selected</span>
        <button class="btn sm" type="button" onclick={() => selected.clear()}
          >Clear selection</button
        >
        <button
          class="btn primary sm"
          type="button"
          disabled={session.running}
          onclick={() => run([...selected])}
        >
          Run selected
        </button>
      </div>
    {/if}
    {#if session.running}
      <p class="started" role="status">
        A run is in progress. <a href={resolve('/runs')}>See it in Runs</a>
      </p>
    {/if}
  </section>

  {#if currentRow}
    <TestDetail
      row={currentRow}
      running={session.running}
      onrun={run}
      {skips}
      onchanged={() => void catalog.load()}
    />
  {/if}
</div>

<style>
  .explorer {
    display: grid;
    grid-template-columns: 200px minmax(0, 1fr) minmax(0, 320px);
    gap: 24px;
    align-items: start;
  }
  @media (max-width: 960px) {
    .explorer {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .facets ul {
    list-style: none;
    margin: 4px 0 16px;
    padding: 0;
  }
  .facet {
    display: flex;
    width: 100%;
    gap: 8px;
    padding: 4px 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--ink2);
    font: inherit;
    cursor: pointer;
    text-align: left;
  }
  .facet[aria-pressed='true'] {
    background: var(--accentSoft);
    color: var(--accentInk);
  }
  .count {
    color: var(--ink3);
  }
  .list {
    display: grid;
    gap: 8px;
  }
  .summary,
  .hint,
  .empty p {
    margin: 0;
    color: var(--ink2);
  }
  .hint {
    font-size: 12px;
    color: var(--ink3);
  }
  .query,
  .selbar {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .selbar {
    padding: 8px 12px;
    border-radius: 10px;
    background: var(--panel3);
  }
  .error {
    color: var(--fail);
  }
</style>
