<script lang="ts">
  import { engine } from '#lib/api/transport.ts';
  import ClientScreen from '#lib/client/ClientScreen.svelte';
  import { ClientLayout } from '#lib/client/layout.svelte.ts';
  import { enginePort } from '#lib/client/port.ts';
  import { engineSeams } from '#lib/client/seams.ts';
  import { ClientSession } from '#lib/client/session.svelte.ts';
  import { getRunSession } from '#lib/run/context.ts';

  const port = enginePort(engine);
  const seams = engineSeams(engine);
  const client = new ClientSession(port, seams);
  const layout = new ClientLayout();
  void client.load();
</script>

<svelte:head><title>API client · Vibranium</title></svelte:head>

<ClientScreen {client} {port} session={getRunSession()} {layout} history={seams} />
