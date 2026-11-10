<script lang="ts">
  import { TreeFocus } from '#lib/tree/focus.svelte.ts';
  import { baseName, folderKey, type Folder } from './files.ts';

  let {
    folders,
    current,
    onopen,
  }: {
    folders: readonly Folder[];
    current: string | undefined;
    onopen: (path: string) => void;
  } = $props();

  let tree: HTMLUListElement | undefined = $state();
  const focus = new TreeFocus({
    branches: () => folders.map((f) => ({ key: folderKey(f.dir), leaves: f.files })),
    root: () => tree,
    onleaf: (path) => onopen(path),
  });
</script>

<ul class="tree" role="tree" aria-label="Files" bind:this={tree}>
  {#each folders as folder (folder.dir)}
    {@const key = folderKey(folder.dir)}
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
        <span class="mono name">{folder.dir || 'Project root'}</span>
        <span class="count">{folder.files.length}</span>
      </span>
      {#if open}
        <ul role="group">
          {#each folder.files as path (path)}
            <li
              role="treeitem"
              aria-level={2}
              aria-selected={current === path}
              tabindex={focus.tabKey === path ? 0 : -1}
              data-key={path}
              title={path}
              onclick={(event) => {
                event.stopPropagation();
                focus.focusKey = path;
                onopen(path);
              }}
              onfocus={() => (focus.focusKey = path)}
              onkeydown={focus.onkeydown}
            >
              <span class="row file" class:current={current === path}>
                <span class="mono name">{baseName(path)}</span>
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
