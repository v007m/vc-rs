import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { available, notYet } from '#lib/api/maybe.ts';
import type { Usage, UsagePort, UsageWindow } from './usage.ts';
import { UsageView } from './usage.svelte.ts';
import UsageScreen from './UsageScreen.svelte';

const start = Date.UTC(2026, 10, 21);

function week(days: UsageWindow): Usage {
  const daily = (n: number) => Array.from({ length: days }, (_, i) => (i % 2) * n);
  const calls = (n: number) => daily(n).reduce((a, b) => a + b, 0);
  return {
    fromMs: start,
    toMs: start + days * 86_400_000,
    agents: [
      {
        agent: 'claude-code',
        total: { calls: calls(400), runs: 31, tookMs: 125_000 },
        daily: daily(400),
      },
      { agent: 'cursor', total: { calls: calls(1), runs: 2, tookMs: 4200 }, daily: daily(1) },
    ],
    tools: [
      {
        agent: 'claude-code',
        tool: 'run',
        calls: 700,
        ok: 640,
        failed: 52,
        refused: 8,
        approved: 6,
      },
      { agent: 'cursor', tool: 'readFile', calls: 88, ok: 88, failed: 0, refused: 0, approved: 0 },
    ],
  };
}

function setup(answer: (days: UsageWindow) => ReturnType<UsagePort['usage']>) {
  const asked: number[] = [];
  const view = new UsageView({
    usage: (days) => {
      asked.push(days);
      return answer(days);
    },
  });
  render(UsageScreen, { view });
  return { asked, user: userEvent.setup() };
}

describe('UsageScreen', () => {
  it('shows each agent’s totals and the tools it used', async () => {
    setup(async (days) => available(week(days)));
    const agents = await screen.findByRole('table', { name: 'By agent' });
    const claude = within(agents).getByRole('row', { name: /claude-code/ });
    expect(claude.textContent).toContain('1,200');
    expect(claude.textContent).toContain('31');
    expect(claude.textContent).toContain('2 min 5 s');
    const tools = screen.getByRole('table', { name: 'Tools claude-code used' });
    const run = within(tools).getByRole('row', { name: /run/ });
    expect([...run.querySelectorAll('td')].map((td) => td.textContent)).toEqual([
      '700',
      '640',
      '52',
      '8',
      '6',
    ]);
    expect(screen.getByRole('table', { name: 'Tools cursor used' })).toBeTruthy();
    expect(document.body.textContent).not.toMatch(/token|cost/i);
  });

  it('asks for the window the person picks', async () => {
    const { asked, user } = setup(async (days) => available(week(days)));
    await screen.findByRole('table', { name: 'By agent' });
    const ninety = screen.getByRole('button', { name: '90 days' });
    await user.click(ninety);
    expect(asked).toEqual([7, 90]);
    expect(ninety.getAttribute('aria-pressed')).toBe('true');
    expect(screen.getByRole('button', { name: '7 days' }).getAttribute('aria-pressed')).toBe(
      'false',
    );
    const day = await screen.findByRole('rowheader', { name: dayAfter(89) });
    expect(day).toBeTruthy();
  });

  it('draws one bar a day and says the chart in words and in a table', async () => {
    setup(async (days) => available(week(days)));
    const chart = await screen.findByRole('img', { name: /Calls per day by agent/ });
    expect(chart.textContent).toContain('1,203 calls over 7 days, Nov 21 to Nov 27');
    const bars = chart.querySelectorAll('rect');
    expect(bars.length).toBe(6);
    expect(bars[0]?.textContent).toBe('Nov 22 · claude-code: 400 calls');
    const table = screen.getByRole('table', { name: 'Calls per day by agent' });
    expect(within(table).getAllByRole('row')).toHaveLength(8);
    expect(within(table).getByRole('row', { name: /Nov 22/ }).textContent).toContain('401');
  });

  it('floats an empty state when no agent called in the window', async () => {
    setup(async (days) => available({ ...week(days), agents: [], tools: [] }));
    expect(await screen.findByText('No agent calls in the last 7 days')).toBeTruthy();
    expect(screen.queryByRole('table')).toBeNull();
    expect(screen.getByRole('button', { name: '30 days' })).toBeTruthy();
  });

  it('says quietly when the engine cannot count agent calls', async () => {
    setup(async () => notYet('Update vc to see them.'));
    expect(await screen.findByText('Agent usage is not available')).toBeTruthy();
    expect(screen.getByText('Update vc to see them.')).toBeTruthy();
    expect(screen.queryByRole('alert')).toBeNull();
  });

  it('says when the engine cannot be reached', async () => {
    setup(async () => {
      throw new Error('down');
    });
    expect((await screen.findByRole('alert')).textContent).toContain(
      'Could not load agent usage: down',
    );
  });
});

function dayAfter(n: number): string {
  return new Date(start + n * 86_400_000).toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    timeZone: 'UTC',
  });
}
