<script lang="ts">
  import type { PageProps } from './$types';
  import { getRunsSpace } from '#lib/history/space.svelte.ts';
  import StoredRunView from '#lib/history/StoredRunView.svelte';
  import { getRunSession } from '#lib/run/context.ts';
  import Screen from '#lib/Screen.svelte';
  import EmptyState from '#lib/ui/EmptyState.svelte';

  /** One stored run, `/runs/12` (D644); the top row names it. */
  let { params }: PageProps = $props();
  const space = getRunsSpace();
  const session = getRunSession();
  const history = space.history;
  const id = $derived(params.id);

  $effect(() => {
    void history.show(id);
    space.title = `Run #${id}`;
    return () => (space.title = undefined);
  });
</script>

<Screen title="Run #{id}">
  {#if history.open && history.open.run.id === id}
    <StoredRunView stored={history.open} {history} {session} tests={space.ports.seams.tests} />
  {:else if history.error}
    <EmptyState icon="runs" title="This run could not be opened" caption={history.error} />
  {:else}
    <p class="st-secondary" role="status">Opening run #{id}…</p>
  {/if}
</Screen>
