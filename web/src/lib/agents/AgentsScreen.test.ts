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
  },
  {
    agent: 'Cursor',
    tool: 'get_test',
    arguments: '{"id":"users.crud#createsUser"}',
    result: 'ok',
    approvedBy: 'you',
  },
];

function setup(fail = false) {
  const desk = new AgentDesk(
    fakeEngine({
      listApprovals: () => ({ approvals: [] }),
      listAgentActions: () => {
        if (fail) throw new Error('down');
        return { actions };
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

  it('says when the engine cannot be reached', async () => {
    setup(true);
    expect((await screen.findByRole('alert')).textContent).toContain('Could not reach the engine');
  });
});
