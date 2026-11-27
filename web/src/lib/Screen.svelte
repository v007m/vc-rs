<script lang="ts">
  import type { Snippet } from 'svelte';

  /** A screen's body on the shell's L1 card; `title` names the page (the top row shows the view). */
  let { title, children }: { title: string; children?: Snippet } = $props();
</script>

<svelte:head>
  <title>{title} · Vibranium</title>
</svelte:head>

<div class="card-body">
  {@render children?.()}
</div>
