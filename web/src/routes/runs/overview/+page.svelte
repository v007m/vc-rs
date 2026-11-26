<script lang="ts">
  import { engine } from '#lib/api/transport.ts';
  import { distinct } from '#lib/history/history.svelte.ts';
  import { getRunsSpace } from '#lib/history/space.svelte.ts';
  import { Insights } from '#lib/insights/insights.svelte.ts';
  import InsightsScreen from '#lib/insights/InsightsScreen.svelte';
  import { enginePort } from '#lib/insights/port.ts';
  import { getProjectStatus } from '#lib/shell/project.svelte.ts';
  import { enginePort as skipPort } from '#lib/skip/port.ts';
  import Screen from '#lib/Screen.svelte';

  /** The Runs overview (D643, the former Insights), scoped by profile and branch. */
  const project = getProjectStatus();
  const space = getRunsSpace();
  const insights = new Insights(enginePort(engine));
  const branches = $derived(distinct(space.history.runs, (r) => (r.branch ? [r.branch] : [])));
  void insights.open();
</script>

<Screen title="Runs overview">
  <InsightsScreen {insights} skips={skipPort(engine)} profiles={project.profiles} {branches} />
</Screen>
