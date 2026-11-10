<script lang="ts">
  import Screen from '#lib/Screen.svelte';
  import RunPanel from '#lib/run/RunPanel.svelte';
  import { getRunSession } from '#lib/run/context.ts';

  const session = getRunSession();
</script>

<Screen title="Runs">
  <RunPanel {session} />
</Screen>
