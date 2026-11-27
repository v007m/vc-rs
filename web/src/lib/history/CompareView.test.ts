import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { expect, it, vi } from 'vitest';
import { available } from '#lib/api/maybe.ts';
import CompareView from './CompareView.svelte';
import type { RunLine } from './port.ts';
import { compareNotYet, noRunsSeams, type ComparePort, type RunComparison } from './seams.ts';

const line = (id: number, selector: string): RunLine => ({
  id,
  kind: 'run',
  startedMs: id,
  profiles: [],
  selector,
  outcome: 'passed',
  counts: { passed: 1, failed: 0, blocked: 0, flaky: 0, skipped: 0 },
});

const RESULT: RunComparison = {
  newlyFailing: [
    { id: 'users.crud#createsUser', was: 'passed', now: 'failed', why: 'status was 500' },
  ],
  fixed: [],
  timing: [{ id: 'users.crud#listsUsers', baseMs: 100, headMs: 300 }],
  shapes: [],
};

function setup(port: ComparePort) {
  const onpick = vi.fn<(base: number, head: number) => void>();
  const runs = [line(7, 'tag:smoke'), line(6, '')];
  render(CompareView, { base: 6, head: 7, runs, port, onpick });
  return { onpick, user: userEvent.setup() };
}

it('says why compare is not available on an engine without it', async () => {
  setup(noRunsSeams.compare);
  expect(await screen.findByText('Compare is not available yet')).toBeTruthy();
  expect(screen.getByText(compareNotYet)).toBeTruthy();
});

it('counts each kind of change and lists the newly failing tests', async () => {
  const asked: [number, number][] = [];
  setup({
    compare: (base, head) => {
      asked.push([base, head]);
      return Promise.resolve(available(RESULT));
    },
  });
  const tabs = within(await screen.findByRole('tablist', { name: 'Changes from #6 to #7' }));
  expect(tabs.getAllByRole('tab').map((tab) => tab.textContent)).toEqual([
    'Newly failing 1',
    'Fixed 0',
    'Slower 1',
    'Faster 0',
    'Response shapes 0',
  ]);
  expect(asked).toEqual([[6, 7]]);
  const row = within(screen.getByRole('tabpanel')).getByRole('row', { name: /createsUser/ });
  expect(row.textContent).toContain('status was 500');
  expect(screen.getByText('1 newly failing · 0 fixed · 1 slower')).toBeTruthy();
});

it('swaps the base and the head', async () => {
  const { onpick, user } = setup(noRunsSeams.compare);
  await user.click(screen.getByRole('button', { name: 'Swap base and head' }));
  expect(onpick).toHaveBeenCalledWith(7, 6);
});
