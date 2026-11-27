import { render, screen } from '@testing-library/svelte';
import { expect, it, vi } from 'vitest';
import { RunHistory } from '#lib/history/history.svelte.ts';
import type { HistoryPort, RunScope } from '#lib/history/port.ts';
import RunsPage from '../../routes/runs/+page.svelte';

const held = vi.hoisted(() => ({ space: {} as unknown }));

vi.mock('$app/navigation', () => ({ goto: vi.fn() }));
vi.mock('$app/state', () => ({
  page: { url: new URL('http://localhost/runs?schedule=nightly') },
}));
vi.mock('#lib/history/space.svelte.ts', () => ({ getRunsSpace: () => held.space }));

it('lists the runs of the schedule the address names', async () => {
  const asked: (RunScope | undefined)[] = [];
  const port: HistoryPort = {
    runs: (_limit, scope) => {
      asked.push(scope);
      return Promise.resolve([]);
    },
    run: () => Promise.reject(new Error('unused')),
  };
  const history = new RunHistory(port);
  held.space = { history, run: vi.fn() };
  render(RunsPage);
  expect(await screen.findByRole('button', { name: 'Remove the schedule filter' })).toBeTruthy();
  expect(history.scope).toEqual({ schedule: 'nightly' });
  expect(asked).toEqual([{ schedule: 'nightly' }]);
});
