<script lang="ts">
  import { untrack } from 'svelte';
  import { engine } from '#lib/api/transport.ts';
  import Screen from '#lib/Screen.svelte';
  import { getRunSession } from '#lib/run/context.ts';
  import { enginePort as skipPort } from '#lib/skip/port.ts';
  import { Catalog } from '#lib/tests/catalog.svelte.ts';
  import Explorer from '#lib/tests/Explorer.svelte';
  import { getVcsSession } from '#lib/vcs/context.ts';

  const session = getRunSession();
  const catalog = new Catalog(engine);
  void catalog.load();
  const vcs = getVcsSession();
  // A pull changed the files on disk.
  $effect(() => {
    if (vcs.revision > 0) untrack(() => void catalog.load());
  });
</script>

<Screen title="Tests">
  <Explorer {catalog} {session} skips={skipPort(engine)} />
</Screen>
