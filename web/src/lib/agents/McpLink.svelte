<script lang="ts">
  import { MASKED } from '#lib/settings/port.ts';
  import { copyText } from './clipboard.ts';
  import type { McpLink } from './desk.svelte.ts';

  let { link, error }: { link: McpLink | undefined; error: string | undefined } = $props();
  let shown = $state(false);
  let copied = $state('');

  /** The MCP server entry for this vc ui, as Claude Code's `.mcp.json` takes it. */
  function config(url: string, token: string): string {
    const server = { type: 'http', url, headers: { Authorization: `Bearer ${token}` } };
    return JSON.stringify({ mcpServers: { vibranium: server } }, null, 2);
  }

  async function copy(text: string): Promise<void> {
    copied = await copyText(text);
  }
</script>

{#if link}
  <dl class="fields">
    <dt>MCP URL</dt>
    <dd>
      <code class="mono">{link.url}</code>
      <button
        class="btn sm"
        type="button"
        aria-label="Copy the MCP URL"
        onclick={() => copy(link.url)}>Copy</button
      >
    </dd>
    <dt>Agent token</dt>
    <dd>
      <code class="mono">{shown ? link.token : MASKED}</code>
      <button class="btn sm" type="button" aria-pressed={shown} onclick={() => (shown = !shown)}
        >Show token</button
      >
      <button
        class="btn sm"
        type="button"
        aria-label="Copy the agent token"
        onclick={() => copy(link.token)}>Copy</button
      >
    </dd>
  </dl>
  <p>Add this server to the agent’s MCP config, such as <code>.mcp.json</code> for Claude Code:</p>
  <div class="code">
    <pre class="mono">{config(link.url, shown ? link.token : MASKED)}</pre>
    <button
      class="btn sm"
      type="button"
      aria-label="Copy the MCP config"
      onclick={() => copy(config(link.url, link.token))}>Copy</button
    >
  </div>
  <p class="lead">
    The token changes each time vc ui starts. It opens only <code>/mcp</code>, where the agent
    policy applies; keep it out of the repo.
  </p>
{:else if error}
  <p class="error" role="alert">Could not load the agent token: {error}</p>
{:else}
  <p class="lead">Loading the agent token…</p>
{/if}
<p class="note" role="status">{copied}</p>

<style>
  p {
    margin: 0;
  }
  .lead,
  .note {
    color: var(--ink2);
  }
  .error {
    color: var(--fail);
  }
  .fields {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 8px 16px;
    align-items: center;
    margin: 0;
  }
  dd {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    margin: 0;
    min-width: 0;
  }
  code {
    overflow-wrap: anywhere;
  }
  .code {
    display: flex;
    gap: 8px;
    align-items: start;
    width: 100%;
  }
  pre {
    flex: 1;
    margin: 0;
    padding: 12px;
    border-radius: 8px;
    background: var(--panel2);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
</style>
