<script lang="ts">
  import { getRunsSpace } from '#lib/history/space.svelte.ts';
  import { getRunSession } from '#lib/run/context.ts';
  import LiveRun from '#lib/run/LiveRun.svelte';
  import Screen from '#lib/Screen.svelte';

  const space = getRunsSpace();
  const session = getRunSession();

  $effect(() => {
    space.title = 'Live run';
    return () => (space.title = undefined);
  });
</script>

<Screen title="Live run">
  <LiveRun {session} history={space.ports.seams.tests} onrun={() => space.run()} />
</Screen>
