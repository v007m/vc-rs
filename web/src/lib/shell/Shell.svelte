<script lang="ts">
  import type { RunSession } from '#lib/run/session.svelte.ts';
  import type { Snippet } from 'svelte';
  import Background from './Background.svelte';
  import type { ProjectStatus } from './project.svelte.ts';
  import Rail from './Rail.svelte';
  import { drawerWidth, shortcutSpace, type ShellState } from './state.svelte.ts';
  import Sidebar from './Sidebar.svelte';
  import { ShellSlots } from './slots.svelte.ts';
  import { spaceOf, viewOf } from './spaces.ts';
  import StatusLine from './StatusLine.svelte';
  import TopBar from './TopBar.svelte';

  /**
   * The five-space shell (D512-D514): L0 background, the space switcher, the space's sidebar,
   * the one top row, the screen on an L1 card, and the status line. `navigate` is SvelteKit's
   * goto in the app and a spy in tests.
   */
  let {
    shell,
    session,
    project,
    pathname,
    search = '',
    navigate,
    slots = new ShellSlots(),
    children,
  }: {
    shell: ShellState;
    session: RunSession;
    project: ProjectStatus;
    pathname: string;
    search?: string;
    navigate: (href: string) => void;
    slots?: ShellSlots;
    children: Snippet;
  } = $props();

  let width = $state(0);
  // Before the first measure (and in jsdom, which measures nothing) the sidebar counts as in the layout.
  let inLayout = $derived(width === 0 || width > drawerWidth);
  let space = $derived(spaceOf(pathname));
  let title = $derived(viewOf(pathname)?.title ?? space.label);
  let sidebarShown = $derived(inLayout ? !shell.prefs.sbCollapsed : shell.drawer);

  $effect(() => shell.remember(pathname, search));
  $effect(() => {
    void pathname;
    shell.drawer = false;
  });

  function toggle(): void {
    shell.toggleSidebar(inLayout);
  }

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === 'Escape' && shell.drawer) {
      shell.drawer = false;
      return;
    }
    const key = shortcutSpace(event);
    if (!key) return;
    event.preventDefault();
    navigate(shell.routeFor(key, pathname));
  }
</script>

<svelte:window {onkeydown} />

<div
  class="app"
  data-switcher={shell.prefs.switcher}
  class:is-sb-collapsed={shell.prefs.sbCollapsed}
  class:is-drawer-open={shell.drawer}
  bind:clientWidth={width}
>
  <Background />
  <div class="shell">
    <Rail {shell} {pathname} />
    <Sidebar {shell} {space} {pathname} body={slots.sidebar} ontoggle={toggle} />
    <div class="main">
      <TopBar {title} {session} {project} {sidebarShown} left={slots.top} ontoggle={toggle} />
      <main class="stage" id="view" tabindex="-1">
        {#if slots.bare}
          {@render children()}
        {:else}
          <div class="glass card">
            {@render children()}
          </div>
        {/if}
      </main>
      <StatusLine {project} />
    </div>
  </div>
  {#if shell.drawer && !inLayout}
    <button
      type="button"
      class="scrim-drawer"
      aria-label="Close sidebar"
      tabindex="-1"
      onclick={() => (shell.drawer = false)}
    ></button>
  {/if}
</div>
