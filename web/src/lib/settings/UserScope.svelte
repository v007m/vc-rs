<script lang="ts">
  import { lens } from '#lib/ui/segmented.ts';
  import { scopeOf, type SettingsScope } from './userScope.ts';

  /**
   * Settings' scope switch in the top row: the project's settings or your own. Neither is
   * pressed on a page of this app alone (Appearance).
   */
  let { pathname, onchoose }: { pathname: string; onchoose: (scope: SettingsScope) => void } =
    $props();

  let scope = $derived(scopeOf(pathname));
  const options: readonly [SettingsScope, string][] = [
    ['project', 'Project'],
    ['user', 'You only'],
  ];
</script>

<div class="st-segmented" role="group" aria-label="Settings scope" {@attach lens}>
  {#each options as [value, label] (value)}
    <button type="button" aria-pressed={scope === value} onclick={() => onchoose(value)}
      >{label}</button
    >
  {/each}
</div>
