<script lang="ts">
  import { tick } from 'svelte';
  import { SvelteSet } from 'svelte/reactivity';
  import type { FileGroup } from './catalog.ts';
  import { fileKey, treeMove, visibleNodes } from './tree.ts';

  let {
    groups,
    selected,
    current,
    onopen,
    ontoggle,
  }: {
    groups: readonly FileGroup[];
    selected: ReadonlySet<string>;
    current: string | undefined;
    onopen: (id: string) => void;
    ontoggle: (id: string) => void;
  } = $props();

  const closed = new SvelteSet<string>();
  let focusKey: string | undefined = $state();
  let tree: HTMLUListElement | undefined = $state();
  let nodes = $derived(visibleNodes(groups, closed));
  let tabKey = $derived(nodes.some((n) => n.key === focusKey) ? focusKey : nodes[0]?.key);

  async function focus(key: string): Promise<void> {
    focusKey = key;
    await tick();
    const items = tree?.querySelectorAll<HTMLElement>('[data-key]') ?? [];
    [...items].find((item) => item.dataset.key === key)?.focus();
  }

  function setOpen(key: string, open: boolean): void {
    if (open) closed.delete(key);
    else closed.add(key);
  }

  function onkeydown(event: KeyboardEvent): void {
    if (event.target !== event.currentTarget) return;
    const key = (event.currentTarget as HTMLElement).dataset.key;
    if (key?.includes('#') && (event.key === ' ' || event.key === 'Enter')) {
      event.preventDefault();
      if (event.key === ' ') ontoggle(key);
      else onopen(key);
      return;
    }
    const move = treeMove(nodes, key, event.key, closed);
    if (!move) return;
    event.preventDefault();
    if (move.kind === 'focus') void focus(move.key);
    else setOpen(move.key, move.kind === 'open');
  }
</script>

<ul class="tree" role="tree" aria-label="Tests" aria-multiselectable="true" bind:this={tree}>
  {#each groups as group (group.fileId)}
    {@const key = fileKey(group.fileId)}
    {@const open = !closed.has(key)}
    <li
      role="treeitem"
      aria-expanded={open}
      aria-level={1}
      aria-selected={false}
      tabindex={tabKey === key ? 0 : -1}
      data-key={key}
      onclick={() => setOpen(key, !open)}
      onfocus={() => (focusKey = key)}
      {onkeydown}
    >
      <span class="row file">
        <span class="chev" aria-hidden="true">{open ? '▾' : '▸'}</span>
        <span class="mono">{group.fileId}</span>
        <span class="count">{group.tests.length}</span>
      </span>
      {#if open}
        <ul role="group">
          {#each group.tests as test (test.id)}
            {@const on = selected.has(test.id)}
            <li
              role="treeitem"
              aria-level={2}
              aria-selected={on}
              aria-current={current === test.id ? 'true' : undefined}
              tabindex={tabKey === test.id ? 0 : -1}
              data-key={test.id}
              onclick={(event) => {
                event.stopPropagation();
                focusKey = test.id;
                onopen(test.id);
              }}
              onfocus={() => (focusKey = test.id)}
              {onkeydown}
            >
              <span class="row test" class:current={current === test.id}>
                <span
                  class="check"
                  class:on
                  aria-hidden="true"
                  onclick={(event) => {
                    event.stopPropagation();
                    ontoggle(test.id);
                  }}
                ></span>
                <span class="mono">{test.local}</span>
                <span class="name">{test.name}</span>
                {#if test.skipped}<span class="pill block">skipped</span>{/if}
                {#each test.tags as tag (tag)}<span class="pill">{tag}</span>{/each}
              </span>
            </li>
          {/each}
        </ul>
      {/if}
    </li>
  {/each}
</ul>

<style>
  .tree,
  .tree ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  [role='treeitem'] {
    border-radius: 8px;
    cursor: pointer;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 32px;
    padding: 0 10px;
    border-radius: 8px;
  }
  .row:hover {
    background: var(--panel2);
  }
  .file {
    font-weight: 600;
  }
  .test {
    padding-left: 28px;
  }
  .test.current {
    background: var(--accentSoft);
  }
  .chev {
    width: 12px;
    color: var(--ink3);
  }
  .count {
    margin-left: auto;
    color: var(--ink3);
    font-weight: 400;
  }
  .name {
    color: var(--ink2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .check {
    width: 14px;
    height: 14px;
    flex: none;
    border: 1.5px solid var(--ink3);
    border-radius: 4px;
  }
  .check.on {
    background: var(--accent);
    border-color: var(--accent);
    box-shadow: inset 0 0 0 2px var(--panel);
  }
</style>
