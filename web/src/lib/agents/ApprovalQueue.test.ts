import { create } from '@bufbuild/protobuf';
import { Code, ConnectError } from '@connectrpc/connect';
import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import {
  ApprovalSchema,
  ApprovalStatus,
  Verdict,
  type Approval,
} from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import ApprovalQueue from './ApprovalQueue.svelte';
import { AgentDesk, ago } from './desk.svelte.ts';

const now = 1_800_000_000_000;

function ask(id: bigint): Approval {
  return create(ApprovalSchema, {
    id,
    agent: 'Claude Code',
    tool: 'call_api',
    profile: 'staging',
    methods: ['POST'],
    summary: 'POST /users on staging',
    arguments: '{"api":"users.create"}',
    createdMs: BigInt(now - 120_000),
    status: ApprovalStatus.PENDING,
  });
}

async function setup() {
  let pending = [ask(1n)];
  const verdicts: Verdict[] = [];
  const desk = new AgentDesk(
    fakeEngine({
      listApprovals: () => ({ approvals: pending }),
      listAgentActions: () => ({ actions: [] }),
      decideApproval: ({ verdict }) => {
        verdicts.push(verdict);
        pending = [];
        return { status: ApprovalStatus.APPROVED };
      },
    }),
  );
  await desk.refresh();
  render(ApprovalQueue, { desk, now });
  return { verdicts, user: userEvent.setup() };
}

describe('AgentDesk', () => {
  it('keeps a failed verdict shown after the refresh that follows it', async () => {
    const desk = new AgentDesk(
      fakeEngine({
        listApprovals: () => ({ approvals: [ask(1n)] }),
        listAgentActions: () => ({ actions: [] }),
        decideApproval: () => {
          throw new ConnectError('approval 1 was already decided', Code.FailedPrecondition);
        },
      }),
    );
    await desk.decide(1n, Verdict.APPROVE);
    expect(desk.error).toBe('approval 1 was already decided');
    expect(desk.deciding).toBeUndefined();
    expect(desk.pending.map((a) => a.id)).toEqual([1n]);
  });
});

describe('ApprovalQueue', () => {
  it('shows what the agent wants and how long it has waited', async () => {
    await setup();
    expect(
      screen.getByRole('heading', { name: 'Claude Code wants to call_api on staging' }),
    ).toBeTruthy();
    expect(screen.getByText('Asked 2 min ago')).toBeTruthy();
  });

  it('approves once and shows the empty state', async () => {
    const { verdicts, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Approve once' }));
    expect(verdicts).toEqual([Verdict.APPROVE]);
    expect(await screen.findByText('Nothing waiting.')).toBeTruthy();
  });

  it('asks before always allowing, and can cancel', async () => {
    const { verdicts, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Always allow on staging' }));
    expect(screen.getByRole('group', { name: 'Confirm always allow' })).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(verdicts).toEqual([]);
    await user.click(screen.getByRole('button', { name: 'Always allow on staging' }));
    await user.click(screen.getByRole('button', { name: 'Save rule and approve' }));
    expect(verdicts).toEqual([Verdict.ALWAYS_ALLOW]);
  });

  it('denies', async () => {
    const { verdicts, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Deny' }));
    expect(verdicts).toEqual([Verdict.DENY]);
  });
});

describe('ago', () => {
  it('reads seconds, minutes and hours', () => {
    expect(ago(BigInt(now - 5_000), now)).toBe('5 s ago');
    expect(ago(BigInt(now - 3 * 3_600_000), now)).toBe('3 h ago');
  });
});
