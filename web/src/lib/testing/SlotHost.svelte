<script lang="ts" generics="Props extends Record<string, unknown>">
  import type { Component } from 'svelte';
  import { setShellSlots, ShellSlots } from '#lib/shell/slots.svelte.ts';

  /**
   * A stand-in shell for tests: it renders `screen` with `args`, and whatever the screen puts
   * in the sidebar and top row slots (D537) in their own labelled regions.
   */
  let { screen: Screen, args }: { screen: Component<Props>; args: Props } = $props();

  const slots = setShellSlots(new ShellSlots());
</script>

<aside aria-label="Sidebar">{@render slots.sidebar?.()}</aside>
<header aria-label="Top row">{@render slots.top?.()}</header>
<main><Screen {...args} /></main>
