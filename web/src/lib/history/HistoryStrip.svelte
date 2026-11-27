<script lang="ts">
  import type { Maybe } from '#lib/api/maybe.ts';
  import { statusClass } from '#lib/results/status.ts';
  import { span } from './links.ts';
  import type { HistoryPoint, TestHistoryPort } from './seams.ts';
  import { sparkline, STRIP, summarize } from './strip.ts';

  /**
   * A test's last results across the history (D525, ui.md §5): one mark per run, newest on the
   * right, a duration sparkline under them, and the flake rate. Each mark names its run.
   */
  let { id, port }: { id: string; port: TestHistoryPort } = $props();

  let points: Maybe<readonly HistoryPoint[]> | undefined = $state.raw();
  let width = $state(0);

  $effect(() => {
    let live = true;
    points = undefined;
    void port.history(id, STRIP).then(
      (got) => {
        if (live) points = got;
      },
      () => {
        if (live) points = { available: false, reason: 'The history could not be read.' };
      },
    );
    return () => {
      live = false;
    };
  });

  const shown = $derived(points?.available ? [...points.value].reverse() : []);
  const sum = $derived(summarize(shown));
</script>

<section class="strip" aria-label="History of {id}">
  <h4 class="st-callout">History</h4>
  {#if !points}
    <p class="st-secondary">Reading the history…</p>
  {:else if !points.available}
    <p class="st-secondary">{points.reason}</p>
  {:else if !shown.length}
    <p class="st-secondary">This test has no results in the history yet.</p>
  {:else}
    <p class="st-secondary sum">
      Last {sum.results} results · {sum.passRate}% passed · {sum.flakeRate}% flaky · median {span(
        sum.medianMs,
      )}
    </p>
    <ol class="marks">
      {#each shown as point (point.run)}
        <li
          class="st-dot {statusClass[point.status]}"
          title="#{point.run} · {point.status} · {span(point.ms)}"
        >
          <span class="vc-sr">Run #{point.run}: {point.status}, {span(point.ms)}</span>
        </li>
      {/each}
    </ol>
    <div class="spark" bind:clientWidth={width} aria-hidden="true">
      <svg viewBox="0 0 {Math.max(1, width)} 28" width={Math.max(1, width)} height="28">
        <path d={sparkline([...shown].reverse(), [Math.max(1, width), 28])} />
      </svg>
    </div>
  {/if}
</section>

<style>
  .strip {
    display: grid;
    gap: 6px;
  }
  h4,
  p {
    margin: 0;
  }
  .sum {
    font-variant-numeric: tabular-nums;
  }
  .marks {
    display: flex;
    justify-content: space-between;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .marks li {
    flex: 1 1 0;
    max-width: 14px;
    height: 14px;
    border-radius: 3px;
  }
  .spark svg {
    display: block;
  }
  .spark path {
    fill: none;
    stroke: var(--st-text-secondary);
    stroke-width: 1.5;
    stroke-linejoin: round;
  }
</style>
