<script lang="ts">
  import type { PageProps } from './$types';
  import { getRunsSpace } from '#lib/history/space.svelte.ts';
  import LoadScreen from '#lib/load/LoadScreen.svelte';
  import Screen from '#lib/Screen.svelte';

  /** A stored load run, `/runs/load/12` (D639); the top row names it. */
  let { params }: PageProps = $props();
  const space = getRunsSpace();
  const load = space.ports.load;
  const id = $derived(params.id);

  $effect(() => {
    if (!load.running) void load.open(id);
    space.title = `Load run #${id}`;
    return () => (space.title = undefined);
  });
</script>

<Screen title="Load run #{id}">
  <LoadScreen session={load} />
</Screen>
