<script lang="ts">
  import type { CheckLine } from './types.ts';

  let { checks }: { checks: readonly CheckLine[] } = $props();
</script>

<ul class="checks" aria-label="Checks">
  {#each checks as check, i (i)}
    <li class:failed={!check.passed}>
      <span class="mark" aria-hidden="true">{check.passed ? '✓' : '✗'}</span>
      <div class="what">
        <code class="vc-mono">{check.label}</code>
        <span class="vc-sr">{check.passed ? ' held' : ' failed'}</span>
        {#if !check.passed}
          <div class="value">expected <code class="vc-mono">{check.expected}</code></div>
          {#if check.actual !== undefined}
            <div class="value">actual <code class="vc-mono">{check.actual}</code></div>
          {/if}
        {/if}
        {#if check.location}<div class="loc vc-mono">{check.location}</div>{/if}
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
    background: var(--st-surface-wash);
  }
  li.failed {
    background: var(--vc-fail-soft);
  }
  .mark {
    color: var(--vc-pass);
    font-weight: 600;
  }
  .failed .mark {
    color: var(--vc-fail);
  }
  .what {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .value,
  .loc {
    margin-top: 3px;
    font-size: 12.5px;
    color: var(--st-text-secondary);
  }
  .value code {
    color: var(--st-text-primary);
  }
</style>
