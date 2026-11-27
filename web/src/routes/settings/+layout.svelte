<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import UserScope from '#lib/settings/UserScope.svelte';
  import type { SettingsScope } from '#lib/settings/userScope.ts';
  import { getShellSlots } from '#lib/shell/slots.svelte.ts';
  import type { Snippet } from 'svelte';

  /** Every Settings page shares the top row's scope switch: Project or You only. */
  let { children }: { children: Snippet } = $props();

  const slots = getShellSlots();
  $effect(() => slots.fill({ top }));

  function choose(scope: SettingsScope): void {
    void goto(resolve(scope === 'user' ? '/settings/user' : '/settings'));
  }
</script>

{#snippet top()}
  <UserScope pathname={page.url.pathname} onchoose={choose} />
{/snippet}

{@render children()}
