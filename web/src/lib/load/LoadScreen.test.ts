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
    breaching: [],
    host: { cpu: 40, memMb: 512, conns: 80, late: 0 },
  },
};

const LATENCY = { p50: 80, p90: 180, p95: 210, p99: 340, max: 900 };

const REPORT: LoadReport = {
  seconds: 120,
  iterations: 144000,
  errors: 720,
  rps: 1200,
  latency: LATENCY,
  breaches: [{ metric: 'p99', shown: '612 ms (lt 500 ms)' }],
  tests: [
    {
      test: 'checkout.flow#buysOneItem',
      iterations: 144000,
      errors: 720,
      rps: 1200,
      latency: LATENCY,
    },
  ],
  errorKinds: [{ api: 'checkout#pay', status: 502, code: 'V5101', count: 720 }],
  seed: '42',
  notices: [],
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
    scenarios: () => Promise.resolve(available(['checkout.peak', 'search.soak'])),
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

it('offers the report of a finished run, with the thresholds it breached', async () => {
  const finished: LoadUpdate = { finished: 'breached', id: 12, report: REPORT };
  const session = new LoadSession(portOf([TICK, finished], false));
  await session.start('checkout.load');
  render(LoadScreen, { session });
  expect(screen.getByText('Threshold breached')).toBeTruthy();
  await userEvent.click(screen.getByRole('button', { name: 'Report' }));
  const breaches = within(screen.getByRole('region', { name: 'Thresholds' }));
  expect(breaches.getByText('612 ms (lt 500 ms)')).toBeTruthy();
  const kinds = within(screen.getByRole('region', { name: 'Failed iterations' }));
  expect(kinds.getByRole('row', { name: /checkout#pay/ }).textContent).toContain('502');
  const mix = within(screen.getByRole('region', { name: 'Mix tests' }));
  expect(mix.getByRole('row', { name: /buysOneItem/ }).textContent).toContain('0.5 %');
  expect(screen.queryAllByRole('figure')).toEqual([]);
  expect(screen.getByText('staging · #12')).toBeTruthy();
});

it('lists the load scenarios and starts the one picked', async () => {
  const port = portOf([], false);
  const session = new LoadSession(port);
  await session.listScenarios();
  render(LoadScreen, { session });
  const picker = screen.getByLabelText<HTMLSelectElement>('Load scenario');
  expect(picker.value).toBe('checkout.peak');
  await userEvent.selectOptions(picker, 'search.soak');
  await userEvent.click(screen.getByRole('button', { name: 'Start' }));
  expect(port.stops).toHaveLength(1);
});

it('says a stored load run keeps no ticks', async () => {
  const port = {
    ...portOf([], false),
    stored: () => Promise.resolve(available({ ...RUN, phase: 'held' as const })),
  };
  const session = new LoadSession(port);
  await session.open(12);
  render(LoadScreen, { session });
  expect(screen.getByText(/keeps a load run's verdict, not its ticks/)).toBeTruthy();
  expect(screen.queryAllByRole('figure')).toEqual([]);
});
