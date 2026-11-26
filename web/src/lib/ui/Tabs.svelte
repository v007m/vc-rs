<script lang="ts">
  import { tick, type Snippet } from 'svelte';
  import { tabMove, type Tab } from './tabs.ts';

  let {
    label,
    tabs,
    selected = $bindable(),
    panel,
  }: {
    label: string;
    tabs: readonly Tab[];
    selected: string;
    panel: Snippet<[string]>;
  } = $props();

  const uid = $props.id();
  let list: HTMLDivElement | undefined = $state();

  async function onkeydown(event: KeyboardEvent): Promise<void> {
    const next = tabMove(tabs, selected, event.key);
    if (!next) return;
    event.preventDefault();
    selected = next;
    await tick();
    list?.querySelector<HTMLElement>(`[data-tab="${next}"]`)?.focus();
  }
</script>

<div class="tabs" role="tablist" aria-label={label} bind:this={list}>
  {#each tabs as tab (tab.id)}
    <button
      type="button"
      role="tab"
      class="tab"
      id="{uid}-{tab.id}"
      data-tab={tab.id}
      aria-selected={selected === tab.id}
      aria-controls="{uid}-panel"
      tabindex={selected === tab.id ? 0 : -1}
      onclick={() => (selected = tab.id)}
      {onkeydown}
      >{tab.count ? `${tab.label} ` : tab.label}{#if tab.count}<span
          class="count"
          class:bad={tab.bad}>{tab.count}</span
        >{/if}</button
    >
  {/each}
</div>
<div class="panel" role="tabpanel" id="{uid}-panel" aria-labelledby="{uid}-{selected}" tabindex="0">
  {@render panel(selected)}
</div>

<style>
  /* The look is the shared .tabs/.tab of controls.css: the selected tab is a raised L2 pill. */
  .count {
    font-size: var(--fs-0);
    font-variant-numeric: tabular-nums;
    color: var(--ink-3);
  }
  .count.bad {
    color: var(--st-fail-ink);
  }
  .panel {
    padding: var(--sp-3) 0;
  }
</style>
