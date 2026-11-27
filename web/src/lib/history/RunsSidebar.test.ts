import { render, screen, within } from '@testing-library/svelte';
import { expect, it, vi } from 'vitest';
import { RunSession } from '#lib/run/session.svelte.ts';
import { events, fakeEngine } from '#lib/testing/engine.ts';
import { RunHistory } from './history.svelte.ts';
import type { RunLine } from './port.ts';
import RunsSidebar from './RunsSidebar.svelte';

function line(id: number, extra: Partial<RunLine> = {}): RunLine {
  return {
    id,
    kind: 'run',
    startedMs: Date.now() - 5 * 60_000,
    profiles: [],
    selector: 'tag:smoke',
    outcome: 'passed',
    counts: { passed: 1, failed: 0, blocked: 0, flaky: 0, skipped: 0 },
    ...extra,
  };
}

async function historyOf(runs: RunLine[]): Promise<RunHistory> {
  const history = new RunHistory({
    runs: () => Promise.resolve(runs),
    run: () => Promise.reject(new Error('unused')),
  });
  await history.load();
  return history;
}

/** A session whose one run has ended, so the sidebar keeps it as the last live run. */
async function endedSession(): Promise<RunSession> {
  const stream = events({ case: 'runStarted', value: { files: 1, instances: 1 } });
  const session = new RunSession(
    fakeEngine({
      async *run() {
        for (const event of stream) yield { event };
      },
    }),
  );
  await session.start({});
  return session;
}

/** A session with a run going until `end` is called. */
function runningSession(): { session: RunSession; end: () => void } {
  let end = (): void => undefined;
  const held = new Promise<void>((resolve) => (end = resolve));
  const stream = events({ case: 'runStarted', value: { files: 1, instances: 3 } });
  const session = new RunSession(
    fakeEngine({
      async *run() {
        for (const event of stream) yield { event };
        await held;
      },
    }),
  );
  void session.start({});
  return { session, end };
}

const recent = () => within(screen.getByRole('list', { name: 'Recent runs' }));

it('lists the live run first, then the recent runs, each a link', async () => {
  const history = await historyOf([line(7, { outcome: 'failed' }), line(6, { kind: 'load' })]);
  render(RunsSidebar, { history, session: await endedSession(), pathname: '/runs/7' });
  const links = recent().getAllByRole('link');
  expect(links.map((a) => a.getAttribute('href'))).toEqual([
    '/runs/live',
    '/runs/7',
    '/runs/load/6',
  ]);
  expect(links[0]?.textContent).toContain('Last live run');
  expect(links[1]?.textContent).toContain('tag:smoke');
  expect(links[1]?.textContent).toContain('5 min ago');
  expect(links[1]?.getAttribute('aria-current')).toBe('page');
  expect(links[2]?.textContent).toContain('load');
});

it('says there are no runs yet when the history is empty and nothing ran', async () => {
  const history = await historyOf([]);
  render(RunsSidebar, { history, session: new RunSession(fakeEngine({})), pathname: '/runs' });
  expect(recent().queryAllByRole('link')).toEqual([]);
  expect(recent().getByText('No runs yet.')).toBeTruthy();
});

it('names a run that is going as the live run, with how far it has got', async () => {
  const { session, end } = runningSession();
  render(RunsSidebar, { history: await historyOf([]), session, pathname: '/runs/live' });
  await vi.waitFor(() => expect(recent().getByRole('link').textContent).toContain('0 of 3'));
  const live = recent().getByRole('link');
  expect(live.textContent).toContain('Live run');
  expect(live.getAttribute('aria-current')).toBe('page');
  // the live run has its own row, so All runs does not light up with it (D669)
  expect(screen.getByRole('link', { name: 'All runs' }).hasAttribute('aria-current')).toBe(false);
  end();
});
