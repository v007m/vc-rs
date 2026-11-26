<script lang="ts">
  import { growFrom, shrinkInto } from '#lib/ui/anchor.ts';
  import Icon from './Icon.svelte';
  import { modHint } from './keys.ts';
  import { filterPalette, stepPalette, type PaletteItem } from './palette.ts';

  /**
   * The command palette (Strata 1.1, D620): a modal 12% from the top, 620px wide, a 40px field
   * over grouped results and a key-help footer. It grows out of the search field that opened
   * it and shrinks back into it. Arrows move through the results, Enter opens one, Escape closes.
   */
  let {
    items,
    anchor,
    onpick,
    onclose,
  }: {
    items: readonly PaletteItem[];
    /** What opened it (the top bar's search field): it grows from and shrinks into this. */
    anchor: Element | null;
    onpick: (href: string) => void;
    onclose: () => void;
  } = $props();

  const uid = $props.id();
  let query = $state('');
  let groups = $derived(filterPalette(items, query));
  let chosen: string | undefined = $state();
  let active = $derived(
    groups.some((group) => group.items.some((item) => item.id === chosen))
      ? chosen
      : stepPalette(groups, undefined, 1),
  );
  let frame: HTMLDialogElement | undefined;
  let field: HTMLInputElement | undefined = $state();

  const optionId = (item: PaletteItem): string => `${uid}-${item.id.replace(/[^\w-]/g, '_')}`;
  const activeItem = $derived(groups.flatMap((group) => group.items).find((i) => i.id === active));

  function modal(dialog: HTMLDialogElement): () => void {
    frame = dialog;
    if (typeof dialog.showModal === 'function') dialog.showModal();
    else dialog.setAttribute('open', '');
    growFrom(dialog, anchor);
    field?.focus();
    return () => dialog.close?.();
  }

  function close(): void {
    if (frame) shrinkInto(frame, anchor, onclose);
    else onclose();
  }

  function pick(item: PaletteItem | undefined): void {
    if (!item) return;
    onpick(item.href);
    close();
  }

  function keys(event: KeyboardEvent): void {
    const step: Record<string, number> = { ArrowDown: 1, ArrowUp: -1 };
    if (event.key in step) chosen = stepPalette(groups, active, step[event.key] ?? 0);
    else if (event.key === 'Enter') pick(activeItem);
    else return;
    event.preventDefault();
  }

  // The active result stays in view as the arrows move through a long list.
  $effect(() => {
    if (activeItem)
      document.getElementById(optionId(activeItem))?.scrollIntoView?.({ block: 'nearest' });
  });
</script>

<dialog
  class="palette st-overlay vc-anchored"
  aria-label="Search or run"
  {@attach modal}
  oncancel={(event) => {
    event.preventDefault();
    close();
  }}
  onclick={(event) => {
    if (event.target === event.currentTarget) close();
  }}
>
  <div class="find st-input">
    <Icon name="search" />
    <input
      bind:this={field}
      bind:value={query}
      role="combobox"
      aria-label="Search or run"
      aria-expanded="true"
      aria-controls="{uid}-list"
      aria-autocomplete="list"
      aria-activedescendant={activeItem ? optionId(activeItem) : undefined}
      autocomplete="off"
      spellcheck="false"
      placeholder="Search screens…"
      onkeydown={keys}
    />
  </div>
  <div class="results" id="{uid}-list" role="listbox" aria-label="Results">
    {#each groups as group, g (group.group)}
      <div role="group" aria-labelledby="{uid}-g{g}">
        <p class="group" id="{uid}-g{g}">{group.group}</p>
        {#each group.items as item (item.id)}
          <div
            class="vc-palette-item"
            role="option"
            id={optionId(item)}
            tabindex="-1"
            aria-selected={item.id === active}
            onpointermove={() => (chosen = item.id)}
            onclick={() => pick(item)}
            onkeydown={keys}
          >
            <span class="label">{item.label}</span>
            {#if item.shortcut}<span class="st-kbd" aria-hidden="true"
                >{modHint(item.shortcut)}</span
              >{/if}
          </div>
        {/each}
      </div>
    {:else}
      <p class="none" role="status">Nothing matches “{query.trim()}”.</p>
    {/each}
  </div>
  <p class="keys" aria-hidden="true">
    <span><span class="st-kbd">↑</span><span class="st-kbd">↓</span> move</span>
    <span><span class="st-kbd">↵</span> open</span>
    <span><span class="st-kbd">esc</span> close</span>
  </p>
</dialog>

<style>
  /* Concentric: the palette is 22 with 8 padding, so a result is 14 (vibranium.css part 5). */
  .palette {
    width: min(620px, calc(100vw - 32px));
    max-height: min(560px, calc(100vh - 24vh));
    margin: 12vh auto auto;
    padding: var(--st-space-8);
    border-radius: var(--vc-radius-palette);
    overflow: hidden;
    flex-direction: column;
    gap: var(--st-space-8);
    box-shadow: var(--st-edge-card), var(--st-shadow-xl);
  }
  .palette[open] {
    display: flex;
  }
  .find {
    flex: none;
    display: flex;
    align-items: center;
    gap: var(--st-space-8);
    height: var(--st-size-control-lg);
    padding: 0 var(--st-space-16);
    color: var(--st-text-secondary);
  }
  .find input {
    flex: 1 1 auto;
    min-width: 0;
    height: 100%;
    border: 0;
    padding: 0;
    background: transparent;
    font: var(--st-type-callout);
    color: var(--st-text-primary);
  }
  .results {
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
    display: grid;
    align-content: start;
    gap: var(--st-space-4);
  }
  .group {
    margin: var(--st-space-6) 0 var(--st-space-2);
    padding: 0 var(--st-space-12);
    font: var(--st-type-label);
    letter-spacing: var(--st-tracking-label);
    color: var(--st-text-tertiary);
  }
  .vc-palette-item {
    display: flex;
    align-items: center;
    gap: var(--st-space-8);
    min-height: var(--st-size-control-lg);
    padding: 0 var(--st-space-12);
    border-radius: var(--vc-radius-palette-item);
    color: var(--st-text-primary);
    cursor: default;
  }
  .vc-palette-item[aria-selected='true'] {
    background: var(--st-accent-soft);
  }
  .label {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .none {
    margin: 0;
    padding: var(--st-space-16) var(--st-space-12);
    color: var(--st-text-secondary);
  }
  .keys {
    flex: none;
    display: flex;
    gap: var(--st-space-16);
    margin: 0 var(--st-space-12);
    padding: var(--st-space-8) 0 var(--st-space-4);
    border-top: 1px solid var(--st-chrome-separator);
    font: var(--st-type-caption);
    color: var(--st-text-tertiary);
  }
  .keys > span {
    display: inline-flex;
    align-items: center;
    gap: var(--st-space-4);
  }
</style>
