<script lang="ts">
  import { untrack } from 'svelte';
  import { page } from '$app/state';
  import AllRuns from '#lib/history/AllRuns.svelte';
  import { getRunsSpace } from '#lib/history/space.svelte.ts';
  import Screen from '#lib/Screen.svelte';

  const space = getRunsSpace();

  $effect(() => {
    const schedule = page.url.searchParams.get('schedule') ?? undefined;
    untrack(() => {
      const { history } = space;
      if (history.scope.schedule !== schedule) void history.rescope({ ...history.scope, schedule });
    });
  });
</script>

<Screen title="All runs">
  <AllRuns history={space.history} onrun={(selector) => space.run(selector)} />
</Screen>
