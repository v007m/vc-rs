<script lang="ts">
  import Screen from '#lib/Screen.svelte';
  import { lens } from '#lib/ui/segmented.ts';
  import { getToasts, type Toasts } from '#lib/ui/toasts.svelte.ts';
  import DemoSetup from './DemoSetup.svelte';
  import InitWizard from './InitWizard.svelte';
  import type { OnboardingPort } from './port.ts';
  import type { Reload } from './steps.ts';

  /**
   * What the stage shows while the folder `vc ui` serves has no `vibranium.config.yaml` (D716):
   * set up a project of one's own, or try the demo. Once either is written the project loads
   * again and the app takes its place.
   */
  let {
    port,
    project,
    toasts = getToasts(),
  }: { port: OnboardingPort; project: Reload; toasts?: Toasts } = $props();

  const choices = [
    { key: 'init', label: 'Set up this folder' },
    { key: 'demo', label: 'Try the demo' },
  ] as const;
  let chosen: 'init' | 'demo' = $state('init');
</script>

<Screen title="Get started">
  <section class="onboarding" aria-labelledby="onboarding-h">
    <h2 id="onboarding-h" class="st-title2">Get started with Vibranium</h2>
    <p class="lead">
      This folder has no <code>vibranium.config.yaml</code> yet. Set up a project of your own here, or
      try the demo to see Vibranium at work first.
    </p>
    <div class="st-segmented" role="group" {@attach lens} aria-label="How to start">
      {#each choices as choice (choice.key)}
        <button
          type="button"
          aria-pressed={chosen === choice.key}
          onclick={() => (chosen = choice.key)}>{choice.label}</button
        >
      {/each}
    </div>
    {#if chosen === 'init'}
      <InitWizard {port} {project} {toasts} />
    {:else}
      <DemoSetup {port} {project} {toasts} />
    {/if}
  </section>
</Screen>

<style>
  .onboarding {
    display: grid;
    gap: 16px;
    justify-items: stretch;
    max-width: 760px;
  }
  .lead {
    margin: 0;
    color: var(--st-text-secondary);
  }
  .st-segmented {
    justify-self: start;
  }
</style>
