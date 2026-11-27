<script lang="ts">
  import { engine } from '#lib/api/transport.ts';
  import Screen from '#lib/Screen.svelte';
  import { UserSettings, userSettingsPort } from '#lib/settings/userSettings.svelte.ts';
  import UserSettingsScreen from '#lib/settings/UserSettingsScreen.svelte';

  const settings = new UserSettings(userSettingsPort(engine));
  void settings.load();
</script>

<Screen title="Your settings">
  <UserSettingsScreen {settings} />
</Screen>
