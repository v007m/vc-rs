<script lang="ts">
  import type { Snippet } from 'svelte';

  let { title, children }: { title: string; children?: Snippet } = $props();
</script>

<svelte:head>
  <title>{title} · Vibranium</title>
</svelte:head>

<div class="panel-head">
  <h1 class="h1">{title}</h1>
</div>
<div class="panel-body">
  {@render children?.()}
</div>
