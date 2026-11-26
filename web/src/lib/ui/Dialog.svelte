<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from '#lib/shell/Icon.svelte';
  import { growFrom, shrinkInto } from './anchor.ts';

  let {
    title,
    onclose,
    width = 640,
    children,
  }: {
    title: string;
    onclose: () => void;
    /** Its width in px, at most the window less 32px (the Run dialog is 560, D632). */
    width?: number;
    children: Snippet;
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
  {@render children()}
</dialog>

<style>
  /* A Strata overlay over the scrim (dialog.st-overlay in vibranium.css): a Title2 title, the
     content, and the caller's actions at the bottom right, Cancel first. */
  .dialog {
    width: min(var(--dialog-w), calc(100vw - 32px));
    max-height: calc(100vh - 64px);
    padding: var(--st-space-24);
    overflow: auto;
    box-shadow: var(--st-edge-card), var(--st-shadow-xl);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--st-space-12);
    margin-bottom: var(--st-space-16);
  }
  .title {
    margin: 0;
    min-width: 0;
    font: var(--st-type-title2);
    letter-spacing: var(--st-tracking-title2);
    overflow-wrap: anywhere;
  }
</style>
