<script lang="ts">
  import AllRuns from '#lib/history/AllRuns.svelte';
  import { getRunsSpace } from '#lib/history/space.svelte.ts';
  import Screen from '#lib/Screen.svelte';

  const space = getRunsSpace();
</script>

<Screen title="All runs">
  <AllRuns history={space.history} onrun={(selector) => space.run(selector)} />
</Screen>
