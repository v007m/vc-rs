<script lang="ts">
  import { engine } from '#lib/api/transport.ts';
  import Screen from '#lib/Screen.svelte';
  import { Alerts, alertsPort } from '#lib/settings/alerts.svelte.ts';
  import AlertsScreen from '#lib/settings/AlertsScreen.svelte';
  import { getProjectStatus } from '#lib/shell/project.svelte.ts';

  const project = getProjectStatus();
  const alerts = new Alerts(alertsPort(engine));
  void alerts.load();
</script>

<Screen title="Alerts">
  <AlertsScreen {alerts} profile={project.profile} />
</Screen>
