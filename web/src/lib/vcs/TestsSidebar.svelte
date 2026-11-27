<script lang="ts">
  import SidebarSection from '#lib/shell/SidebarSection.svelte';
  import { getShellState } from '#lib/shell/state.svelte.ts';
  import { spaceOf } from '#lib/shell/spaces.ts';
  import ChangesPane from './ChangesPane.svelte';
  import type { VcsSession } from './session.svelte.ts';

  /** The Tests space's sidebar (D516): its views, then Changes, the git pane (D518). */
  let { vcs, pathname }: { vcs: VcsSession; pathname: string } = $props();

  const shell = getShellState();
  const tests = spaceOf('/tests');
  const changed = $derived(vcs.files.length);
</script>

{#each tests.sections as section (section.id)}
  <SidebarSection {shell} space="tests" {section} {pathname} />
{/each}
<SidebarSection
  {shell}
  space="tests"
  section={{ id: 'changes', label: 'Changes' }}
  meta={changed ? String(changed) : undefined}
>
  <ChangesPane {vcs} />
</SidebarSection>
