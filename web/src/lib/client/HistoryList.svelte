<script lang="ts">
  import { resolve } from '$app/paths';
  import Icon from '#lib/shell/Icon.svelte';
  import { byDay, clock, type HistoryDay } from './days.ts';
  import type { HistoryEntry, HistorySource } from './seams.ts';
  import { message } from './tab.svelte.ts';
  import { statusClass } from './wire.ts';

  /**
   * The Client's recent calls by day (D565). A call with a method opens again as a scratch
   * tab; an engine that keeps no history says so in one quiet line.
   */
  let {
    source,
    stamp = 0,
    onreopen,
  }: {
    source: HistorySource;
    /** Changes after each send, so the list reads again. */
    stamp?: number;
    onreopen: (entry: Readonly<{ method: string; url: string }>) => void;
  } = $props();

  let days: HistoryDay[] = $state([]);
  let quiet: string | undefined = $state();

  async function load(): Promise<void> {
    try {
      const answer = await source.recent(50);
      quiet = answer.available ? undefined : answer.reason;
      days = answer.available ? byDay(answer.value, Date.now()) : [];
    } catch (error) {
      quiet = message(error);
    }
  }

  $effect(() => {
    void stamp;
    void load();
  });

  function reopen(entry: HistoryEntry): void {
    if (entry.method) onreopen({ method: entry.method, url: entry.label });
  }
</script>

{#if quiet}
  <p class="hist-quiet">{quiet}</p>
{:else if days.length === 0}
  <p class="hist-quiet">No calls yet. A request sent here shows up in the history.</p>
{/if}
{#each days as day (day.label)}
  <p class="sb-label">{day.label}</p>
  <ul class="sb-nav" aria-label="Calls {day.label}">
    {#each day.entries as entry (entry.id)}
      <li>
        <button
          type="button"
          class="sb-row hist-row"
          disabled={!entry.method}
          title={entry.label}
          onclick={() => reopen(entry)}
        >
          <span class="method" data-m={entry.method}>{entry.method ?? 'RUN'}</span>
          <span class="title vc-mono">{entry.label}</span>
          {#if entry.status}
            <span class="hs" data-class={statusClass(entry.status)}>{entry.status}</span>
          {:else}
            <span class="meta">{entry.outcome}</span>
          {/if}
          <span class="meta">{clock(entry.startedMs)}</span>
        </button>
      </li>
    {/each}
  </ul>
{/each}
<a class="sb-row hist-all" href={resolve('/runs')}><Icon name="history" small />All history</a>

<style>
  .hist-quiet {
    margin: 0;
    padding: var(--st-space-4) var(--st-space-12) var(--st-space-8);
    color: var(--st-text-secondary);
    font-size: var(--vc-size-caption);
  }
  .hist-row {
    width: 100%;
    border: 0;
    background: transparent;
    text-align: left;
  }
  .hist-row .title {
    font-size: var(--vc-size-caption);
  }
  .hist-row .hs {
    font-size: var(--vc-size-caption);
  }
  .hist-row:disabled {
    cursor: default;
  }
  .hist-all {
    margin-top: var(--st-space-8);
    color: var(--st-text-secondary);
  }
</style>
