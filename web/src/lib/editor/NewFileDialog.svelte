<script lang="ts">
  import { ConnectError } from '@connectrpc/connect';
  import { untrack } from 'svelte';
  import Dialog from '#lib/ui/Dialog.svelte';
  import type { FileOps, Template } from './seams.ts';

  /** New service, api file or test file, from the templates `vc new` uses (ui.md §5). */
  let {
    files,
    service: start,
    onclose,
    oncreated,
  }: {
    files: FileOps;
    /** The service of the open file, the default home of a new file. */
    service: string;
    onclose: () => void;
    oncreated: (path: string) => void;
  } = $props();

  const uid = $props.id();
  let template = $state<Template>('test');
  let service = $state(untrack(() => start));
  let name = $state('');
  let busy = $state(false);
  let note: string | undefined = $state();
  let ready = $derived(name.trim() !== '' && (template === 'service' || service.trim() !== ''));

  async function create(): Promise<void> {
    busy = true;
    note = undefined;
    try {
      const made = await files.create(template, name.trim(), service.trim());
      if (made.available) oncreated(made.value);
      else note = made.reason;
    } catch (error) {
      note = ConnectError.from(error).rawMessage;
    } finally {
      busy = false;
    }
  }
</script>

<Dialog title="New file" {onclose}>
  <div class="form">
    <label for="{uid}-kind">Kind</label>
    <select id="{uid}-kind" class="select" bind:value={template}>
      <option value="test">Test file</option>
      <option value="api">Api file</option>
      <option value="service">Service</option>
    </select>
    {#if template !== 'service'}
      <label for="{uid}-service">Service</label>
      <input id="{uid}-service" class="input mono" bind:value={service} />
    {/if}
    <label for="{uid}-name">{template === 'test' ? 'Area' : 'Name'}</label>
    <input id="{uid}-name" class="input mono" bind:value={name} placeholder="crud" />
  </div>
  {#if note}<p class="muted" role="status">{note}</p>{/if}
  <div class="actions">
    <button type="button" class="btn" onclick={onclose}>Cancel</button>
    <button
      type="button"
      class="btn primary"
      disabled={!ready || busy}
      onclick={() => void create()}>{busy ? 'Creating…' : 'Create'}</button
    >
  </div>
</Dialog>

<style>
  .form {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    align-items: center;
    gap: 8px 12px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
</style>
