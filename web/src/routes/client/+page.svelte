<script lang="ts">
  import { engine } from '#lib/api/transport.ts';
  import ClientScreen from '#lib/client/ClientScreen.svelte';
  import { enginePort } from '#lib/client/port.ts';
  import { ClientSession } from '#lib/client/session.svelte.ts';
  import { getRunSession } from '#lib/run/context.ts';
  import Screen from '#lib/Screen.svelte';

  const port = enginePort(engine);
  const client = new ClientSession(port);
  void client.load();
</script>

<Screen title="API client">
  <ClientScreen {client} {port} session={getRunSession()} />
</Screen>
