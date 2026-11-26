<script lang="ts">
  import Icon from '#lib/shell/Icon.svelte';
  import SidebarSection from '#lib/shell/SidebarSection.svelte';
  import { getShellState } from '#lib/shell/state.svelte.ts';
  import GroupTree from '#lib/tree/GroupTree.svelte';
  import { apiGroups } from './apis.ts';
  import HistoryList from './HistoryList.svelte';
  import type { HistorySource } from './seams.ts';
  import type { ClientSession } from './session.svelte.ts';

  /**
   * The Client's own sidebar (ui.md §4, D537): the project's apis as a tree, its flow tests and
   * the recent calls. Each section folds, and the shell remembers which are shut.
   */
  let {
    client,
    history,
    flow,
    onhar,
    onapi,
    onflow,
  }: {
    client: ClientSession;
    history: HistorySource;
    /** The flow test open in the editor, if any. */
    flow?: string | undefined;
    onhar: () => void;
    onapi: () => void;
    onflow: (id: string) => void;
  } = $props();

  const shell = getShellState();
  let filter = $state('');
  const groups = $derived(apiGroups(client.apis, filter));

  function open(id: string): void {
    onapi();
    void client.open(id);
  }
</script>

<SidebarSection
  {shell}
  space="client"
  section={{ id: 'requests', label: 'Requests' }}
  meta={String(client.apis.length)}
>
  {#snippet actions()}
    <button
      type="button"
      class="st-icon-btn st-icon-btn--sm"
      aria-label="New request"
      title="New request"
      onclick={() => {
        onapi();
        client.scratch();
      }}><Icon name="plus" small /></button
    >
    <button
      type="button"
      class="st-icon-btn st-icon-btn--sm"
      aria-label="Import HAR"
      title="Import HAR"
      onclick={onhar}><Icon name="import" small /></button
    >
  {/snippet}
  <input
    class="st-input sb-filter"
    type="search"
    autocomplete="off"
    aria-label="Filter apis"
    placeholder="Filter apis"
    bind:value={filter}
  />
  <GroupTree label="Apis" {groups} current={client.current?.id} onopen={open} />
  {#if groups.length === 0}
    <p class="sb-quiet">{client.apis.length ? `No api matches “${filter}”.` : 'No apis yet.'}</p>
  {/if}
</SidebarSection>

<SidebarSection
  {shell}
  space="client"
  section={{ id: 'flows', label: 'Flows' }}
  meta={String(client.flows.length)}
>
  <ul class="sb-nav" aria-label="Flows">
    {#each client.flows as listed (listed.id)}
      <li>
        <button
          type="button"
          class="sb-row"
          aria-current={flow === listed.id ? 'true' : undefined}
          onclick={() => onflow(listed.id)}
        >
          <span class="title">{listed.name}</span>
          <span class="meta">{listed.steps} steps</span>
        </button>
      </li>
    {:else}
      <li class="sb-quiet">No flow tests yet. Import a HAR to record one.</li>
    {/each}
  </ul>
</SidebarSection>

<SidebarSection {shell} space="client" section={{ id: 'history', label: 'History' }}>
  <HistoryList
    source={history}
    stamp={client.sent}
    onreopen={(entry) => {
      onapi();
      client.scratch(entry);
    }}
  />
</SidebarSection>

<style>
  .sb-filter {
    width: calc(100% - 2 * var(--st-space-8));
    margin: 0 var(--st-space-8) var(--st-space-8);
  }
  .sb-quiet {
    margin: 0;
    padding: var(--st-space-4) var(--st-space-12);
    color: var(--st-text-secondary);
    font-size: var(--vc-size-caption);
  }
</style>
