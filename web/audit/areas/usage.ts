import { click } from '../act.ts';
import type { Fakes } from '../engine.ts';
import type { State } from '../states.ts';

/** The engine answers the usage screens need (batch E). */

const DAY = 86_400_000;
/** Today in the fixtures: 27 Nov 2026, early afternoon UTC. */
const toMs = Date.UTC(2026, 10, 27, 14, 5);
const today = Date.UTC(2026, 10, 27);

type Agent = Readonly<{ agent: string; peak: number; runShare: number; msPerCall: number }>;

const agents: readonly Agent[] = [
  { agent: 'claude-code', peak: 46, runShare: 0.22, msPerCall: 1_850 },
  { agent: 'cursor', peak: 21, runShare: 0.15, msPerCall: 1_400 },
  { agent: 'codex', peak: 7, runShare: 0.3, msPerCall: 2_100 },
];

/** A steady weekday rhythm with quiet weekends, the same for any window. */
function callsOn(day: number, a: Agent, rank: number): number {
  const weekday = new Date(day).getUTCDay();
  const weekend = weekday === 0 || weekday === 6;
  const wave = 0.6 + 0.4 * Math.abs(Math.sin(day / DAY + rank));
  return Math.round(a.peak * wave * (weekend ? 0.15 : 1));
}

function count(calls: number, a: Agent) {
  return {
    calls: BigInt(calls),
    runs: BigInt(Math.round(calls * a.runShare)),
    tookMs: BigInt(calls * a.msPerCall),
  };
}

function totals(days: number) {
  const first = today - (days - 1) * DAY;
  return agents.map((a, rank) => {
    const daily = Array.from({ length: days }, (_, i) => callsOn(first + i * DAY, a, rank));
    const calls = daily.reduce((sum, n) => sum + n, 0);
    return { agent: a.agent, total: count(calls, a), daily: daily.map((n) => count(n, a)) };
  });
}

/** Each agent's tools, the most called first, as shares of its calls. */
const toolMix: Readonly<Record<string, readonly [string, number][]>> = {
  'claude-code': [
    ['run', 0.34],
    ['check', 0.24],
    ['readFile', 0.18],
    ['explain', 0.12],
    ['writeFile', 0.08],
    ['listTests', 0.04],
  ],
  cursor: [
    ['check', 0.38],
    ['readFile', 0.3],
    ['run', 0.22],
    ['explain', 0.1],
  ],
  codex: [
    ['run', 0.55],
    ['readFile', 0.45],
  ],
};

function tools(agentTotals: ReturnType<typeof totals>) {
  return agentTotals.flatMap(({ agent, total }) =>
    (toolMix[agent] ?? []).map(([tool, share]) => {
      const calls = Math.round(Number(total.calls) * share);
      const refused = tool === 'writeFile' || tool === 'run' ? Math.round(calls * 0.04) : 0;
      const failed = Math.round(calls * 0.06);
      const approved = tool === 'writeFile' ? Math.round(calls * 0.5) : 0;
      return {
        agent,
        tool,
        calls: BigInt(calls),
        ok: BigInt(calls - failed - refused),
        failed: BigInt(failed),
        refused: BigInt(refused),
        approved: BigInt(approved),
        tookMs: BigInt(calls * 1_600),
      };
    }),
  );
}

export const fakes: Fakes = {
  agentUsage: ({ days }) => {
    const window = days || 7;
    const agentTotals = totals(window);
    return {
      fromMs: BigInt(today - (window - 1) * DAY),
      toMs: BigInt(toMs),
      agents: agentTotals,
      tools: tools(agentTotals),
    };
  },
};

/** The usage screens and overlays the audit checks. */
export const states: State[] = [
  { name: 'agents-usage', path: '/agents/usage' },
  {
    name: 'agents-usage-90',
    path: '/agents/usage',
    act: async (page) => {
      await click(page, '90 days');
      await page.getByRole('button', { name: '90 days', pressed: true }).waitFor();
    },
  },
  {
    name: 'agents-usage-empty',
    path: '/agents/usage',
    fakes: {
      agentUsage: ({ days }) => ({
        fromMs: BigInt(today - ((days || 7) - 1) * DAY),
        toMs: BigInt(toMs),
        agents: [],
        tools: [],
      }),
    },
  },
];
