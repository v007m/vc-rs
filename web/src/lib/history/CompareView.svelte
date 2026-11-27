<script lang="ts">
  import { ConnectError } from '@connectrpc/connect';
  import type { Maybe } from '#lib/api/maybe.ts';
  import { ms, statusClass, statusGlyph } from '#lib/results/status.ts';
  import EmptyState from '#lib/ui/EmptyState.svelte';
  import Tabs from '#lib/ui/Tabs.svelte';
  import { compareLine, compareTabs, timingMoves } from './compare.ts';
  import type { RunLine } from './port.ts';
  import {
    percentChange,
    type ComparePort,
    type RunComparison,
    type StatusChange,
    type TimingChange,
  } from './seams.ts';

  /**
   * Compare two runs (D637): pick the base and the head, then the tests newly failing, fixed,
   * slower and faster, and the response fields whose shape changed, each a tab with its count.
   */
  let {
    base,
    head,
    runs,
    port,
    onpick,
  }: {
    base: number;
    head: number;
    runs: readonly RunLine[];
    port: ComparePort;
    onpick: (base: number, head: number) => void;
  } = $props();

  const uid = $props.id();
  let result: Maybe<RunComparison> | undefined = $state.raw();
  let error: string | undefined = $state();
  let tab = $state('failing');
  const value = $derived(result?.available ? result.value : undefined);
  const moves = $derived(value ? timingMoves(value.timing) : undefined);

  $effect(() => {
    let live = true;
    result = undefined;
    error = undefined;
    port.compare(base, head).then(
      (value) => live && (result = value),
      (reason: unknown) => live && (error = ConnectError.from(reason).rawMessage),
    );
    return () => (live = false);
  });

  const label = (id: number): string => {
    const run = runs.find((r) => r.id === id);
    return run ? `#${id} · ${run.selector || 'all tests'}` : `#${id}`;
  };
</script>

<div class="pick">
  <label for="{uid}-base">Base</label>
  <select
    id="{uid}-base"
    class="st-input"
    value={base}
    onchange={(event) => onpick(Number(event.currentTarget.value), head)}
  >
    {#if !runs.some((r) => r.id === base)}<option value={base}>{label(base)}</option>{/if}
    {#each runs as run (run.id)}<option value={run.id}>{label(run.id)}</option>{/each}
  </select>
  <button
    type="button"
    class="st-icon-btn"
    aria-label="Swap base and head"
    onclick={() => onpick(head, base)}><span aria-hidden="true">⇄</span></button
  >
  <label for="{uid}-head">Head</label>
  <select
    id="{uid}-head"
    class="st-input"
    value={head}
    onchange={(event) => onpick(base, Number(event.currentTarget.value))}
  >
    {#if !runs.some((r) => r.id === head)}<option value={head}>{label(head)}</option>{/if}
    {#each runs as run (run.id)}<option value={run.id}>{label(run.id)}</option>{/each}
  </select>
</div>

{#if error}
  <EmptyState icon="columns" title="These runs could not be compared" caption={error} />
{:else if !result}
  <p class="st-secondary" role="status">Comparing #{base} and #{head}…</p>
{:else if !result.available}
  <EmptyState icon="columns" title="Compare is not available yet" caption={result.reason} />
{:else}
  <p class="st-secondary sum" role="status">{compareLine(result.value)}</p>
  <Tabs
    label="Changes from #{base} to #{head}"
    tabs={compareTabs(result.value)}
    bind:selected={tab}
  >
    {#snippet panel(id)}
      {#if id === 'failing'}
        {@render changes(
          value?.newlyFailing ?? [],
          'No test that passed in the base fails in the head.',
        )}
      {:else if id === 'fixed'}
        {@render changes(value?.fixed ?? [], 'No test that failed in the base passes in the head.')}
      {:else if id === 'shapes'}
        {@render shapes(value)}
      {:else}
        {@render timing(id === 'slower' ? (moves?.slower ?? []) : (moves?.faster ?? []))}
      {/if}
    {/snippet}
  </Tabs>
{/if}

{#snippet changes(rows: readonly StatusChange[], none: string)}
  {#if rows.length}
    <table class="vc-table">
      <thead>
        <tr
          ><th scope="col">Test</th><th scope="col">Base</th><th scope="col">Head</th><th
            scope="col">Why</th
          ></tr
        >
      </thead>
      <tbody>
        {#each rows as row (row.id)}
          <tr>
            <td class="vc-mono">{row.id}</td>
            <td
              ><span class="st-badge {statusClass[row.was]}">{statusGlyph[row.was]} {row.was}</span
              ></td
            >
            <td
              ><span class="st-badge {statusClass[row.now]}">{statusGlyph[row.now]} {row.now}</span
              ></td
            >
            <td class="st-secondary">{row.why ?? ''}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else}
    <p class="st-secondary">{none}</p>
  {/if}
{/snippet}

{#snippet timing(rows: readonly TimingChange[])}
  {#if rows.length}
    <table class="vc-table">
      <thead>
        <tr>
          <th scope="col">Test</th><th scope="col" class="r">Base</th><th scope="col" class="r"
            >Head</th
          ><th scope="col" class="r">Change</th>
        </tr>
      </thead>
      <tbody>
        {#each rows as row (row.id)}
          <tr>
            <td class="vc-mono">{row.id}</td>
            <td class="r">{ms(row.baseMs)}</td>
            <td class="r">{ms(row.headMs)}</td>
            <td class="r">{percentChange(row) > 0 ? '+' : ''}{percentChange(row)} %</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else}
    <p class="st-secondary">No test moved by 20 % and 50 ms or more.</p>
  {/if}
{/snippet}

{#snippet shapes(found: RunComparison | undefined)}
  {#if found?.shapes.length}
    <table class="vc-table">
      <thead>
        <tr><th scope="col">Api</th><th scope="col">Field</th><th scope="col">Change</th></tr>
      </thead>
      <tbody>
        {#each found.shapes as row (`${row.api} ${row.path}`)}
          <tr>
            <td class="vc-mono">{row.api}</td>
            <td class="vc-mono">{row.path}</td>
            <td>{row.change === 'type' ? `now ${row.type}` : `${row.change} (${row.type})`}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {:else}
    <p class="st-secondary">No response field changed its shape.</p>
  {/if}
{/snippet}

<style>
  .pick {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 10px;
  }
  .pick select {
    width: auto;
    min-width: 200px;
  }
  .sum {
    margin: 0;
    font-variant-numeric: tabular-nums;
  }
  .r {
    text-align: right;
  }
</style>
