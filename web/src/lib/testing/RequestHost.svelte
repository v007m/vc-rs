<script lang="ts">
  import RequestBar from '#lib/client/RequestBar.svelte';
  import RequestPanel from '#lib/client/RequestPanel.svelte';
  import type { ClientSession } from '#lib/client/session.svelte.ts';

  /** The request bar over the request card, as the Client screen stacks them, for tests. */
  let { client, onsaved }: { client: ClientSession; onsaved: (what: string) => void } = $props();
</script>

<RequestBar {client} />
<RequestPanel {client} {onsaved} />
