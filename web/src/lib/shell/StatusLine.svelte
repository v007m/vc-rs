<script lang="ts">
  import { checkLine, type ProjectStatus } from './project.svelte.ts';

  /**
   * The status line on L0, under the card: vc's version, the project, the git branch once the
   * engine gives its status (D532), and vc check.
   */
  let { project, branch }: { project: ProjectStatus; branch?: string | undefined } = $props();

  let check = $derived(project.check);
  let tone = $derived(
    !check ? '' : check.errors ? 'vc-fail' : check.warnings ? 'vc-flaky' : 'vc-pass',
  );
</script>

<footer class="statusline" aria-label="Status">
  <span class="item">vc <b>{project.version ?? '…'}</b></span>
  {#if project.name}<span class="item hide-narrow">project <b>{project.name}</b></span>{/if}
  {#if branch}<span class="item hide-narrow">branch <b class="vc-mono">{branch}</b></span>{/if}
  <span class="vc-grow"></span>
  <span class="item" title={project.error}>
    {#if tone}<span class="st-dot {tone}" aria-hidden="true"></span>{/if}
    {checkLine(check)}
  </span>
</footer>
