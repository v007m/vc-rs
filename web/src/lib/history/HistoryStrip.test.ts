import { render, screen } from '@testing-library/svelte';
import { beforeAll, expect, it } from 'vitest';
import { available } from '#lib/api/maybe.ts';
import { stubResizeObserver } from '#lib/testing/layout.ts';
import HistoryStrip from './HistoryStrip.svelte';
import { historyNotYet, noRunsSeams, type HistoryPoint, type TestHistoryPort } from './seams.ts';

beforeAll(stubResizeObserver);

const POINTS: HistoryPoint[] = [
  { run: 9, status: 'passed', ms: 120, startedMs: 9 },
  { run: 8, status: 'flaky', ms: 300, startedMs: 8 },
  { run: 7, status: 'failed', ms: 200, startedMs: 7 },
  { run: 6, status: 'passed', ms: 100, startedMs: 6 },
];

it('says why the history is not available on an engine without it', async () => {
  render(HistoryStrip, { id: 'users.crud#getsUser', port: noRunsSeams.tests });
  expect(await screen.findByText(historyNotYet)).toBeTruthy();
});

it('sums up the test’s results and marks each run, newest on the right', async () => {
  const asked: [string, number][] = [];
  const port: TestHistoryPort = {
    history: (id, limit) => {
      asked.push([id, limit]);
      return Promise.resolve(available(POINTS));
    },
  };
  render(HistoryStrip, { id: 'users.crud#getsUser', port });
  expect(
    await screen.findByText('Last 4 results · 75% passed · 25% flaky · median 200 ms'),
  ).toBeTruthy();
  expect(asked).toEqual([['users.crud#getsUser', 30]]);
  const marks = screen.getAllByRole('listitem').map((mark) => mark.textContent?.trim());
  expect(marks).toEqual([
    'Run #6: passed, 100 ms',
    'Run #7: failed, 200 ms',
    'Run #8: flaky, 300 ms',
    'Run #9: passed, 120 ms',
  ]);
});

it('says when the history could not be read', async () => {
  const port: TestHistoryPort = { history: () => Promise.reject(new Error('locked')) };
  render(HistoryStrip, { id: 'users.crud#getsUser', port });
  expect(await screen.findByText('The history could not be read.')).toBeTruthy();
});
