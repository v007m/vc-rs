<script lang="ts" module>
  export type MenuItem = Readonly<{ id: string; label: string; meta?: string; disabled?: boolean }>;
</script>

<script lang="ts">
  import { tick, type Snippet } from 'svelte';
  import { growFrom, shrinkInto } from './anchor.ts';

  /**
   * A button that opens a short menu of actions (WAI-ARIA menu button): arrows move, Enter
   * picks, Escape closes and gives focus back to the button. The menu grows out of its button
   * and shrinks back into it (Strata 1.1, D618).
   */
  let {
    label,
    items,
    onpick,
    button = 'st-btn',
    children,
  }: {
    label: string;
    items: readonly MenuItem[];
    onpick: (id: string) => void;
    /** The trigger's classes. */
    button?: string;
    children: Snippet;
  } = $props();

  const uid = $props.id();
  let open = $state(false);
  let root: HTMLElement | undefined = $state();
  let trigger: HTMLButtonElement | undefined = $state();
  let pop: HTMLElement | undefined = $state();
  /** Shrinking back into the button: still shown, but already closed for the button and keys. */
  let closing = $state(false);
  let up = $state(false);

  function entries(): HTMLElement[] {
    return [...(root?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
  }

  async function show(): Promise<void> {
    const fresh = !pop;
    open = true;
    closing = false;
    await tick();
    if (fresh) await flip();
    if (pop) growFrom(pop, trigger);
    entries()[0]?.focus();
  }

  /** Opens upward when the menu would run off the bottom of the window (a trigger low on the page). */
  async function flip(): Promise<void> {
    up = false;
    await tick();
    up = !!pop && pop.getBoundingClientRect().bottom > window.innerHeight - 8;
    await tick();
  }

  function hide(refocus = true): void {
    if (refocus) trigger?.focus();
    if (!pop) {
      open = false;
      return;
    }
    closing = true;
    shrinkInto(pop, trigger, () => {
      open = false;
      closing = false;
    });
  }

  function pick(item: MenuItem): void {
    if (item.disabled) return;
    hide();
    onpick(item.id);
  }

  function move(event: KeyboardEvent): void {
    const all = entries();
    const at = all.indexOf(document.activeElement as HTMLElement);
    const step: Record<string, number> = { ArrowDown: 1, ArrowUp: -1 };
    if (event.key === 'Escape') hide();
    else if (event.key in step)
      all[(at + (step[event.key] ?? 0) + all.length) % all.length]?.focus();
    else return;
    event.preventDefault();
  }
</script>

<svelte:window
  onpointerdown={(event) => {
    if (open && !closing && !root?.contains(event.target as Node)) hide(false);
  }}
/>

<span class="menu" bind:this={root}>
  <button
    type="button"
    class={button}
    aria-label={label}
    aria-haspopup="menu"
    aria-expanded={open && !closing}
    aria-controls="{uid}-menu"
    bind:this={trigger}
    onclick={() => (open && !closing ? hide() : void show())}
  >
    {@render children()}
  </button>
  {#if open}
    <div
      class="pop st-overlay st-menu vc-anchored"
      class:up
      role="menu"
      bind:this={pop}
      inert={closing}
      id="{uid}-menu"
      aria-label={label}
      tabindex="-1"
      onkeydown={move}
    >
      {#each items as item (item.id)}
        <button
          type="button"
          role="menuitem"
          class="st-menu-item"
          tabindex="-1"
          aria-disabled={item.disabled}
          onclick={() => pick(item)}
        >
          {item.label}
          {#if item.meta}<span class="meta">{item.meta}</span>{/if}
        </button>
      {/each}
    </div>
  {/if}
</span>

<style>
  .menu {
    position: relative;
    display: inline-flex;
  }
  /* Strata's menu recipe on an overlay; only the anchoring is local. */
  .pop {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    width: max-content;
    max-width: min(320px, calc(100vw - 24px));
  }
  .pop.up {
    top: auto;
    bottom: calc(100% + 6px);
  }
  .st-menu-item:focus-visible {
    background: var(--st-accent-soft);
  }
  .st-menu-item[aria-disabled='true'] {
    color: var(--st-text-disabled);
    background: transparent;
  }
  .meta {
    margin-left: auto;
    color: var(--st-text-tertiary);
    font: var(--st-type-caption);
  }
</style>
