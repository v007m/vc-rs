import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { DefinitionKind } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import type { SkipInput, SkipPort } from '#lib/skip/port.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { Insights } from './insights.svelte.ts';
import InsightsScreen from './InsightsScreen.svelte';
import {
  enginePort,
  type InsightsPort,
  type RunSummary,
  type Scope,
  type SkipLine,
} from './port.ts';

const t0 = Date.UTC(2026, 10, 10, 9);
const runs: RunSummary[] = [
  {
    id: 3,
    startedMs: t0 + 2e6,
    durationMs: 4_000,
    passed: 9,
    failed: 0,
    blocked: 1,
    flaky: 0,
    branch: 'main',
  },
  {
    id: 2,
    startedMs: t0 + 1e6,
    durationMs: 2_000,
    passed: 10,
    failed: 0,
    blocked: 0,
    flaky: 0,
    branch: 'tax-line',
  },
];

const CRUD = `tests:
  - id: createsOne
    steps:
      - call: users#create
`;

/** A skip port that answers with a real test file's change and keeps what it was sent. */
function skipPort(sent: [SkipInput | string, string | undefined][]): SkipPort {
  const after = `${CRUD}    skip:\n      reason: flaky on staging\n      until: 2026-11-20\n`;
  const files = [{ path: 'services/users/crud.test.yaml', before: CRUD, after }];
  return {
    skip: (input, digest) => {
      sent.push([input, digest]);
      return Promise.resolve({ files, titles: [`Skip ${input.target}`], digest: 'd1' });
    },
    unskip: (target, digest) => {
      sent.push([target, digest]);
      return Promise.resolve({ files, titles: [`Unskip ${target}`], digest: 'd2' });
    },
  };
}

const TODAY = '2026-11-12';

/** One skip per case: ended, ends in a week, ends in 14 days (still listed), in 15 (not listed). */
const SKIPS: SkipLine[] = [
  { id: 'users.crud#deletesOne', until: '2026-11-19', reason: 'waits on #41', ended: false },
  { id: 'users.crud#updatesOne', until: '2026-11-27', ended: false },
  { id: 'users.crud#listsAll', until: '2026-11-26', ended: false },
  { id: 'users.crud#getsOne', until: '2026-11-10', reason: 'flaky upstream', ended: true },
];

