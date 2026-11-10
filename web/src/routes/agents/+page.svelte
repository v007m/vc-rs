<script lang="ts">
  import { engine } from '#lib/api/transport.ts';
  import AgentsScreen from '#lib/agents/AgentsScreen.svelte';
  import { AgentDesk } from '#lib/agents/desk.svelte.ts';
  import Screen from '#lib/Screen.svelte';

  const desk = new AgentDesk(engine);
</script>

<Screen title="AI agents">
  <AgentsScreen {desk} />
</Screen>
