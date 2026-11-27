import { describe, expect, it } from 'vitest';
import { fakeEngine } from '#lib/testing/engine.ts';
import { anyCalls, toolsByAgent, usageNotYet, usagePort, type Usage } from './usage.ts';
import { chartLayout, chartSummary, dailyTotals, dayLabels, margin, toneOf } from './usageChart.ts';

const nov25 = Date.UTC(2026, 10, 25);

const usage: Usage = {
  fromMs: nov25,
  toMs: nov25 + 3 * 86_400_000 - 1,
  agents: [
    { agent: 'claude-code', total: { calls: 9, runs: 2, tookMs: 4000 }, daily: [2, 0, 7] },
    { agent: 'cursor', total: { calls: 3, runs: 0, tookMs: 900 }, daily: [1, 2, 0] },
  ],
  tools: [
    { agent: 'claude-code', tool: 'run', calls: 6, ok: 5, failed: 1, refused: 0, approved: 1 },
    { agent: 'cursor', tool: 'explain', calls: 3, ok: 3, failed: 0, refused: 0, approved: 0 },
    { agent: 'claude-code', tool: 'check', calls: 3, ok: 3, failed: 0, refused: 0, approved: 0 },
  ],
};

describe('usagePort', () => {
  it('asks for the window and turns the counts into numbers', async () => {
    let asked = 0;
    const port = usagePort(
      fakeEngine({
        agentUsage: ({ days }) => {
          asked = days;
          return {
            fromMs: BigInt(nov25),
            toMs: BigInt(nov25 + 1),
            agents: [
              {
                agent: 'cursor',
                total: { calls: 4n, runs: 1n, tookMs: 20n },
                daily: [{ calls: 4n }],
              },
            ],
            tools: [{ agent: 'cursor', tool: 'run', calls: 4n, ok: 3n, failed: 1n }],
          };
        },
      }),
    );
    const got = await port.usage(30);
    expect(asked).toBe(30);
    expect(got).toEqual({
      available: true,
      value: {
        fromMs: nov25,
        toMs: nov25 + 1,
        agents: [{ agent: 'cursor', total: { calls: 4, runs: 1, tookMs: 20 }, daily: [4] }],
        tools: [
          { agent: 'cursor', tool: 'run', calls: 4, ok: 3, failed: 1, refused: 0, approved: 0 },
        ],
      },
    });
  });

  it('says why when the engine has no AgentUsage', async () => {
    expect(await usagePort(fakeEngine({})).usage(7)).toEqual({
      available: false,
      reason: usageNotYet,
    });
  });
});

describe('usage helpers', () => {
  it('groups the tools under each agent', () => {
    expect(toolsByAgent(usage).map(([agent, tools]) => [agent, tools.map((t) => t.tool)])).toEqual([
      ['claude-code', ['run', 'check']],
      ['cursor', ['explain']],
    ]);
  });

  it('knows a window with no calls', () => {
    expect(anyCalls(usage)).toBe(true);
    expect(anyCalls({ ...usage, agents: [] })).toBe(false);
  });

  it('labels the UTC days and totals each one', () => {
    expect(dayLabels(usage)).toEqual(['Nov 25', 'Nov 26', 'Nov 27']);
    expect(dailyTotals(usage)).toEqual([3, 2, 7]);
  });

  it('says the chart in words', () => {
    expect(chartSummary(usage)).toBe(
      '12 calls over 3 days, Nov 25 to Nov 27: claude-code 9, cursor 3. Busiest day Nov 27 with 7 calls.',
    );
  });

  it('stacks each day the busiest agent first, on a round axis', () => {
    const at = chartLayout(usage, 400);
    expect(at.grid.map((g) => g.value)).toEqual([0, 5, 10]);
    const [first, second, third] = at.bars;
    expect(first?.segments.map((s) => [s.agent, s.calls, s.tone])).toEqual([
      ['claude-code', 2, 0],
      ['cursor', 1, 1],
    ]);
    expect(second?.segments.map((s) => s.agent)).toEqual(['cursor']);
    expect(third?.total).toBe(7);
    const [foot, above] = first?.segments ?? [];
    expect((above?.y ?? 0) + (above?.h ?? 0)).toBeLessThan(foot?.y ?? 0);
    expect(first?.x).toBeGreaterThan(margin.l);
  });

  it('labels fewer days when they do not fit, always today', () => {
    const days = Array.from({ length: 90 }, (_, i) => i % 3);
    const long: Usage = {
      ...usage,
      agents: [{ agent: 'a', total: { calls: 90, runs: 0, tookMs: 0 }, daily: days }],
    };
    const { labels } = chartLayout(long, 640);
    expect(labels.length).toBeLessThanOrEqual(10);
    expect(labels.at(-1)?.text).toBe(dayLabels(long).at(-1));
  });

  it('reuses the tones after five agents', () => {
    expect([0, 4, 5].map(toneOf)).toEqual([0, 4, 0]);
  });
});
