import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { fakeEngine } from '#lib/testing/engine.ts';
import { Insights } from './insights.svelte.ts';
import InsightsScreen from './InsightsScreen.svelte';
import { enginePort, type InsightsPort, type RunSummary } from './port.ts';

const t0 = Date.UTC(2026, 10, 10, 9);
const runs: RunSummary[] = [
  { id: 3, startedMs: t0 + 2e6, durationMs: 4_000, passed: 9, failed: 0, blocked: 1, flaky: 0 },
  { id: 2, startedMs: t0 + 1e6, durationMs: 2_000, passed: 10, failed: 0, blocked: 0, flaky: 0 },
];

async function rendered(port?: Partial<InsightsPort>) {
  const asked: number[] = [];
  const insights = new Insights({
    runs: (limit) => {
      asked.push(limit);
      return Promise.resolve(runs);
    },
    trends: () =>
      Promise.resolve([
        { testId: 'users.crud#createsOne', results: 2, failed: 0, flaky: 1, meanMs: 12 },
        { testId: 'users.crud#getsOne', results: 2, failed: 0, flaky: 0, meanMs: 8 },
      ]),
    ...port,
  });
  await insights.load();
  render(InsightsScreen, { insights });
  return { asked, user: userEvent.setup() };
}

describe('InsightsScreen', () => {
  it('charts pass rate and duration per run, oldest first', async () => {
    await rendered();
    const rate = screen.getByRole('region', { name: 'Pass rate per run' });
    expect(
      within(rate)
        .getAllByRole('img')
        .map((b) => b.getAttribute('aria-label')),
    ).toEqual([
      expect.stringMatching(/^Run #2: 100% passed · /),
      expect.stringMatching(/^Run #3: 90% passed · 1 blocked · /),
    ]);
    expect(within(rate).getByText('run had blocked tests')).toBeTruthy();
    const time = screen.getByRole('region', { name: 'Run duration' });
    expect(within(time).getAllByRole('img')[1]?.getAttribute('aria-label')).toMatch(
      /^Run #3 took 4\.0 s/,
    );
  });

  it('sums the range up in tiles and a table, newest run first', async () => {
    await rendered();
    expect(screen.getByText('Last run pass rate').nextElementSibling?.textContent).toBe('90%');
    expect(screen.getByText('Median run duration').nextElementSibling?.textContent?.trim()).toBe(
      '3.0 s',
    );
    const table = screen.getByRole('table', { name: 'Last 10 runs in the range' });
    const heads = within(table)
      .getAllByRole('rowheader')
      .map((h) => h.textContent);
    expect(heads).toEqual(['#3', '#2']);
  });

  it('lists the flaky tests and narrows the time range', async () => {
    const { asked, user } = await rendered();
    const flaky = screen.getByRole('region', { name: 'Flakiest tests' });
    expect(
      within(flaky)
        .getAllByRole('listitem')
        .map((li) => li.textContent),
    ).toEqual([expect.stringContaining('users.crud#createsOne')]);
    const range = screen.getByRole('group', { name: 'Time range' });
    await user.click(within(range).getByRole('button', { name: 'Last 10 runs' }));
    expect(asked).toEqual([30, 10]);
    expect(within(range).getByRole('button', { name: 'Last 10 runs' }).ariaPressed).toBe('true');
  });

  it('says so when there is no history yet', async () => {
    await rendered({ runs: () => Promise.resolve([]), trends: () => Promise.resolve([]) });
    expect(screen.getByText('No flaky or failing tests in this range.')).toBeTruthy();
    expect(screen.getAllByText('No runs in this range yet.')).toHaveLength(2);
  });
});

describe('enginePort', () => {
  it('keeps only `run` runs and times the finished ones', async () => {
    const port = enginePort(
      fakeEngine({
        listRuns: () => ({
          runs: [
            { id: 5n, kind: 'run', startedMs: 1000n, finishedMs: 3500n, totals: { passed: 2 } },
            { id: 4n, kind: 'load', startedMs: 500n },
            { id: 3n, kind: 'run', startedMs: 100n },
          ],
        }),
      }),
    );
    expect(await port.runs(10)).toEqual([
      { id: 5, startedMs: 1000, durationMs: 2500, passed: 2, failed: 0, blocked: 0, flaky: 0 },
      { id: 3, startedMs: 100, passed: 0, failed: 0, blocked: 0, flaky: 0 },
    ]);
  });
});
