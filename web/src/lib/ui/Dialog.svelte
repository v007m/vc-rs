<script lang="ts">
  import type { Snippet } from 'svelte';

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
  class="dialog"
  aria-labelledby="{uid}-title"
  {@attach modal}
  oncancel={(event) => {
    event.preventDefault();
    onclose();
  }}
>
  <div class="head">
    <h2 class="h2" id="{uid}-title">{title}</h2>
    <button type="button" class="btn sm" onclick={onclose}>Close</button>
  </div>
  {@render children()}
</dialog>

<style>
  /* L3: the strongest glass, over a dimmed backdrop (glass.css). */
  .dialog {
    width: min(640px, calc(100vw - 32px));
    max-height: calc(100vh - 64px);
    padding: var(--sp-5);
    border: 1px solid var(--l3-border);
    border-radius: var(--r-xl);
    background: var(--l3-bg);
    -webkit-backdrop-filter: blur(var(--blur-l3));
    backdrop-filter: blur(var(--blur-l3));
    box-shadow: var(--sh-l3);
    color: var(--ink);
  }
  .dialog::backdrop {
    background: var(--l3-backdrop);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: var(--sp-3);
  }
</style>
