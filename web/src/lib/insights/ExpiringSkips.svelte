<script lang="ts">
  import type { SkipPort } from '#lib/skip/port.ts';
  import SkipForm from '#lib/skip/SkipForm.svelte';
  import { SkipChange } from '#lib/skip/skip.svelte.ts';
  import type { SkipLine } from './port.ts';
  import { expiring } from './stats.ts';

  /**
   * The skips that end within two weeks, or have ended and only wait to be removed: once its
   * date has passed a test runs again and `vc check` warns. Each can be unskipped here.
   */
  let {
    skips,
    today,
    port,
    onchanged,
  }: { skips: readonly SkipLine[]; today: string; port: SkipPort; onchanged: () => void } =
    $props();

  let removing: SkipChange | undefined = $state();
  const soon = $derived(expiring(skips, today));

  function toggle(id: string): void {
    removing = removing?.target === id ? undefined : new SkipChange(port, id, true);
  }
</script>

<section aria-labelledby="expiring" class="expiring">
  <h2 class="h2" id="expiring">Skips expiring soon</h2>
  <p class="muted">Skipped tests whose <code>until</code> date is within 14 days or has passed</p>
  {#if soon.length === 0}
    <p class="muted">No skip ends in the next 14 days.</p>
  {:else}
    <ol>
      {#each soon as skip (skip.id)}
        {@const ended = skip.until < today}
        <li>
          <span class="mono">{skip.id}</span>
          <span class="pill {ended ? 'fail' : 'block'}"
            >{ended ? `ended ${skip.until}` : `until ${skip.until}`}</span
          >
          {#if skip.reason}<span class="muted">{skip.reason}</span>{/if}
          <button
            type="button"
            class="btn sm"
            aria-label="Unskip {skip.id}"
            aria-expanded={removing?.target === skip.id}
            onclick={() => toggle(skip.id)}>Unskip…</button
          >
          {#if removing?.target === skip.id}
            {@const change = removing}
            <SkipForm
              {change}
              oncancel={() => (removing = undefined)}
              onwritten={() => {
                removing = undefined;
                onchanged();
              }}
            />
          {/if}
        </li>
      {/each}
    </ol>
  {/if}
</section>

<style>
  .expiring ol {
    display: grid;
    gap: 6px;
    margin: 8px 0 0;
    padding: 0;
    list-style: none;
  }
  .expiring li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 12px;
  }
  .expiring p {
    margin: 4px 0 0;
  }
  .btn {
    margin-left: auto;
  }
  .muted {
    color: var(--ink2);
  }
</style>
