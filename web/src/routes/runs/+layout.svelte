<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { page } from '$app/state';
  import { engine } from '#lib/api/transport.ts';
  import { engineRunsSeams } from '#lib/history/engine.ts';
  import { RunHistory } from '#lib/history/history.svelte.ts';
  import { enginePort } from '#lib/history/port.ts';
  import RunsSidebar from '#lib/history/RunsSidebar.svelte';
  import { RunsSpace, setRunsSpace } from '#lib/history/space.svelte.ts';
  import { engineLoadPort } from '#lib/load/engine.ts';
  import { LoadSession } from '#lib/load/session.svelte.ts';
  import { getRunSession } from '#lib/run/context.ts';
  import { engineEstimate } from '#lib/run/estimate.ts';
  import RunDialog from '#lib/run/RunDialog.svelte';
  import type { RunInit } from '#lib/run/session.svelte.ts';
  import { isRunKey } from '#lib/run/shortcut.ts';
  import { modAria, modHint } from '#lib/shell/keys.ts';
  import { getProjectStatus } from '#lib/shell/project.svelte.ts';
  import { getShellSlots } from '#lib/shell/slots.svelte.ts';
  import { untrack, type Snippet } from 'svelte';

  /** Every Runs screen shares its sidebar, the top row's **Run…** (⌘R) and its dialog (D630). */
  let { children }: { children: Snippet } = $props();

  const session = getRunSession();
  const project = getProjectStatus();
  const space = setRunsSpace(
    new RunsSpace(new RunHistory(enginePort(engine)), {
      seams: engineRunsSeams(engine),
      load: new LoadSession(engineLoadPort(engine)),
      estimate: engineEstimate(engine),
    }),
  );
  void space.history.load();

  $effect(() => getShellSlots().fill({ sidebar, primary, title: space.title }));

  // A run that ends lands in the history, so the sidebar and All runs read it again.
  let wasRunning = false;
  $effect(() => {
    const running = session.running;
    if (wasRunning && !running) untrack(() => void space.history.load());
    wasRunning = running;
  });

  function run(init: RunInit): void {
    space.dialog = undefined;
    void session.start(init);
    void goto(resolve('/runs/live'));
  }

  function onkeydown(event: KeyboardEvent): void {
    if (!isRunKey(event)) return;
    event.preventDefault();
    if (!space.dialog && !session.running) space.run();
  }
</script>

<svelte:window {onkeydown} />

{#snippet sidebar()}
  <RunsSidebar history={space.history} {session} pathname={page.url.pathname} />
{/snippet}

{#snippet primary()}
  <button
    type="button"
    class="st-btn st-btn--primary st-btn--sm"
    aria-keyshortcuts={modAria('R')}
    disabled={session.running}
    title={session.running ? 'A run is going: stop it, or wait for it to end' : undefined}
    onclick={() => space.run()}>Run… <span class="st-kbd">{modHint('R')}</span></button
  >
{/snippet}

{@render children()}

{#if space.dialog}
  <RunDialog
    profiles={project.profiles}
    profile={project.profile}
    selector={space.dialog.selector}
    estimate={space.ports.estimate}
    onrun={run}
    onclose={() => (space.dialog = undefined)}
  />
{/if}
