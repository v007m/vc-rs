<script lang="ts">
  import { getRunsSpace } from '#lib/history/space.svelte.ts';
  import LoadScreen from '#lib/load/LoadScreen.svelte';
  import Screen from '#lib/Screen.svelte';

  /** The load run on screen (D639): the one running, or the last one opened. */
  const space = getRunsSpace();
  const load = space.ports.load;
  if (!load.run) void load.listScenarios();

  $effect(() => {
    space.title = 'Load run';
    return () => (space.title = undefined);
  });
</script>

<Screen title="Load run">
  <LoadScreen session={load} />
</Screen>
