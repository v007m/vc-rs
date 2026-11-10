<script lang="ts">
  import GroupTree from '#lib/tree/GroupTree.svelte';
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

  const groups = $derived(
    folders.map((f) => ({
      key: folderKey(f.dir),
      label: f.dir || 'Project root',
      leaves: f.files.map((path) => ({ key: path, label: baseName(path), title: path })),
    })),
  );
</script>

<GroupTree label="Files" {groups} {current} mono {onopen} />
