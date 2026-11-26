<script lang="ts">
  import type { Snippet } from 'svelte';
  import { clampSplit, type ClientLayout } from './layout.svelte.ts';

  /**
   * The request and response cards with a divider between them (D561): drag it, or focus it and
   * use the arrow keys (Shift for bigger steps, Home and End for the limits).
   */
  let { layout, first, second }: { layout: ClientLayout; first: Snippet; second: Snippet } =
    $props();

  let box: HTMLDivElement | undefined = $state();
  let wide = $state(0);
  let tall = $state(0);
  /** Whether the cards are stacked; under `auto` the CSS decides, so the divider's shape says. */
  const stacked = $derived(layout.orient === 'auto' ? wide > tall : layout.orient === 'stack');
  const keys: Record<string, number> = { ArrowLeft: -2, ArrowUp: -2, ArrowRight: 2, ArrowDown: 2 };

  function onkeydown(event: KeyboardEvent): void {
    const step = keys[event.key];
    if (event.key === 'Home') layout.setSplit(20);
    else if (event.key === 'End') layout.setSplit(80);
    else if (step !== undefined) layout.setSplit(layout.split + step * (event.shiftKey ? 5 : 1));
    else return;
    event.preventDefault();
  }

  function share(event: PointerEvent): number {
    const rect = box?.getBoundingClientRect();
    if (!rect || rect.width === 0 || rect.height === 0) return layout.split;
    const across = stacked
      ? (event.clientY - rect.top) / rect.height
      : (event.clientX - rect.left) / rect.width;
    return clampSplit(across * 100);
  }

  function onpointerdown(event: PointerEvent): void {
    const divider = event.currentTarget as HTMLElement;
    divider.setPointerCapture(event.pointerId);
    const move = (e: PointerEvent) => layout.setSplit(share(e));
    const stop = () => {
      divider.removeEventListener('pointermove', move);
      divider.removeEventListener('pointerup', stop);
    };
    divider.addEventListener('pointermove', move);
    divider.addEventListener('pointerup', stop);
  }
</script>

<div
  class="split"
  data-orient={layout.orient === 'auto' ? undefined : layout.orient}
  style:--split="{layout.split}%"
  bind:this={box}
>
  {@render first()}
  <!-- A focusable separator is a widget: the APG window splitter pattern (D566). -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
  <div
    class="divider"
    role="separator"
    tabindex="0"
    aria-label="Resize request and response"
    aria-orientation={stacked ? 'horizontal' : 'vertical'}
    aria-valuemin={20}
    aria-valuemax={80}
    aria-valuenow={layout.split}
    title="Drag, or use the arrow keys"
    bind:clientWidth={wide}
    bind:clientHeight={tall}
    {onkeydown}
    {onpointerdown}
  ></div>
  {@render second()}
</div>
