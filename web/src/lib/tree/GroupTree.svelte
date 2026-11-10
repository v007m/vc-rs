<script lang="ts">
  import { TreeFocus } from './focus.svelte.ts';
  import type { Group } from './tree.ts';

  let {
    label,
    groups,
    current,
    mono = false,
    onopen,
  }: {
    label: string;
    groups: readonly Group[];
    current: string | undefined;
    /** Whether leaf labels are code (paths, IDs) and set in the mono face. */
    mono?: boolean;
    onopen: (key: string) => void;
  } = $props();

  let tree: HTMLUListElement | undefined = $state();
  const focus = new TreeFocus({
    branches: () => groups.map((g) => ({ key: g.key, leaves: g.leaves.map((l) => l.key) })),
    root: () => tree,
    onleaf: (key) => onopen(key),
  });
</script>

<ul class="tree" role="tree" aria-label={label} bind:this={tree}>
  {#each groups as group (group.key)}
    {@const key = group.key}
    {@const open = focus.isOpen(key)}
    <li
      role="treeitem"
      aria-expanded={open}
      aria-level={1}
      aria-selected={false}
      tabindex={focus.tabKey === key ? 0 : -1}
      data-key={key}
      onclick={() => focus.setOpen(key, !open)}
      onfocus={() => (focus.focusKey = key)}
      onkeydown={focus.onkeydown}
    >
      <span class="row folder">
        <span class="chev" aria-hidden="true">{open ? '▾' : '▸'}</span>
        <span class="mono name">{group.label}</span>
        <span class="count">{group.leaves.length}</span>
      </span>
      {#if open}
        <ul role="group">
          {#each group.leaves as leaf (leaf.key)}
            <li
              role="treeitem"
              aria-level={2}
              aria-selected={current === leaf.key}
              tabindex={focus.tabKey === leaf.key ? 0 : -1}
              data-key={leaf.key}
              title={leaf.title}
              onclick={(event) => {
                event.stopPropagation();
                focus.focusKey = leaf.key;
                onopen(leaf.key);
              }}
              onfocus={() => (focus.focusKey = leaf.key)}
              onkeydown={focus.onkeydown}
            >
              <span class="row file" class:current={current === leaf.key}>
                <span class="name" class:mono>{leaf.label}</span>
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
    min-height: 30px;
    padding: 0 10px;
    border-radius: 8px;
  }
  .row:hover {
    background: var(--panel2);
  }
  .folder {
    font-weight: 600;
  }
  .file {
    padding-left: 28px;
  }
  .file.current {
    background: var(--accentSoft);
    color: var(--accentInk);
  }
  .chev {
    width: 12px;
    color: var(--ink3);
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    margin-left: auto;
    color: var(--ink3);
    font-weight: 400;
  }
</style>
