<script lang="ts">
  import { levelFilters, visibleLines, type EventLine, type Level } from './events.ts';

  let { events, dropped = 0 }: { events: readonly EventLine[]; dropped?: number } = $props();
  let min: Level = $state('info');
  let text = $state('');
  let shown = $derived(visibleLines(events, min, text));
  const icon: Record<Level, string> = { info: 'dot', warn: 'dot block', error: 'dot fail' };
</script>

<section class="events" aria-labelledby="events-h">
  <div class="sechead">
    <h2 class="h2" id="events-h">Events</h2>
    <span class="muted"
      >{shown.length} of {events.length}{dropped ? ` · ${dropped} older not kept` : ''}</span
    >
  </div>
  <div class="evtools">
    <div class="seg" role="group" aria-label="Level">
      {#each levelFilters as f (f.min)}
        <button type="button" aria-pressed={min === f.min} onclick={() => (min = f.min)}>
          {f.label}
        </button>
      {/each}
    </div>
    <label class="filter">
      <span class="sr">Filter events</span>
      <input class="input" type="search" placeholder="Filter events" bind:value={text} />
    </label>
  </div>
  <div class="evlist" role="log" aria-label="Run events, newest first">
    {#each shown as line (line.seq)}
      <div class="ev {line.level}">
        <span class="tm">#{line.seq}</span>
        <span class={icon[line.level]} aria-label={line.level}></span>
        <span><span class="who">{line.who}</span> <span class="msg">{line.message}</span></span>
      </div>
    {:else}
      <p class="muted none">
        {events.length ? 'No events match these filters.' : 'No events yet.'}
      </p>
    {/each}
  </div>
</section>

<style>
  .events {
    display: flex;
    flex-direction: column;
    min-width: 0;
    border-top: 1px solid var(--line);
  }
  .sechead {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    padding: 14px 20px 10px;
  }
  .evtools {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    padding: 0 16px 12px;
    border-bottom: 1px solid var(--line);
  }
  .filter {
    flex: 1 1 140px;
    min-width: 0;
  }
  .filter .input {
    height: 30px;
    font-size: 12.5px;
  }
  .evlist {
    max-height: 560px;
    overflow: auto;
  }
  .ev {
    display: grid;
    grid-template-columns: 58px 16px minmax(0, 1fr);
    gap: 8px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--line);
    font-size: 12.5px;
    align-items: start;
    line-height: 1.45;
  }
  .ev.error {
    background: #fffaf9;
  }
  .ev .dot {
    margin-top: 5px;
    background: var(--ink3);
  }
  .ev .dot.block {
    background: var(--block);
  }
  .ev .dot.fail {
    background: var(--fail);
  }
  .tm {
    font: 11.5px var(--mono);
    color: var(--ink2);
  }
  .who {
    font: 500 12px var(--mono);
    word-break: break-all;
  }
  .none {
    margin: 0;
    padding: 16px 20px;
  }
</style>