async function rendered(port?: Partial<InsightsPort>) {
  const asked: number[] = [];
  const skipReads: number[] = [];
  const scopes: Scope[] = [];
  const sent: [SkipInput | string, string | undefined][] = [];
  const insights = new Insights(
    {
      runs: (limit, scope) => {
        asked.push(limit);
        scopes.push(scope);
        return Promise.resolve(runs);
      },
      skips: () => {
        skipReads.push(skipReads.length + 1);
        return Promise.resolve(SKIPS);
      },
      trends: () =>
        Promise.resolve([
          { testId: 'users.crud#createsOne', results: 2, failed: 0, flaky: 1, meanMs: 12 },
          { testId: 'users.crud#getsOne', results: 2, failed: 0, flaky: 0, meanMs: 8 },
        ]),
      ...port,
    },
    TODAY,
  );
  await insights.open();
  render(InsightsScreen, { insights, skips: skipPort(sent) });
  return { asked, scopes, sent, skipReads, user: userEvent.setup() };
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

  it('offers no profile or branch filter yet, and reads the whole history', async () => {
    const { scopes } = await rendered();
    expect(screen.queryByRole('group', { name: 'Profile' })).toBeNull();
    expect(screen.queryByRole('group', { name: 'Branch' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'staging' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'main' })).toBeNull();
    expect(scopes).toEqual([{}]);
  });

  it('skips a flaky test until a date after a preview, and undoes it', async () => {
    const { sent, user } = await rendered();
    await user.click(screen.getByRole('button', { name: 'Skip users.crud#createsOne until…' }));
    const form = screen.getByRole('group', { name: 'Skip users.crud#createsOne' });
    await user.type(within(form).getByLabelText('Until'), '2026-11-20');
    await user.type(within(form).getByRole('textbox', { name: 'Reason' }), 'flaky on staging');
    await user.click(within(form).getByRole('button', { name: 'Preview' }));
    expect(within(form).getByRole('region').textContent).toContain('until: 2026-11-20');
    await user.click(within(form).getByRole('button', { name: 'Write skip' }));
    const input = {
      target: 'users.crud#createsOne',
      reason: 'flaky on staging',
      until: '2026-11-20',
    };
    expect(sent).toEqual([
      [input, undefined],
      [input, 'd1'],
    ]);
    expect(screen.getByText('skipped until 2026-11-20')).toBeTruthy();
    await user.click(
      screen.getByRole('button', { name: 'Undo the skip of users.crud#createsOne' }),
    );
    const undo = screen.getByRole('group', { name: 'Unskip users.crud#createsOne' });
    await user.click(within(undo).getByRole('button', { name: 'Preview' }));
    await user.click(within(undo).getByRole('button', { name: 'Remove skip' }));
    expect(sent.slice(2)).toEqual([
      ['users.crud#createsOne', undefined],
      ['users.crud#createsOne', 'd2'],
    ]);
    expect(screen.getByRole('button', { name: 'Skip users.crud#createsOne until…' })).toBeTruthy();
  });

  it('lists the skips that end within 14 days or have ended, soonest first', async () => {
    await rendered();
    const panel = screen.getByRole('region', { name: 'Skips expiring soon' });
    const rows = within(panel)
      .getAllByRole('listitem')
      .map((li) => li.textContent.replace(/\s+/g, ' ').trim());
    expect(rows).toEqual([
      'users.crud#getsOne ended 2026-11-10 flaky upstream Unskip…',
      'users.crud#deletesOne until 2026-11-19 waits on #41 Unskip…',
      'users.crud#listsAll until 2026-11-26 Unskip…',
    ]);
  });

  it('unskips an expiring test after a preview and reads the skips again', async () => {
    const { sent, skipReads, user } = await rendered();
    const panel = screen.getByRole('region', { name: 'Skips expiring soon' });
    await user.click(within(panel).getByRole('button', { name: 'Unskip users.crud#getsOne' }));
    const form = within(panel).getByRole('group', { name: 'Unskip users.crud#getsOne' });
    await user.click(within(form).getByRole('button', { name: 'Preview' }));
    await user.click(within(form).getByRole('button', { name: 'Remove skip' }));
    expect(sent).toEqual([
      ['users.crud#getsOne', undefined],
      ['users.crud#getsOne', 'd2'],
    ]);
    expect(skipReads).toEqual([1, 2]);
    expect(within(panel).queryByRole('group')).toBeNull();
  });

  it('says when no skip ends soon', async () => {
    await rendered({ skips: () => Promise.resolve([]) });
    const panel = screen.getByRole('region', { name: 'Skips expiring soon' });
    expect(within(panel).getByText('No skip ends in the next 14 days.')).toBeTruthy();
  });

  it('says so when there is no history yet', async () => {
    await rendered({ runs: () => Promise.resolve([]), trends: () => Promise.resolve([]) });
    expect(screen.getByText('No flaky or failing tests in this range.')).toBeTruthy();
    expect(screen.getAllByText('No runs in this range yet.')).toHaveLength(2);
  });
});

describe('enginePort', () => {
  it('asks for the last `run` runs in a scope and times the finished ones', async () => {
    const asked: unknown[] = [];
    const port = enginePort(
      fakeEngine({
        listRuns: ({ limit, kind, profile, branch }) => {
          asked.push({ limit, kind, profile, branch });
          return {
            runs: [
              {
                id: 5n,
                kind: 'run',
                startedMs: 1000n,
                finishedMs: 3500n,
                totals: { passed: 2 },
                gitBranch: 'main',
              },
              { id: 3n, kind: 'run', startedMs: 100n },
            ],
          };
        },
        trends: ({ window, profile, branch }) => {
          asked.push({ window, profile, branch });
          return { trends: [] };
        },
        listDefinitions: ({ kind, skipped }) => {
          asked.push({ kind, skipped });
          return {
            definitions: [
              { id: 'users.crud#getsOne', skipped: true, skipUntil: '2026-11-20', skipReason: 'x' },
              { id: 'users.crud#listsAll', skipped: true, skipReason: 'no end date' },
              { id: 'users.crud#updatesOne', skipUntil: '2026-11-01', skipExpired: true },
            ],
          };
        },
      }),
    );
    expect(await port.skips()).toEqual([
      { id: 'users.crud#getsOne', until: '2026-11-20', reason: 'x', ended: false },
      { id: 'users.crud#updatesOne', until: '2026-11-01', ended: true },
    ]);
    expect(await port.runs(10, { profile: 'staging', branch: 'main' })).toEqual([
      {
        id: 5,
        startedMs: 1000,
        durationMs: 2500,
        passed: 2,
        failed: 0,
        blocked: 0,
        flaky: 0,
        branch: 'main',
      },
      { id: 3, startedMs: 100, passed: 0, failed: 0, blocked: 0, flaky: 0 },
    ]);
    await port.trends(30, {});
    expect(asked).toEqual([
      { kind: DefinitionKind.TESTS, skipped: true },
      { limit: 10, kind: 'run', profile: 'staging', branch: 'main' },
      { window: 30, profile: undefined, branch: undefined },
    ]);
  });
});
