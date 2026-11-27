<script lang="ts">
  import Icon from '#lib/shell/Icon.svelte';
  import Menu, { type MenuItem } from '#lib/ui/Menu.svelte';
  import type { ClientLayout, Orient } from './layout.svelte.ts';

  /** Lays the request and response cards side by side, stacked, or by the window (D561). */
  let { layout }: { layout: ClientLayout } = $props();

  const choices: readonly { id: Orient; label: string }[] = [
    { id: 'auto', label: 'Follow the window' },
    { id: 'side', label: 'Side by side' },
    { id: 'stack', label: 'Stacked' },
  ];
  const items: readonly MenuItem[] = $derived(
    choices.map(({ id, label }) =>
      layout.orient === id ? { id, label, meta: 'current' } : { id, label },
    ),
  );

  function pick(id: string): void {
    const choice = choices.find((c) => c.id === id);
    if (choice) layout.setOrient(choice.id);
  }
</script>

<Menu label="Layout" {items} button="st-icon-btn" onpick={pick}>
  <Icon name={layout.orient === 'stack' ? 'rows' : 'columns'} small />
</Menu>
