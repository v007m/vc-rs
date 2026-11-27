<script lang="ts">
  import '#lib/fonts/fonts.css';
  import '#lib/styles/strata/tokens.css';
  import '#lib/styles/strata/strata.css';
  import '#lib/styles/strata/vibranium.css';
  import '#lib/styles/shell.css';
  import '#lib/styles/sidebar.css';
  import '#lib/styles/client.css';
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { engine } from '#lib/api/transport.ts';
  import Onboarding from '#lib/onboarding/Onboarding.svelte';
  import { onboardingPort } from '#lib/onboarding/port.ts';
  import { setRunSession } from '#lib/run/context.ts';
  import { RunSession } from '#lib/run/session.svelte.ts';
  import { ProjectStatus, setProjectStatus } from '#lib/shell/project.svelte.ts';
  import Shell from '#lib/shell/Shell.svelte';
  import { setShellSlots, ShellSlots } from '#lib/shell/slots.svelte.ts';
  import { setShellState, ShellState } from '#lib/shell/state.svelte.ts';
  import { setToasts, Toasts } from '#lib/ui/toasts.svelte.ts';
  import { setVcsSession } from '#lib/vcs/context.ts';
  import { enginePort as vcsPort } from '#lib/vcs/port.ts';
  import { VcsSession } from '#lib/vcs/session.svelte.ts';
  import { untrack, type Snippet } from 'svelte';

  let { children }: { children: Snippet } = $props();
  // One run at a time for the whole app, so a run started on any screen shows on Runs.
  const session = new RunSession(engine);
  setRunSession(session);
  const shell = setShellState(new ShellState());
  const slots = setShellSlots(new ShellSlots());
  setToasts(new Toasts());
  const project = setProjectStatus(new ProjectStatus(engine));
  void project.load();
  const vcs = setVcsSession(new VcsSession(vcsPort(engine)));
  void vcs.refresh();
  // A pull changed the files on disk, so the project's check is stale.
  $effect(() => {
    if (vcs.revision > 0) untrack(() => void project.load());
  });

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
  branch={vcs.branch}
>
  {#if project.configured === false}
    <Onboarding {project} port={onboardingPort(engine)} />
  {:else}
    {@render children()}
  {/if}
</Shell>
