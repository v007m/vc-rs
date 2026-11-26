<script lang="ts">
  import '#lib/fonts/fonts.css';
  import '#lib/styles/tokens.css';
  import '#lib/styles/legacy.css';
  import '#lib/styles/glass.css';
  import '#lib/styles/shell.css';
  import '#lib/styles/sidebar.css';
  import '#lib/styles/controls.css';
  import '#lib/styles/status.css';
  import '#lib/styles/client.css';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { engine } from '#lib/api/transport.ts';
  import { setRunSession } from '#lib/run/context.ts';
  import { RunSession } from '#lib/run/session.svelte.ts';
  import { ProjectStatus } from '#lib/shell/project.svelte.ts';
  import Shell from '#lib/shell/Shell.svelte';
  import { setShellSlots, ShellSlots } from '#lib/shell/slots.svelte.ts';
  import { setShellState, ShellState } from '#lib/shell/state.svelte.ts';
  import type { Snippet } from 'svelte';

  let { children }: { children: Snippet } = $props();
  // One run at a time for the whole app, so a run started on any screen shows on Runs.
  const session = new RunSession(engine);
  setRunSession(session);
  const shell = setShellState(new ShellState());
  const slots = setShellSlots(new ShellSlots());
  const project = new ProjectStatus(engine);
  void project.load();

  function navigate(href: string): void {
    // eslint-disable-next-line svelte/no-navigation-without-resolve -- href is a space's resolved route, or one the shell remembered
    void goto(href);
  }
</script>

<Shell
  {shell}
  {session}
  {project}
  pathname={page.url.pathname}
  search={page.url.search}
  {navigate}
  {slots}
>
  {@render children()}
</Shell>
