import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { beforeAll, expect, it } from 'vitest';
import { available } from '#lib/api/maybe.ts';
import { stubResizeObserver } from '#lib/testing/layout.ts';
import LoadScreen from './LoadScreen.svelte';
import {
  loadNotYet,
  noLoadPort,
  type LoadPort,
  type LoadReport,
  type LoadRun,
  type LoadThreshold,
  type LoadUpdate,
} from './port.ts';
import { LoadSession } from './session.svelte.ts';

beforeAll(stubResizeObserver);

const P99: LoadThreshold = { metric: 'p99', limit: 500, label: 'p99 < 500ms', abortOn: false };

const RUN: LoadRun = {
  file: 'checkout.load',
  profile: 'staging',
  startedMs: 0,
  plannedS: 120,
  phase: 'running',
  thresholds: [P99],
  ticks: [],
};

const TICK: LoadUpdate = {
  tick: {
    t: 5,
    users: 50,
    rps: 1200,
    p50: 80,
    p95: 210,
    p99: 340,
    errorRate: 0.5,
    host: { cpu: 40, memMb: 512, conns: 80, dropped: 0 },
  },
};

const REPORT: LoadReport = {
  verdicts: [{ threshold: P99, scope: 'whole run', held: true, worst: 340, at: 5, whole: 300 }],
  histogram: [[100, 40]],
  scenarios: [],
  previous: [{ run: 12, p99: 340 }],
};

/** A load port whose one run sends `updates`, then, when `hold`, stays open until stopped. */
function portOf(updates: LoadUpdate[], hold: boolean): LoadPort & { stops: AbortSignal[] } {
  const stops: AbortSignal[] = [];
  async function* stream(signal: AbortSignal): AsyncGenerator<LoadUpdate> {
    yield* updates;
    if (hold) await new Promise((resolve) => signal.addEventListener('abort', resolve));
  }
  return {
    stops,
    start: (_file, signal) => {
      stops.push(signal);
      return Promise.resolve(available({ run: RUN, updates: stream(signal) }));
    },
    stored: () => Promise.resolve(available(RUN)),
  };
}

it('shows the empty state with why there is no load run', async () => {
  const session = new LoadSession(noLoadPort);
  await session.start('checkout.load');
  render(LoadScreen, { session });
  expect(screen.getByText('No load run is open')).toBeTruthy();
  expect(screen.getByText(loadNotYet)).toBeTruthy();
});

it('shows a running load run’s KPIs, charts and hosts, and stops it', async () => {
  const port = portOf([TICK], true);
  const session = new LoadSession(port);
  const done = session.start('checkout.load');
  render(LoadScreen, { session });
  const kpis = await screen.findByText('Requests a second');
  expect(kpis.parentElement?.textContent).toContain('1,200');
  expect(screen.getAllByRole('figure').map((f) => f.querySelector('h3')?.textContent)).toEqual([
    'Latency',
    'Throughput',
    'Error rate',
  ]);
  const hosts = within(screen.getByRole('region', { name: 'Hosts' }));
  expect(hosts.getByText('40 %')).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
  await done;
  expect(port.stops[0]?.aborted).toBe(true);
  expect(screen.getByText('Stopped')).toBeTruthy();
  expect(screen.getByRole('button', { name: 'Run again' })).toBeTruthy();
});

it('offers the report of a finished run, with each threshold’s verdict', async () => {
  const finished: LoadUpdate = { finished: 'held', id: 12, report: REPORT };
  const session = new LoadSession(portOf([TICK, finished], false));
  await session.start('checkout.load');
  render(LoadScreen, { session });
  expect(screen.getByText('Thresholds held')).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Report' }));
  const verdicts = within(screen.getByRole('region', { name: 'Thresholds' }));
  const row = verdicts.getByRole('row', { name: /p99 < 500ms/ });
  expect(row.textContent).toContain('✓ Held');
  expect(screen.queryAllByRole('figure')).toEqual([]);
  expect(screen.getByText('staging · #12')).toBeTruthy();
});
