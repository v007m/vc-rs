<script lang="ts">
  import type { Toasts } from './toasts.svelte.ts';

  /**
   * The toasts, newest at the bottom centre (D604): Strata capsules over the screen, read out
   * politely, an error assertively. Each has its optional action and leaves by itself.
   */
  let { toasts }: { toasts: Toasts } = $props();
</script>

<div class="stack" aria-live="polite">
  {#each toasts.list as toast (toast.id)}
    <div
      class="st-toast"
      class:is-error={toast.tone === 'error'}
      role={toast.tone === 'error' ? 'alert' : 'status'}
    >
      {#if toast.tone === 'error'}<span class="st-dot vc-fail" aria-hidden="true"></span>{/if}
      <span class="text">{toast.text}</span>
      {#if toast.action}
        {@const action = toast.action}
        <button
          type="button"
          class="st-btn st-btn--sm"
          onclick={() => {
            toasts.dismiss(toast.id);
            action.run();
          }}>{action.label}</button
        >
      {/if}
    </div>
  {/each}
</div>

<style>
  .stack {
    position: fixed;
    left: 50%;
    bottom: 24px;
    translate: -50% 0;
    display: grid;
    justify-items: center;
    gap: 8px;
    z-index: var(--st-z-toast);
    pointer-events: none;
  }
  /* stacked in the column, not each fixed at the same spot */
  .stack .st-toast {
    position: static;
    translate: none;
    pointer-events: auto;
    min-height: 40px;
  }
  .st-toast:not(:has(.st-btn)) {
    padding-right: 16px;
  }
  .text {
    font: var(--st-type-body);
  }
</style>
