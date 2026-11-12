<script lang="ts">
  import { engine } from '#lib/api/transport.ts';
  import { RunHistory } from '#lib/history/history.svelte.ts';
  import { enginePort } from '#lib/history/port.ts';
  import RunsScreen from '#lib/history/RunsScreen.svelte';
  import Screen from '#lib/Screen.svelte';
  import { getRunSession } from '#lib/run/context.ts';

  const session = getRunSession();
  const history = new RunHistory(enginePort(engine));
  void history.load();
</script>

<Screen title="Runs">
  <RunsScreen {history} {session} />
</Screen>
