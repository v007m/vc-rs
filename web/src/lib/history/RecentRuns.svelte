<script lang="ts">
  import { doneCount, ms } from '#lib/results/status.ts';
  import { outcomeClass } from './outcome.ts';
  import type { RunLine } from './port.ts';

  /** The newest runs from the history; picking one opens it in place of the live run. */
  let {
    runs,
    current,
    onopen,
  }: { runs: readonly RunLine[]; current: number | undefined; onopen: (id: number) => void } =
    $props();

  function rate(run: RunLine): string {
    const done = doneCount(run.counts);
    return done ? `${Math.round(((run.counts.passed + run.counts.flaky) / done) * 100)}%` : '';
  }

  function meta(run: RunLine): string {
    const parts = [new Date(run.startedMs).toLocaleString()];
    if (run.durationMs !== undefined) parts.push(`${ms(run.durationMs)} ms`);
    if (run.kind !== 'run') parts.push(run.kind);
    return parts.join(' · ');
  }
</script>

<section aria-labelledby="recent-title">
  <h2 class="h2" id="recent-title">Recent runs</h2>
  <ul class="runs" aria-label="Recent runs">
    {#each runs as run (run.id)}
      <li>
        <button
          type="button"
          class="run"
          aria-pressed={current === run.id}
          aria-label="Run #{run.id}, {run.outcome}"
          onclick={() => onopen(run.id)}
        >
          <span class="dot {outcomeClass[run.outcome]}" aria-hidden="true"></span>
          <span class="what">
            <span class="mono">#{run.id}</span>
            {#if run.profiles.length}· {run.profiles.join(', ')}{/if}
            <span class="muted meta">{meta(run)}</span>
          </span>
          <span class="muted">{rate(run)}</span>
        </button>
      </li>
    {:else}
      <li class="muted">No runs in the history yet.</li>
    {/each}
  </ul>
</section>

<style>
  .runs {
    display: grid;
    gap: 2px;
    margin: 6px 0 0;
    padding: 0;
    list-style: none;
  }
  .run {
    display: flex;
    align-items: baseline;
    gap: 8px;
    width: 100%;
    padding: 6px 8px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .run:hover,
  .run[aria-pressed='true'] {
    background: var(--hover, rgb(0 0 0 / 6%));
  }
  .what {
    flex: 1;
    min-width: 0;
  }
  .meta {
    display: block;
    font-size: 12px;
  }
</style>
