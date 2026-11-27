import { render, screen } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { afterEach, expect, it, vi } from 'vitest';
import { goto } from '$app/navigation';
import AllRuns from './AllRuns.svelte';
import { ALL, RunHistory } from './history.svelte.ts';
import type { HistoryPort, RunLine, RunScope } from './port.ts';

vi.mock('$app/navigation', () => ({ goto: vi.fn() }));

afterEach(() => vi.mocked(goto).mockClear());

function line(id: number, extra: Partial<RunLine> = {}): RunLine {
  return {
    id,
    kind: 'run',
    startedMs: 1_790_000_000_000 - id,
    profiles: [],
    selector: '',
    outcome: 'passed',
    counts: { passed: 1, failed: 0, blocked: 0, flaky: 0, skipped: 0 },
    ...extra,
  };
}

const RUNS = [
  line(7, { selector: 'tag:smoke', profiles: ['staging'], outcome: 'failed' }),
  line(6, { kind: 'load', selector: 'checkout.load' }),
  line(5, { selector: 'users.crud' }),
];

async function rendered(scope: RunScope = {}) {
  const asked: [number, RunScope | undefined][] = [];
  const port: HistoryPort = {
    runs: (limit, scope) => {
      asked.push([limit, scope]);
      return Promise.resolve(RUNS);
    },
    run: () => Promise.reject(new Error('unused')),
  };
  const history = new RunHistory(port);
  history.scope = scope;
  await history.load();
  render(AllRuns, { history, onrun: vi.fn() });
  return { asked, user: userEvent.setup() };
}

const hrefs = () => screen.getAllByRole('link').map((a) => a.getAttribute('href'));

it('lists each run as a link to its page, a load run to the load screen', async () => {
  await rendered();
  expect(hrefs()).toEqual(['/runs/7', '/runs/load/6', '/runs/5']);
});

it('keeps the runs with every word typed in the search box', async () => {
  const { user } = await rendered();
  await user.type(screen.getByRole('searchbox', { name: /^Search runs/ }), 'smoke staging');
  expect(hrefs()).toEqual(['/runs/7']);
  expect(screen.getByText('1 runs')).toBeTruthy();
});

it('compares two picked runs, the older as the base', async () => {
  const { user } = await rendered();
  const compare = screen.getByRole<HTMLButtonElement>('button', { name: 'Compare' });
  expect(compare.disabled).toBe(true);
  await user.click(screen.getByRole('checkbox', { name: 'Pick run #7 to compare' }));
  await user.click(screen.getByRole('checkbox', { name: 'Pick run #5 to compare' }));
  expect(screen.getByText('2 of 2 picked')).toBeTruthy();
  expect(compare.disabled).toBe(false);
  await user.click(compare);
  expect(goto).toHaveBeenCalledWith('/runs/compare/5/7');
});

it('asks the history for the runs of the kind chosen', async () => {
  const { asked, user } = await rendered();
  const load = screen.getByRole('button', { name: 'Load' });
  await user.click(load);
  expect(asked.at(-1)).toEqual([ALL, { kind: 'load' }]);
  expect(load.getAttribute('aria-pressed')).toBe('true');
  await user.click(screen.getByRole('button', { name: 'All' }));
  expect(asked.at(-1)).toEqual([ALL, { kind: undefined }]);
});

it('shows the schedule the runs are listed for, and lists every run once it is removed', async () => {
  const { asked, user } = await rendered({ schedule: 'nightly' });
  expect(asked.at(-1)).toEqual([ALL, { schedule: 'nightly' }]);
  expect(screen.getByText('nightly')).toBeTruthy();
  await user.click(screen.getByRole('button', { name: 'Remove the schedule filter' }));
  expect(goto).toHaveBeenCalledWith('/runs');
  expect(asked.at(-1)).toEqual([ALL, { schedule: undefined }]);
  expect(screen.queryByRole('button', { name: 'Remove the schedule filter' })).toBeNull();
});

it('shows no schedule chip for runs from every schedule', async () => {
  await rendered();
  expect(screen.queryByRole('button', { name: 'Remove the schedule filter' })).toBeNull();
});
