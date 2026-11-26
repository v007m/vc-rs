<script lang="ts">
  import { MASKED } from '#lib/settings/port.ts';
  import Icon from '#lib/shell/Icon.svelte';
  import Menu, { type MenuItem } from '#lib/ui/Menu.svelte';
  import { METHODS } from './apiForm.ts';
  import CurlDialog from './CurlDialog.svelte';
  import type { ClientLayout } from './layout.svelte.ts';
  import LayoutMenu from './LayoutMenu.svelte';
  import type { CallType } from './seams.ts';
  import type { ClientSession } from './session.svelte.ts';

  /**
   * The request bar on L0 (ui.md §4, D563): the method, the URL, and Send with its menu (Send
   * and keep, Copy as cURL). A scratch tab picks its type here; a stream connects in place of
   * a send. ⌘↵ or Ctrl+↵ sends from anywhere on the screen. The layout menu ends the bar.
   */
  let { client, layout }: { client: ClientSession; layout?: ClientLayout } = $props();

  let curl = $state(false);
  const tab = $derived(client.tab);
  const scratch = $derived(tab?.kind === 'scratch' ? tab : undefined);
  const stream = $derived(scratch && scratch.type !== 'http' ? scratch.stream : undefined);
  const types: readonly { id: CallType; label: string }[] = [
    { id: 'http', label: 'HTTP' },
    { id: 'sse', label: 'SSE' },
    { id: 'websocket', label: 'WebSocket' },
  ];
  const items: readonly MenuItem[] = $derived([
    { id: 'send', label: 'Send', meta: '⌘↵' },
    { id: 'keep', label: 'Send and keep', meta: 'skips cleanup', disabled: !client.call },
    { id: 'curl', label: 'Copy as cURL', disabled: !client.call },
  ]);

  async function pick(id: string): Promise<void> {
    if (id === 'send') await client.send();
    else if (id === 'keep') await client.send(true);
    else if (await client.explain()) curl = true;
  }

  function onkeydown(event: KeyboardEvent): void {
    if (event.key !== 'Enter' || !(event.ctrlKey || event.metaKey) || !tab) return;
    event.preventDefault();
    if (stream?.live) return;
    void client.send();
  }
</script>

<svelte:window {onkeydown} />

{#if tab}
  <div class="reqbar on-l0" role="group" aria-label="Request">
    {#if scratch}
      <label class="sr" for="request-type">Request type</label>
      <select id="request-type" class="select" bind:value={scratch.type} disabled={stream?.live}>
        {#each types as type (type.id)}<option value={type.id}>{type.label}</option>{/each}
      </select>
      {#if scratch.type === 'http'}
        {@render methods(scratch.method, (m) => (scratch.method = m))}
      {/if}
      <div class="url">
        <input
          aria-label="URL"
          autocomplete="off"
          spellcheck="false"
          placeholder={'Paste a URL, or type ${baseUrl}/…'}
          bind:value={scratch.url}
        />
      </div>
    {:else if client.request}
      {@const form = client.request.form}
      {@render methods(form.method, (m) => (form.method = m))}
      <div class="url">
        <label class="sr" for="request-path">Path</label>
        <input
          id="request-path"
          autocomplete="off"
          spellcheck="false"
          readonly={form.path.includes(MASKED)}
          bind:value={form.path}
        />
      </div>
    {/if}
    {#if stream}
      {#if stream.live}
        <button type="button" class="btn" onclick={() => void stream.close()}>Disconnect</button>
      {:else}
        <button
          type="button"
          class="btn primary"
          disabled={!scratch?.url.trim()}
          onclick={() => void client.send()}>Connect</button
        >
      {/if}
    {:else}
      <div class="btn-split">
        <button
          type="button"
          class="btn primary"
          title="Send (Ctrl+Enter or Cmd+Enter)"
          disabled={client.sending}
          aria-busy={client.sending}
          onclick={() => void client.send()}
          ><Icon name="send" small />{client.sending ? 'Sending…' : 'Send'}<span
            class="kbd"
            aria-hidden="true">⌘↵</span
          ></button
        >
        <Menu label="More send options" {items} button="btn primary" onpick={(id) => void pick(id)}>
          <Icon name="chevron" small />
        </Menu>
      </div>
    {/if}
    {#if layout}<LayoutMenu {layout} />{/if}
  </div>
{/if}

{#snippet methods(value: string, set: (method: string) => void)}
  <label class="sr" for="request-method">Method</label>
  <select
    id="request-method"
    class="select"
    {value}
    onchange={(event) => set(event.currentTarget.value)}
  >
    {#each METHODS as method (method)}<option value={method}>{method}</option>{/each}
  </select>
{/snippet}

{#if curl && client.request?.explained}
  <CurlDialog curl={client.request.explained.curl} onclose={() => (curl = false)} />
{/if}
