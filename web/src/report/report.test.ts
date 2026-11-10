import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import type { IndexRow, Summary } from './data.ts';
import ReportView from './Report.svelte';
import { Report } from './report.svelte.ts';

const summary: Summary = {
  run: { id: 4, status: 'failed', startedMs: 0, durationMs: 900, profiles: ['dev'], seed: 7 },
  totals: { passed: 1, failed: 1 },
  lifecycle: [{ label: 'cleanup deleteUser', status: 'failed', message: '<b>500</b>' }],
  files: ['users.crud'],
  shardSize: 500,
};
const index: IndexRow[] = [
  ['Creates one', null, 0, 'passed', 12, []],
  ['Rejects <img src=x onerror=alert(1)>', null, 0, 'failed', 3, []],
];
const tests = {
  '1': {
    location: null,
    blockedBy: null,
    skipReason: null,
    message: 'expected 400, got 200',
    checks: [{ label: 'status', passed: false, expected: '400', actual: '200', location: null }],
    exchanges: [],
  },
};

beforeAll(() => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe(): void {}
      unobserve(): void {}
      disconnect(): void {}
    },
  );
});

async function rendered() {
  const data: Record<string, unknown> = { summary, index, 'tests/0': tests };
  const report = new Report({ get: (key) => Promise.resolve(data[key]) }, () => {});
  await report.start();
  render(ReportView, { report });
  return { report, user: userEvent.setup() };
}

describe('Report', () => {
  it('shows the run, its totals and its lifecycle as text', async () => {
    await rendered();
    expect(screen.getByRole('heading', { name: 'Run #4' })).toBeTruthy();
    expect(screen.getByRole('img').getAttribute('aria-label')).toMatch(/^1 passed, 1 failed/);
    expect(screen.getByText(/cleanup deleteUser: <b>500<\/b>/)).toBeTruthy();
    expect(screen.getByText('2 of 2 tests')).toBeTruthy();
  });

  it('filters by outcome and opens a test, rendering its label as text', async () => {
    const { user } = await rendered();
    const status = screen.getByRole('group', { name: 'Status' });
    await user.click(within(status).getByRole('button', { name: 'failed' }));
    expect(within(status).getByRole('button', { name: 'failed' }).getAttribute('aria-pressed')).toBe(
      'true',
    );
    expect(screen.getByText('1 of 2 tests')).toBeTruthy();
    await user.click(screen.getByRole('option'));
    const detail = await screen.findByRole('region', { name: /^Rejects <img/ });
    expect(detail.querySelector('img')).toBeNull();
    expect(within(detail).getByRole('list', { name: 'Checks' }).textContent).toContain('400');
  });
});
