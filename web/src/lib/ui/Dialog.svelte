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
  .dialog {
    width: min(640px, calc(100vw - 32px));
    max-height: calc(100vh - 64px);
    padding: 18px;
    border: 1px solid var(--line2);
    border-radius: 12px;
    background: var(--panel);
    color: var(--ink);
  }
  .dialog::backdrop {
    background: rgb(20 18 35 / 40%);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 12px;
  }
</style>
