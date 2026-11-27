<script lang="ts">
  import Screen from '#lib/Screen.svelte';
  import Appearance from '#lib/settings/Appearance.svelte';
  import { getShellState } from '#lib/shell/state.svelte.ts';

  const shell = getShellState();
</script>

<Screen title="Appearance">
  <Appearance {shell} />
</Screen>
