<script lang="ts">
  import { engine } from '#lib/api/transport.ts';
  import Screen from '#lib/Screen.svelte';
  import { configPort, ConfigForm } from '#lib/settings/configForm.svelte.ts';
  import { enginePort } from '#lib/settings/port.ts';
  import { SecretCheck, secretCheckPort } from '#lib/settings/secretCheck.svelte.ts';
  import { schemaPort, SecretSchema } from '#lib/settings/secrets.svelte.ts';
  import SettingsScreen from '#lib/settings/SettingsScreen.svelte';
  import { EffectiveSettings } from '#lib/settings/settings.svelte.ts';

  const settings = new EffectiveSettings(enginePort(engine));
  const form = new ConfigForm(configPort(engine));
  const schema = new SecretSchema(schemaPort(engine));
  const check = new SecretCheck(secretCheckPort(engine));
  void settings.load().then(() => check.check(settings.profiles));
  void form.load();
  void schema.load();
</script>

<Screen title="Settings">
  <SettingsScreen {settings} {form} {schema} {check} />
</Screen>
