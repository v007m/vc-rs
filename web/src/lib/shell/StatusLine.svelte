<script lang="ts">
  import { checkLine, type ProjectStatus } from './project.svelte.ts';

  /** The status line on L0, under the card: vc's version, the project, and vc check. */
  let { project }: { project: ProjectStatus } = $props();

  let check = $derived(project.check);
  let tone = $derived(
    !check ? '' : check.errors ? 'st-fail' : check.warnings ? 'st-flaky' : 'st-pass',
  );
</script>

<footer class="statusline" aria-label="Status">
  <span class="item">vc <b>{project.version ?? '…'}</b></span>
  {#if project.name}<span class="item hide-narrow">project <b>{project.name}</b></span>{/if}
  <span class="grow"></span>
  <span class="item" title={project.error}>
    {#if tone}<span class="dot {tone}" aria-hidden="true"></span>{/if}
    {checkLine(check)}
  </span>
</footer>
