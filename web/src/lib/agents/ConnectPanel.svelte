<script lang="ts">
  import { copyText } from './clipboard.ts';
  import type { McpLink as Link } from './desk.svelte.ts';
  import McpLink from './McpLink.svelte';

  let { link, linkError }: { link: Link | undefined; linkError: string | undefined } = $props();

  /** How to connect each kind of agent; `vc mcp` runs over stdio, started by the agent. */
  const setups = [
    {
      label: 'Claude Code',
      lead: 'Run this in the project folder:',
      code: 'claude mcp add vibranium -- vc mcp',
    },
    {
      label: 'Cursor',
      lead: 'Add this to .cursor/mcp.json in the project:',
      code: '{\n  "mcpServers": {\n    "vibranium": { "command": "vc", "args": ["mcp"] }\n  }\n}',
    },
    { label: 'Other', lead: 'Add a stdio MCP server that runs:', code: 'vc mcp' },
    { label: 'HTTP', lead: 'While vc ui runs, an agent can use its MCP endpoint instead.' },
  ] as const;

  let label: string = $state(setups[0].label);
  let chosen = $derived(setups.find((s) => s.label === label) ?? setups[0]);
  let copied = $state('');

  async function copy(code: string): Promise<void> {
    copied = await copyText(code);
  }
</script>

<section class="connect" aria-labelledby="connect-h">
  <h2 id="connect-h" class="h2">Connect an agent</h2>
  <p class="lead">Once per project. The agent starts Vibranium’s MCP server when it needs it.</p>
  <div class="seg" role="group" aria-label="Agent">
    {#each setups as setup (setup.label)}
      <button
        type="button"
        aria-pressed={label === setup.label}
        onclick={() => {
          label = setup.label;
          copied = '';
        }}>{setup.label}</button
      >
    {/each}
  </div>
  <p>{chosen.lead}</p>
  {#if 'code' in chosen}
    {@const code = chosen.code}
    <div class="code">
      <pre class="mono">{code}</pre>
      <button class="btn sm" type="button" onclick={() => copy(code)}>Copy</button>
    </div>
    <p class="note" role="status">{copied}</p>
  {:else}
    <McpLink {link} error={linkError} />
  {/if}
  <p class="lead">
    The agent works under the default profile. Add <code>--profile &lt;name&gt;</code> to give it another;
    an agent cannot pick its own. Then try asking it: “Which tests failed in the last run, and why?”
  </p>
</section>

<style>
  .connect {
    display: grid;
    gap: 8px;
    justify-items: start;
  }
  p {
    margin: 0;
  }
  .lead,
  .note {
    color: var(--ink2);
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
  }
</style>
