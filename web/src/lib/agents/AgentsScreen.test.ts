import { Code, ConnectError } from '@connectrpc/connect';
import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { fakeEngine } from '#lib/testing/engine.ts';
import AgentsScreen from './AgentsScreen.svelte';
import { AgentDesk } from './desk.svelte.ts';

const actions = [
  {
    agent: 'Claude Code',
    tool: 'run_tests',
    profile: 'dev',
    arguments: '{"ids":["users.crud"]}',
    result: '3 passed',
    atMs: BigInt(Date.now() - 120_000),
  },
  {
    agent: 'Cursor',
    tool: 'get_test',
    arguments: '{"id":"users.crud#createsUser"}',
    result: 'ok',
    approvedBy: 'you',
  },
];

const mcpUrl = 'http://127.0.0.1:7420/mcp';
const agentToken = 'agent-token-0123456789';

function setup(fail = false, noLink = false) {
  const desk = new AgentDesk(
    fakeEngine({
      listApprovals: () => ({ approvals: [] }),
      listAgentActions: () => {
        if (fail) throw new Error('down');
        return { actions };
      },
      connection: () => {
        if (noLink) throw new ConnectError('not yet', Code.Unimplemented);
        return { mcpUrl, agentToken };
      },
    }),
  );
  render(AgentsScreen, { desk, every: 60_000 });
  return userEvent.setup();
}

describe('AgentsScreen', () => {
  it('lists the activity and filters it by agent and text', async () => {
    const user = setup();
    const log = screen.getByRole('region', { name: 'Activity' });
    expect(await within(log).findByRole('button', { name: 'run_tests' })).toBeTruthy();
    await user.selectOptions(within(log).getByRole('combobox', { name: 'Agent' }), 'Cursor');
    expect(within(log).queryByRole('button', { name: 'run_tests' })).toBeNull();
    await user.type(within(log).getByRole('searchbox', { name: 'Filter activity' }), 'nothing');
    expect(within(log).getByText('No activity matches these filters.')).toBeTruthy();
  });

  it('says when each tool call was made', async () => {
    setup();
    const row = (await screen.findByRole('button', { name: 'run_tests' })).closest('tr');
    const time = row?.querySelector('time');
    expect(time?.textContent).toBe('2 min ago');
    expect(time?.getAttribute('datetime')).toMatch(/^\d{4}-\d\d-\d\dT/);
  });

  it('expands a log row to show its arguments and result', async () => {
    const user = setup();
    const row = await screen.findByRole('button', { name: 'run_tests' });
    expect(row.getAttribute('aria-expanded')).toBe('false');
    await user.click(row);
    expect(row.getAttribute('aria-expanded')).toBe('true');
    expect(screen.getByText('Result returned to Claude Code')).toBeTruthy();
  });

  it('switches the connect snippet and copies it', async () => {
    const user = setup();
    await user.click(screen.getByRole('button', { name: 'Cursor' }));
    expect(screen.getByRole('button', { name: 'Cursor' }).getAttribute('aria-pressed')).toBe(
      'true',
    );
    await user.click(screen.getByRole('button', { name: 'Copy' }));
    expect(await navigator.clipboard.readText()).toContain('"mcpServers"');
    expect(screen.getByText('Copied.')).toBeTruthy();
  });

  it('shows the MCP URL and the agent token masked until asked, and copies them unmasked', async () => {
    const user = setup();
    await user.click(screen.getByRole('button', { name: 'HTTP' }));
    expect(await screen.findByText(mcpUrl)).toBeTruthy();
    expect(document.body.textContent).not.toContain(agentToken);
    expect(screen.getByText(/"Authorization": "Bearer ••••"/)).toBeTruthy();

    await user.click(screen.getByRole('button', { name: 'Copy the MCP config' }));
    const copied = JSON.parse(await navigator.clipboard.readText());
    expect(copied.mcpServers.vibranium).toEqual({
      type: 'http',
      url: mcpUrl,
      headers: { Authorization: `Bearer ${agentToken}` },
    });
    await user.click(screen.getByRole('button', { name: 'Copy the agent token' }));
    expect(await navigator.clipboard.readText()).toBe(agentToken);
    expect(document.body.textContent).not.toContain(agentToken);

    const show = screen.getByRole('button', { name: 'Show token' });
    await user.click(show);
    expect(show.getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByText(agentToken)).toBeTruthy();
    await user.click(show);
    expect(document.body.textContent).not.toContain(agentToken);
    expect(JSON.stringify({ ...localStorage, ...sessionStorage }) + location.href).not.toContain(
      agentToken,
    );
  });

  it('says when the agent token cannot be loaded', async () => {
    const user = setup(false, true);
    await user.click(screen.getByRole('button', { name: 'HTTP' }));
    expect((await screen.findByRole('alert')).textContent).toContain(
      'Could not load the agent token: not yet',
    );
  });

  it('says when the engine cannot be reached', async () => {
    setup(true);
    expect((await screen.findByRole('alert')).textContent).toContain('Could not reach the engine');
  });
});
