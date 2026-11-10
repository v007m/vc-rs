<script lang="ts">
  import { engine } from '#lib/api/transport.ts';
  import Screen from '#lib/Screen.svelte';
  import { getRunSession } from '#lib/run/context.ts';
  import { Catalog } from '#lib/tests/catalog.svelte.ts';
  import Explorer from '#lib/tests/Explorer.svelte';

  const session = getRunSession();
  const catalog = new Catalog(engine);
  void catalog.load();
</script>

<Screen title="Tests">
  <Explorer {catalog} {session} />
</Screen>
