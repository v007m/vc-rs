<script lang="ts">
  import { engine } from '#lib/api/transport.ts';
  import Screen from '#lib/Screen.svelte';
  import { Plugins, pluginsPort } from '#lib/settings/plugins.svelte.ts';
  import PluginsScreen from '#lib/settings/PluginsScreen.svelte';

  const plugins = new Plugins(pluginsPort(engine));
  void plugins.load();
</script>

<Screen title="Plugins">
  <PluginsScreen {plugins} />
</Screen>
