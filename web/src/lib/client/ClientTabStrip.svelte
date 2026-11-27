<script lang="ts">
  import Icon from '#lib/shell/Icon.svelte';
  import type { ClientSession, ClientTab } from './session.svelte.ts';

  /** The Client's open tabs in the top row (D539): one per api or scratch request. */
  let { client, onpick }: { client: ClientSession; onpick?: () => void } = $props();

  function pick(id: string): void {
    client.activate(id);
    onpick?.();
  }

  function fresh(): void {
    client.scratch();
    onpick?.();
  }

  /** The pill a tab shows: its method, or SSE and WS for a stream. */
  function badge(tab: ClientTab): string {
    return tab.kind === 'api' ? tab.request.form.method : tab.badge;
  }
</script>

<nav class="tabstrip" aria-label="Open requests">
  <ul class="ts-list">
    {#each client.tabs as tab (tab.id)}
      <li class="ot">
        <button
          type="button"
          class="ot-open"
          aria-current={tab.id === client.activeId ? 'page' : undefined}
          title={tab.kind === 'api' ? tab.api.id : tab.title}
          onclick={() => pick(tab.id)}
        >
          <span class="method" data-m={badge(tab)}>{badge(tab)}</span>
          <span class="title">{tab.title}</span>
        </button>
        <button
          type="button"
          class="ot-close"
          aria-label="Close {tab.title}"
          onclick={() => client.close(tab.id)}><Icon name="x" small /></button
        >
      </li>
    {/each}
  </ul>
  <button
    type="button"
    class="st-icon-btn ts-new"
    aria-label="New request"
    title="New request"
    onclick={fresh}><Icon name="plus" /></button
  >
</nav>

<style>
  .tabstrip {
    display: flex;
    align-items: center;
    gap: var(--st-space-4);
    flex: 1 1 auto;
    min-width: 0;
  }
  .ts-list {
    display: flex;
    align-items: center;
    gap: 2px;
    min-width: 0;
    margin: 0;
    padding: 2px;
    list-style: none;
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
    scroll-padding-inline: 36px;
  }
  .ts-list::-webkit-scrollbar {
    display: none;
  }
  /* tabs are chrome on the canvas: no box until hovered; the open one is a raised plate */
  .ot {
    position: relative;
    display: flex;
    align-items: center;
    flex: none;
    max-width: 232px;
    height: var(--st-size-control-md);
    border-radius: var(--st-radius-md);
    color: var(--st-text-secondary);
    transition:
      background var(--st-duration-fast) var(--st-ease-standard),
      color var(--st-duration-fast) var(--st-ease-standard);
  }
  .ot:hover {
    background: var(--st-chrome-hover);
    color: var(--st-text-primary);
  }
  .ot:has([aria-current='page']) {
    color: var(--st-text-primary);
    background: var(--st-chrome-selected);
    box-shadow: var(--st-shadow-xs);
  }
  .ot-open {
    display: flex;
    align-items: center;
    gap: var(--st-space-8);
    flex: 1 1 auto;
    min-width: 0;
    height: 100%;
    padding: 0 var(--st-space-4) 0 var(--st-space-12);
    border: 0;
    border-radius: var(--st-radius-md);
    background: transparent;
    color: inherit;
    text-align: left;
    font: var(--st-type-body-strong);
  }
  .ot-open:focus-visible {
    outline-offset: -2px;
  }
  .ot-open .title {
    flex: 1 1 auto;
    min-width: 0;
    max-width: 22ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ot :global(.method) {
    min-width: 0;
    height: 16px;
    padding: 0 3px;
    background: transparent;
    font-size: 9.5px;
  }
  .ot-close {
    flex: none;
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    margin-right: 6px;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--st-text-secondary);
    opacity: 0;
  }
  .ot:hover .ot-close,
  .ot:focus-within .ot-close,
  .ot:has([aria-current='page']) .ot-close {
    opacity: 1;
  }
  .ot-close:hover {
    color: var(--st-text-primary);
  }
</style>
