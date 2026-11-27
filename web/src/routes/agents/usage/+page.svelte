<script lang="ts">
  import { engine } from '#lib/api/transport.ts';
  import { usagePort } from '#lib/agents/usage.ts';
  import { UsageView } from '#lib/agents/usage.svelte.ts';
  import UsageScreen from '#lib/agents/UsageScreen.svelte';
  import Screen from '#lib/Screen.svelte';

  const view = new UsageView(usagePort(engine));
</script>

<Screen title="Agent usage">
  <UsageScreen {view} />
</Screen>
