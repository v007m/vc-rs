<script lang="ts">
  import { hostWarning } from './view.ts';
  import type { HostStats } from './port.ts';

  /**
   * The load generator's own state at the newest tick (D639, `load_tick.host`): CPU, memory,
   * open connections and dropped iterations, with a warning when the generator is the limit.
   */
  let { host }: { host: HostStats | undefined } = $props();

  const uid = $props.id();
  const warning = $derived(host ? hostWarning(host) : undefined);
</script>

<section class="hosts st-card" aria-labelledby="{uid}-t">
  <h3 class="st-title3" id="{uid}-t">Hosts</h3>
  <p class="st-secondary sub">The load generator on this machine</p>
  {#if host}
    <dl>
      <div>
        <dt>CPU</dt>
        <dd>{Math.round(host.cpu)} %</dd>
      </div>
      <div>
        <dt>Memory</dt>
        <dd>{Math.round(host.memMb).toLocaleString('en-US')} MB</dd>
      </div>
      <div>
        <dt>Connections</dt>
        <dd>{host.conns.toLocaleString('en-US')}</dd>
      </div>
      <div>
        <dt>Dropped</dt>
        <dd class:bad={host.dropped > 0}>{host.dropped.toLocaleString('en-US')}</dd>
      </div>
    </dl>
    {#if warning}<p class="warn" role="note"><span aria-hidden="true">⚠</span> {warning}</p>{/if}
  {:else}
    <p class="st-secondary">Waiting for the first tick.</p>
  {/if}
</section>

<style>
  .hosts {
    display: grid;
    gap: 8px;
    align-content: start;
    padding: 14px 16px;
  }
  h3,
  .sub {
    margin: 0;
  }
  dl {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px 16px;
    margin: 0;
  }
  dt {
    font: var(--st-type-caption);
    color: var(--st-text-tertiary);
  }
  dd {
    margin: 2px 0 0;
    font-size: var(--vc-size-callout);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .bad {
    color: var(--vc-fail-ink);
  }
  .warn {
    margin: 0;
    padding: 8px 10px;
    border-radius: var(--vc-radius-inner);
    background: var(--vc-flaky-soft);
    color: var(--vc-flaky-ink);
    font-size: var(--vc-size-caption);
  }
</style>
