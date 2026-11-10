import { Code, ConnectError } from '@connectrpc/connect';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { fakeEngine } from '#lib/testing/engine.ts';
import { AgentDesk, linkRetryMs, type DeskClient } from './desk.svelte.ts';

afterEach(() => {
  vi.useRealTimers();
});

describe('AgentDesk', () => {
  it('tries the MCP endpoint again while loading it fails, then stops asking', async () => {
    vi.useFakeTimers();
    let calls = 0;
    const desk = new AgentDesk(
      fakeEngine({
        listApprovals: () => ({ approvals: [] }),
        listAgentActions: () => ({ actions: [] }),
        connection: () => {
          calls += 1;
          if (calls === 1) throw new ConnectError('starting', Code.Unavailable);
          return { mcpUrl: 'http://127.0.0.1:7420/mcp', agentToken: 'agent-token' };
        },
      }),
    );
    const stop = desk.poll(linkRetryMs * 10);
    await vi.waitFor(() => expect(desk.linkError).toBe('starting'));

    await vi.advanceTimersByTimeAsync(linkRetryMs);
    expect(desk.link).toEqual({ url: 'http://127.0.0.1:7420/mcp', token: 'agent-token' });
    expect(desk.linkError).toBeUndefined();

    await vi.advanceTimersByTimeAsync(linkRetryMs * 2);
    expect(calls).toBe(2);
    stop();
  });

  function failing(connection: DeskClient['connection']): { desk: AgentDesk; calls: () => number } {
    let calls = 0;
    const engine = fakeEngine({
      listApprovals: () => ({ approvals: [] }),
      listAgentActions: () => ({ actions: [] }),
    });
    const desk = new AgentDesk({
      listApprovals: engine.listApprovals,
      listAgentActions: engine.listAgentActions,
      decideApproval: engine.decideApproval,
      connection: (...args) => {
        calls += 1;
        return connection(...args);
      },
    });
    return { desk, calls: () => calls };
  }

  it.each([Code.Unimplemented, Code.NotFound])(
    'stops asking for the MCP endpoint when the answer will not change (code %i)',
    async (code) => {
      vi.useFakeTimers();
      const { desk, calls } = failing(() => Promise.reject(new ConnectError('no mcp', code)));
      const stop = desk.poll(linkRetryMs * 10);
      await vi.waitFor(() => expect(desk.linkError).toBe('no mcp'));
      await vi.advanceTimersByTimeAsync(linkRetryMs * 3);
      expect(calls()).toBe(1);
      stop();
    },
  );

  it('asks again after a network failure or a slow server', async () => {
    vi.useFakeTimers();
    const failures = [
      new TypeError('Failed to fetch'),
      new ConnectError('slow', Code.DeadlineExceeded),
    ];
    const { desk, calls } = failing(() => Promise.reject(failures.shift() ?? new Error('done')));
    const stop = desk.poll(linkRetryMs * 10);
    await vi.waitFor(() => expect(desk.linkError).toBe('Failed to fetch'));
    await vi.advanceTimersByTimeAsync(linkRetryMs);
    expect(desk.linkError).toBe('slow');
    await vi.advanceTimersByTimeAsync(linkRetryMs * 3);
    expect(calls()).toBe(3);
    stop();
  });
});
