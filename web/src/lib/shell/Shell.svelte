<script lang="ts">
  import { resolve } from '$app/paths';
  import type { RunSession } from '#lib/run/session.svelte.ts';
  import { untrack, type Snippet } from 'svelte';
  import { cycleRegion, isPaletteKey, watchModifier } from './keys.ts';
  import Palette from './Palette.svelte';
  import { paletteItems } from './palette.ts';
  import type { ProjectStatus } from './project.svelte.ts';
  import Rail from './Rail.svelte';
  import { drawerWidth, shortcutSpace, type ShellState } from './state.svelte.ts';
  import Sidebar from './Sidebar.svelte';
  import { ShellSlots } from './slots.svelte.ts';
  import { spaceOf, spaces, viewOf } from './spaces.ts';
  import StatusLine from './StatusLine.svelte';
  import TopBar from './TopBar.svelte';
  import { applyAppearance, presetFor } from './appearance.ts';
  import { commandItems } from './commands.ts';
  import { runEnded } from './runToast.ts';
  import ToastStack from '#lib/ui/ToastStack.svelte';
  import { getToasts } from '#lib/ui/toasts.svelte.ts';

  /**
   * The five-space shell (D512-D514) on Strata (D590): the space's canvas, with the space
   * switcher, the sidebar, the one top row and the status line on it, and the screen on a card.
   * `navigate` is SvelteKit's goto in the app and a spy in tests.
   */
  let {
    shell,
    session,
    project,
    pathname,
    search = '',
    navigate,
    slots = new ShellSlots(),
    branch,
    children,
  }: {
    shell: ShellState;
    session: RunSession;
    project: ProjectStatus;
    pathname: string;
    search?: string;
    navigate: (href: string) => void;
    slots?: ShellSlots;
    /** The git branch for the status line, when the engine gives it. */
    branch?: string | undefined;
    children: Snippet;
  } = $props();

  let width = $state(0);
  // Before the first measure (and in jsdom, which measures nothing) the sidebar counts as in the layout.
  let inLayout = $derived(width === 0 || width > drawerWidth);
  let space = $derived(spaceOf(pathname));
  let title = $derived(slots.title ?? viewOf(pathname)?.title ?? space.label);
  let sidebarShown = $derived(inLayout ? !shell.prefs.sbCollapsed : shell.drawer);
  let hints = $state(false);
  let app: HTMLElement | undefined = $state();
  /** The command palette, open with what it grows from (D620). */
  let palette: { anchor: Element | null } | undefined = $state();
  const screens = paletteItems(spaces, (route) => resolve(route));
  const toasts = getToasts();
  let items = $derived([
    ...screens,
    ...commandItems({
      runHref: `${resolve('/runs')}?run=ask`,
      fileHref: (path) => `${resolve('/tests/editor')}?file=${encodeURIComponent(path)}`,
      files: project.files,
      profiles: project.profiles,
      profile: project.profile,
      appearance: shell.prefs.appearance,
      useProfile: (profile) => project.useProfile(profile),
      setAppearance: (change) => shell.setAppearance(change),
      say: (text) => toasts.show(text),
    }),
  ]);

  function openPalette(anchor: Element | null): void {
    if (project.files.length === 0) void project.listFiles();
    palette ??= { anchor };
  }

  $effect(() => applyAppearance(document.documentElement, shell.prefs.appearance));
  // A run that ends, on any screen, says how it went, with a way to its report.
  let wasRunning = false;
  $effect(() => {
    const running = session.running;
    if (wasRunning && !running) {
      const ended = runEnded(session.view);
      untrack(() =>
        toasts.show(ended.text, ended.tone, {
          label: 'Open',
          run: () =>
            navigate(
              ended.id === undefined
                ? resolve('/runs/live')
                : resolve('/runs/[id=integer]', { id: ended.id }),
            ),
        }),
      );
    }
    wasRunning = running;
  });

  $effect(() => shell.remember(pathname, search));
  $effect(() => watchModifier(window, (held) => (hints = held)));
  $effect(() => {
    void pathname;
    shell.drawer = false;
  });

  function toggle(): void {
    shell.toggleSidebar(inLayout);
  }

  function onkeydown(event: KeyboardEvent): void {
    if (isPaletteKey(event)) {
      event.preventDefault();
      openPalette(app?.querySelector('.search-btn') ?? null);
      return;
    }
    if (event.key === 'Escape' && shell.drawer) {
      shell.drawer = false;
      return;
    }
    if (event.key === 'F6' && app && cycleRegion(app, document.activeElement, event.shiftKey)) {
      event.preventDefault();
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
  class="app st-root st-canvas"
  data-space={presetFor(shell.prefs.appearance, space.preset)}
  data-switcher={shell.prefs.switcher}
  class:is-sb-collapsed={shell.prefs.sbCollapsed}
  class:is-drawer-open={shell.drawer}
  class:is-hints={hints}
  bind:clientWidth={width}
  bind:this={app}
>
  <div class="shell">
    <Rail {shell} {pathname} />
    <Sidebar {shell} {space} {pathname} body={slots.sidebar} ontoggle={toggle} />
    <div class="main">
      <TopBar
        {title}
        {session}
        {project}
        {sidebarShown}
        left={slots.top}
        primary={slots.primary}
        ontoggle={toggle}
        onsearch={openPalette}
      />
      <main class="stage" id="view" tabindex="-1" data-region>
        <!-- One wrapper either way: switching to a bare stage must not remount the screen. -->
        <div class={slots.bare ? 'bare' : 'st-card st-card--glass card'}>
          {@render children()}
        </div>
      </main>
      <StatusLine {project} {branch} />
    </div>
    <!-- Inside .shell, its stacking context, so the drawer and rail stack above the scrim. -->
    {#if palette}
      <Palette
        {items}
        anchor={palette.anchor}
        onpick={navigate}
        onclose={() => (palette = undefined)}
      />
    {/if}
    <ToastStack {toasts} />
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
</div>
