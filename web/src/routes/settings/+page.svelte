<script lang="ts">
  import { engine } from '#lib/api/transport.ts';
  import Screen from '#lib/Screen.svelte';
  import { enginePort } from '#lib/settings/port.ts';
  import SettingsScreen from '#lib/settings/SettingsScreen.svelte';
  import { EffectiveSettings } from '#lib/settings/settings.svelte.ts';

  const settings = new EffectiveSettings(enginePort(engine));
  void settings.load();
</script>

<Screen title="Settings">
  <SettingsScreen {settings} />
</Screen>
