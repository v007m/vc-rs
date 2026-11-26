<script lang="ts">
  import { page } from '$app/state';
  import { getShellSlots } from '#lib/shell/slots.svelte.ts';
  import { getVcsSession } from '#lib/vcs/context.ts';
  import TestsSidebar from '#lib/vcs/TestsSidebar.svelte';
  import type { Snippet } from 'svelte';

  /** Every Tests screen shares its sidebar: the views and the Changes pane (D516, D518). */
  let { children }: { children: Snippet } = $props();

  const vcs = getVcsSession();
  $effect(() => getShellSlots().fill({ sidebar }));
</script>

<svelte:window onfocus={() => void vcs.refresh()} />

{#snippet sidebar()}
  <TestsSidebar {vcs} pathname={page.url.pathname} />
{/snippet}

{@render children()}
