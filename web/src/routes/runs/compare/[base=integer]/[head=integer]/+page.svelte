<script lang="ts">
  import type { PageProps } from './$types';
  import { goto } from '$app/navigation';
  import CompareView from '#lib/history/CompareView.svelte';
  import { compareHref } from '#lib/history/links.ts';
  import { getRunsSpace } from '#lib/history/space.svelte.ts';
  import Screen from '#lib/Screen.svelte';

  /** Compare, `/runs/compare/11/12` (D637): the base, then the head; the top row names both. */
  let { params }: PageProps = $props();
  const space = getRunsSpace();
  const base = $derived(params.base);
  const head = $derived(params.head);
  const title = $derived(`Compare #${base} and #${head}`);

  $effect(() => {
    space.title = title;
    return () => (space.title = undefined);
  });
</script>

<Screen {title}>
  <CompareView
    {base}
    {head}
    runs={space.history.runs}
    port={space.ports.seams.compare}
    onpick={(b, h) => void goto(compareHref(b, h))}
  />
</Screen>
