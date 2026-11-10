<script lang="ts">
  import { engine } from '#lib/api/transport.ts';
  import Screen from '#lib/Screen.svelte';
  import { enginePort } from '#lib/settings/port.ts';
  import { schemaPort, SecretSchema } from '#lib/settings/secrets.svelte.ts';
  import SettingsScreen from '#lib/settings/SettingsScreen.svelte';
  import { EffectiveSettings } from '#lib/settings/settings.svelte.ts';

  const settings = new EffectiveSettings(enginePort(engine));
  const schema = new SecretSchema(schemaPort(engine));
  void settings.load();
  void schema.load();
</script>

<Screen title="Settings">
  <SettingsScreen {settings} {schema} />
</Screen>
