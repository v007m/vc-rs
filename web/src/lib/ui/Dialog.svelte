<script lang="ts">
  import type { Snippet } from 'svelte';
  import Icon from '#lib/shell/Icon.svelte';

  let { title, onclose, children }: { title: string; onclose: () => void; children: Snippet } =
    $props();

  const uid = $props.id();

  /** Opens the dialog modally, so focus stays inside it and Escape closes it. */
  function modal(dialog: HTMLDialogElement): () => void {
    if (typeof dialog.showModal === 'function') dialog.showModal();
    else dialog.setAttribute('open', '');
    return () => dialog.close?.();
  }
</script>

<dialog
  class="dialog st-overlay"
  aria-labelledby="{uid}-title"
  {@attach modal}
  oncancel={(event) => {
    event.preventDefault();
    onclose();
  }}
>
  <div class="head">
    <h2 class="title" id="{uid}-title">{title}</h2>
    <button
      type="button"
      class="st-icon-btn st-icon-btn--sm"
      aria-label="Close"
      title="Close"
      onclick={onclose}><Icon name="x" /></button
    >
  </div>
  {@render children()}
</dialog>

<style>
  /* A Strata overlay over the scrim (dialog.st-overlay in vibranium.css): a Title2 title, the
     content, and the caller's actions at the bottom right, Cancel first. */
  .dialog {
    width: min(640px, calc(100vw - 32px));
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
