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
  .tabs {
    display: flex;
    gap: 2px;
    border-bottom: 1px solid var(--line);
  }
  .tab {
    height: 32px;
    padding: 0 12px;
    border: 0;
    border-bottom: 2px solid transparent;
    background: none;
    color: var(--ink2);
    font: 500 13px var(--font);
    cursor: pointer;
  }
  .tab[aria-selected='true'] {
    border-bottom-color: var(--accent);
    color: var(--ink);
  }
  .count {
    color: var(--ink2);
    font-weight: 400;
  }
  .count.bad {
    color: var(--fail);
  }
  .panel {
    padding: 12px 0;
  }
</style>
