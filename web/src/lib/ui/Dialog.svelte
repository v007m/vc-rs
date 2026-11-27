<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from '#lib/shell/Icon.svelte';
  import { growFrom, shrinkInto } from './anchor.ts';

  let {
    title,
    onclose,
    width = 640,
    onsubmit,
    children,
    actions,
  }: {
    title: string;
    onclose: () => void;
    /** Its width in px, at most the window less 32px (the Run dialog is 560, D632). */
    width?: number;
    /** Makes the body and the actions one form, so Enter and a submit button send it. */
    onsubmit?: (event: SubmitEvent) => void;
    children: Snippet;
    /** The buttons at the bottom right, Cancel first; they stay in view while the body scrolls. */
    actions?: Snippet;
  } = $props();

  const uid = $props.id();
  let frame: HTMLDialogElement | undefined;
  /** What opened the dialog: it grows out of it and shrinks back into it (D618). */
  let anchor: Element | null = null;

  /** Opens the dialog modally, so focus stays inside it and Escape closes it. */
  function modal(dialog: HTMLDialogElement): () => void {
    frame = dialog;
    anchor = document.activeElement;
    if (typeof dialog.showModal === 'function') dialog.showModal();
    else dialog.setAttribute('open', '');
    growFrom(dialog, anchor);
    return () => dialog.close?.();
  }

  /** The close button and Escape shrink the dialog into its anchor first. */
  function close(): void {
    if (frame) shrinkInto(frame, anchor, onclose);
    else onclose();
  }
</script>

{#snippet content()}
  <div class="body">{@render children()}</div>
  {#if actions}<div class="actions">{@render actions()}</div>{/if}
{/snippet}

<dialog
  class="dialog st-overlay vc-anchored"
  aria-labelledby="{uid}-title"
  style:--dialog-w="{width}px"
  {@attach modal}
  oncancel={(event) => {
    event.preventDefault();
    close();
  }}
>
  <div class="head">
    <h2 class="title" id="{uid}-title">{title}</h2>
    <button
      type="button"
      class="st-icon-btn st-icon-btn--sm"
      aria-label="Close"
      title="Close"
      onclick={close}><Icon name="x" /></button
    >
  </div>
  {#if onsubmit}
    <form class="frame" {onsubmit} novalidate>{@render content()}</form>
  {:else}
    <div class="frame">{@render content()}</div>
  {/if}
</dialog>

<style>
  /* A Strata overlay over the scrim (dialog.st-overlay in vibranium.css): a Title2 title, the
     body, and the caller's actions at the bottom right, Cancel first. The title and the
     actions stay put; only the body scrolls, so the actions never fall below the fold (D742). */
  .dialog {
    width: min(var(--dialog-w), calc(100vw - 32px));
    max-height: calc(100vh - 64px);
    padding: 0;
    overflow: hidden;
    box-shadow: var(--st-edge-card), var(--st-shadow-xl);
  }
  .dialog[open] {
    display: flex;
    flex-direction: column;
  }
  .head {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    gap: var(--st-space-12);
    padding: var(--st-space-24) var(--st-space-24) var(--st-space-16);
  }
  .title {
    margin: 0;
    min-width: 0;
    font: var(--st-type-title2);
    letter-spacing: var(--st-tracking-title2);
    overflow-wrap: anywhere;
  }
  .frame {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    min-height: 0;
  }
  /* 4px of slack inside the scroller keeps a focus ring at its edge from being clipped */
  .body {
    flex: 1 1 auto;
    min-height: 0;
    margin: -4px 0;
    padding: 4px var(--st-space-24);
    overflow: auto;
  }
  .body:last-child {
    margin-bottom: 0;
    padding-bottom: var(--st-space-24);
  }
  .actions {
    display: flex;
    flex: none;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: var(--st-space-8);
    padding: var(--st-space-16) var(--st-space-24) var(--st-space-24);
  }
</style>
