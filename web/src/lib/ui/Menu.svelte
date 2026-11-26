<script lang="ts" module>
  export type MenuItem = Readonly<{ id: string; label: string; meta?: string; disabled?: boolean }>;
</script>

<script lang="ts">
  import { tick, type Snippet } from 'svelte';

  /**
   * A button that opens a short menu of actions (WAI-ARIA menu button): arrows move, Enter
   * picks, Escape closes and gives focus back to the button.
   */
  let {
    label,
    items,
    onpick,
    button = 'btn',
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

  function entries(): HTMLElement[] {
    return [...(root?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [])];
  }

  async function show(): Promise<void> {
    open = true;
    await tick();
    entries()[0]?.focus();
  }

  function hide(refocus = true): void {
    open = false;
    if (refocus) trigger?.focus();
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
    if (open && !root?.contains(event.target as Node)) hide(false);
  }}
/>

<span class="menu" bind:this={root}>
  <button
    type="button"
    class={button}
    aria-label={label}
    aria-haspopup="menu"
    aria-expanded={open}
    aria-controls="{uid}-menu"
    bind:this={trigger}
    onclick={() => (open ? hide() : void show())}
  >
    {@render children()}
  </button>
  {#if open}
    <div
      class="pop l3"
      role="menu"
      id="{uid}-menu"
      aria-label={label}
      tabindex="-1"
      onkeydown={move}
    >
      {#each items as item (item.id)}
        <button
          type="button"
          role="menuitem"
          class="pop-item"
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
  .pop {
    position: absolute;
    z-index: 30;
    top: calc(100% + 6px);
    right: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: max-content;
    min-width: 200px;
    max-width: min(320px, calc(100vw - 24px));
    padding: var(--sp-2);
    border-radius: var(--r-lg);
  }
  .pop-item {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    min-height: 36px;
    padding: 6px var(--sp-3);
    border: 0;
    border-radius: var(--r-sm);
    background: transparent;
    color: var(--ink);
    font-size: var(--fs-2);
    text-align: left;
  }
  .pop-item:is(:hover, :focus-visible) {
    background: var(--row-hover);
    outline: none;
  }
  .pop-item[aria-disabled='true'] {
    color: var(--ink-disabled);
    cursor: not-allowed;
  }
  .meta {
    margin-left: auto;
    color: var(--ink-3);
    font-size: var(--fs-1);
  }
</style>
