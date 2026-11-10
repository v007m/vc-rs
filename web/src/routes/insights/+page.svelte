<script lang="ts">
  import { engine } from '#lib/api/transport.ts';
  import { Insights } from '#lib/insights/insights.svelte.ts';
  import InsightsScreen from '#lib/insights/InsightsScreen.svelte';
  import { enginePort } from '#lib/insights/port.ts';
  import Screen from '#lib/Screen.svelte';

  const insights = new Insights(enginePort(engine));
  void insights.load();
</script>

<Screen title="Insights">
  <InsightsScreen {insights} />
</Screen>
