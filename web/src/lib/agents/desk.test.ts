import { Code, ConnectError } from '@connectrpc/connect';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { fakeEngine } from '#lib/testing/engine.ts';
import { AgentDesk, linkRetryMs } from './desk.svelte.ts';

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
});
