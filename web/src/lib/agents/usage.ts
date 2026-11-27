import type { Client } from '@connectrpc/connect';
import { unlessMissing, type Maybe } from '#lib/api/maybe.ts';
import type {
  AgentTotals,
  EngineService,
  ToolUsage,
  UsageCount,
} from '#lib/gen/vibranium/engine/v1/engine_pb.ts';

/** The windows the screen offers, in days, today included (UTC). */
export const WINDOWS = [7, 30, 90] as const;

export type UsageWindow = (typeof WINDOWS)[number];

/** What agents did in a stretch of time: calls made, runs they started, time it took (D709). */
export type UsageTotals = Readonly<{ calls: number; runs: number; tookMs: number }>;

/** One agent's totals, and its calls on each day of the window, the oldest first. */
export type AgentUse = Readonly<{ agent: string; total: UsageTotals; daily: readonly number[] }>;

/** How one agent used one tool: its calls, and how each ended. */
export type ToolUse = Readonly<{
  agent: string;
  tool: string;
  calls: number;
  ok: number;
  failed: number;
  refused: number;
  approved: number;
}>;

/** The window's usage: when it starts (the first day, UTC) and ends, by agent and by tool. */
export type Usage = Readonly<{
  fromMs: number;
  toMs: number;
  agents: readonly AgentUse[];
  tools: readonly ToolUse[];
}>;

/** What the usage screen asks of the engine. */
export interface UsagePort {
  usage(days: UsageWindow): Promise<Maybe<Usage>>;
}

/** Why an older engine shows no usage. */
export const usageNotYet = 'This vc does not count agent calls yet. Update vc to see them.';

const zero: UsageTotals = { calls: 0, runs: 0, tookMs: 0 };

function totals(count: UsageCount | undefined): UsageTotals {
  if (!count) return zero;
  return { calls: Number(count.calls), runs: Number(count.runs), tookMs: Number(count.tookMs) };
}

function agentUse({ agent, total, daily }: AgentTotals): AgentUse {
  return { agent, total: totals(total), daily: daily.map((day) => Number(day.calls)) };
}

function toolUse(tool: ToolUsage): ToolUse {
  const { agent, calls, ok, failed, refused, approved } = tool;
  return {
    agent,
    tool: tool.tool,
    calls: Number(calls),
    ok: Number(ok),
    failed: Number(failed),
    refused: Number(refused),
    approved: Number(approved),
  };
}

/** The port over the engine client's AgentUsage. */
export function usagePort(client: Pick<Client<typeof EngineService>, 'agentUsage'>): UsagePort {
  return {
    usage: (days) =>
      unlessMissing(usageNotYet, async () => {
        const got = await client.agentUsage({ days });
        return {
          fromMs: Number(got.fromMs),
          toMs: Number(got.toMs),
          agents: got.agents.map(agentUse),
          tools: got.tools.map(toolUse),
        };
      }),
  };
}

/** Whether any agent made a call in the window. */
export function anyCalls(usage: Usage): boolean {
  return usage.agents.some((a) => a.total.calls > 0);
}

/** The tools each agent used, in the engine's order, under the agents' order. */
export function toolsByAgent(usage: Usage): readonly (readonly [string, readonly ToolUse[]])[] {
  const names = [
    ...new Set([...usage.agents.map((a) => a.agent), ...usage.tools.map((t) => t.agent)]),
  ];
  return names
    .map((agent) => [agent, usage.tools.filter((t) => t.agent === agent)] as const)
    .filter(([, tools]) => tools.length > 0);
}

/** A whole count with thousands separators: `1,204`. */
export function count(n: number): string {
  return n.toLocaleString('en-US');
}
