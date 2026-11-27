<script lang="ts">
  import { choiceLabels, choices, type Accent } from '#lib/shell/appearance.ts';
  import type { ShellState } from '#lib/shell/state.svelte.ts';
  import { lens } from '#lib/ui/segmented.ts';

  /**
   * Settings › Appearance (D722): theme, accent, density and motion for this app in this
   * browser only, applied at once. Each is a flat row with an inset hairline: the setting and
   * what it does on the left, its control on the right; the accent's swatches sit under it.
   */
  let { shell }: { shell: ShellState } = $props();

  const uid = $props.id();
  const look = $derived(shell.prefs.appearance);
  const label = (choice: string): string => choiceLabels[choice] ?? choice;
</script>

<div class="appearance">
  <p class="intro">How this app looks, in this browser only. Changes apply at once.</p>

  <div class="row">
    <div class="what">
      <p class="name" id="{uid}-theme">Theme</p>
      <p class="say">Light or dark, or follow the system's setting.</p>
    </div>
    <div class="st-segmented" role="group" aria-labelledby="{uid}-theme" {@attach lens}>
      {#each choices.theme as theme (theme)}
        <button
          type="button"
          aria-pressed={look.theme === theme}
          onclick={() => shell.setAppearance({ theme })}>{label(theme)}</button
        >
      {/each}
    </div>
  </div>

  <div class="row stack">
    <div class="what">
      <p class="name" id="{uid}-accent">Accent</p>
      <p class="say">Each space keeps its own colour, or one colour goes on every space.</p>
    </div>
    <div class="swatches" role="radiogroup" aria-labelledby="{uid}-accent">
      {#each choices.accent as accent (accent)}
        <label class="swatch">
          <input
            type="radio"
            class="vc-radio"
            name="{uid}-accent"
            checked={look.accent === accent}
            onchange={() => shell.setAppearance({ accent: accent as Accent })}
          />
          {#if accent !== 'space'}<span class="chip" data-space={accent} aria-hidden="true"
            ></span>{/if}
          {label(accent)}
        </label>
      {/each}
    </div>
  </div>

  <div class="row">
    <div class="what">
      <p class="name" id="{uid}-density">Density</p>
      <p class="say">Compact fits more rows in lists, tables and menus.</p>
    </div>
    <div class="st-segmented" role="group" aria-labelledby="{uid}-density" {@attach lens}>
      {#each choices.density as density (density)}
        <button
          type="button"
          aria-pressed={look.density === density}
          onclick={() => shell.setAppearance({ density })}>{label(density)}</button
        >
      {/each}
    </div>
  </div>

  <div class="row">
    <div class="what">
      <label class="name" for="{uid}-motion">Reduce motion</label>
      <p class="say">
        Turns off springs, fades and the live pulse. Motion is also reduced while the system asks
        for it.
      </p>
    </div>
    <input
      id="{uid}-motion"
      type="checkbox"
      class="st-switch"
      checked={look.motion === 'reduced'}
      onchange={(event) =>
        shell.setAppearance({ motion: event.currentTarget.checked ? 'reduced' : 'system' })}
    />
  </div>
</div>

<style>
  .appearance {
    display: grid;
    max-width: 960px;
  }
  .intro {
    margin: 0 0 12px;
    color: var(--st-text-secondary);
  }
  /* a flat row, its hairline 12px short of both ends (D669) */
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    min-width: 0;
    padding: 14px 12px;
    background: linear-gradient(var(--st-chrome-separator), var(--st-chrome-separator)) no-repeat
      12px 100% / calc(100% - 24px) 1px;
  }
  /* six swatches do not fit beside their words: they take the row under them, so none is
     left alone on a line of its own (D741) */
  .stack {
    flex-direction: column;
    align-items: stretch;
    gap: 10px;
  }
  .what {
    min-width: 0;
  }
  .name {
    margin: 0;
    font: var(--st-type-headline);
    color: var(--st-text-primary);
  }
  .say {
    margin: 2px 0 0;
    color: var(--st-text-secondary);
  }
  .swatches {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 20px;
  }
  .swatch {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
  }
  .chip {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--st-accent);
  }
</style>
