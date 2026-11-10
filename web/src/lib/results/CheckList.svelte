<script lang="ts">
  import type { CheckLine } from './types.ts';

  let { checks }: { checks: readonly CheckLine[] } = $props();
</script>

<ul class="checks" aria-label="Checks">
  {#each checks as check, i (i)}
    <li class:failed={!check.passed}>
      <span class="mark" aria-hidden="true">{check.passed ? '✓' : '✗'}</span>
      <div class="what">
        <code class="mono">{check.label}</code>
        <span class="sr">{check.passed ? ' held' : ' failed'}</span>
        {#if !check.passed}
          <div class="value">expected <code class="mono">{check.expected}</code></div>
          {#if check.actual !== undefined}
            <div class="value">actual <code class="mono">{check.actual}</code></div>
          {/if}
        {/if}
        {#if check.location}<div class="loc mono">{check.location}</div>{/if}
      </div>
    </li>
  {/each}
</ul>

<style>
  .checks {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 6px;
  }
  li {
    display: grid;
    grid-template-columns: 16px minmax(0, 1fr);
    gap: 8px;
    padding: 8px 10px;
    border-radius: 8px;
    background: var(--panel2);
  }
  li.failed {
    background: var(--failSoft);
  }
  .mark {
    color: var(--pass);
    font-weight: 600;
  }
  .failed .mark {
    color: var(--fail);
  }
  .what {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .value,
  .loc {
    margin-top: 3px;
    font-size: 12.5px;
    color: var(--ink2);
  }
  .value code {
    color: var(--ink);
  }
</style>
