<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from '#lib/shell/Icon.svelte';
  import type { IconName } from '#lib/shell/spaces.ts';

  /**
   * Strata's empty state (D609): a 28px icon, a Title3 title, a caption and at most one
   * button, floating centred on the card itself, with no box of its own (D658).
   */
  let {
    icon,
    title,
    caption,
    action,
  }: { icon: IconName; title: string; caption?: string; action?: Snippet } = $props();
</script>

<div class="empty vc-empty">
  <Icon name={icon} />
  <p class="title">{title}</p>
  {#if caption}<p class="caption">{caption}</p>{/if}
  {#if action}{@render action()}{/if}
</div>

<style>
  .empty {
    align-self: center;
    margin: auto;
    width: min(100%, 400px);
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--st-space-6);
    min-height: 160px;
    padding: var(--st-space-24);
    text-align: center;
    color: var(--st-text-tertiary);
    background: none;
    border: 0;
  }
  .empty :global(svg) {
    width: 28px;
    height: 28px;
    margin-bottom: var(--st-space-4);
  }
  .title {
    margin: 0;
    font: var(--st-type-title3);
    color: var(--st-text-primary);
  }
  .caption {
    margin: 0 0 var(--st-space-6);
    max-width: 44ch;
    font: var(--st-type-caption);
    color: var(--st-text-secondary);
  }
</style>
