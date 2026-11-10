<script lang="ts">
  import { TreeFocus } from '#lib/tree/focus.svelte.ts';
  import type { FileGroup } from './catalog.ts';

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

  let tree: HTMLUListElement | undefined = $state();
  const focus = new TreeFocus({
    branches: () =>
      groups.map((g) => ({ key: fileKey(g.fileId), leaves: g.tests.map((t) => t.id) })),
    root: () => tree,
    onleaf: (key, press) => (press === ' ' ? ontoggle(key) : onopen(key)),
  });

  /** The tree key of a file; test keys are full IDs, which always have a `#`. */
  function fileKey(fileId: string): string {
    return `file:${fileId}`;
  }
</script>

<ul class="tree" role="tree" aria-label="Tests" aria-multiselectable="true" bind:this={tree}>
  {#each groups as group (group.fileId)}
    {@const key = fileKey(group.fileId)}
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
              tabindex={focus.tabKey === test.id ? 0 : -1}
              data-key={test.id}
              onclick={(event) => {
                event.stopPropagation();
                focus.focusKey = test.id;
                onopen(test.id);
              }}
              onfocus={() => (focus.focusKey = test.id)}
              onkeydown={focus.onkeydown}
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
