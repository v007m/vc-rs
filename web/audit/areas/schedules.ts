import type { MessageInitShape } from '@bufbuild/protobuf';
import type { Page } from '@playwright/test';
import type * as E from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import * as R from '#lib/gen/vibranium/results/v1/results_pb.ts';
import { click } from '../act.ts';
import type { Fakes } from '../engine.ts';
import { adhocRuns, now, runs } from '../fixtures.ts';
import type { State } from '../states.ts';

/** The engine answers the schedules screens need (batch E). */

const HOUR = 3_600_000n;

function run(id: bigint, ago: bigint, selector: string, failed: number) {
  const startedMs = now - ago;
  return {
    id,
    kind: 'run',
    startedMs,
    finishedMs: startedMs + 4_200n,
    profiles: ['prod'],
    selector,
    gitBranch: 'main',
    gitCommit: '7d41e0a9c2',
    vcVersion: '2.0.0-dev',
    status: failed ? R.RunStatus.FAILED : R.RunStatus.PASSED,
    totals: { passed: 12 - failed, failed, skipped: 1, flaky: 0 },
  } satisfies MessageInitShape<typeof R.RunSchema>;
}

const listings: MessageInitShape<typeof E.ScheduleListingSchema>[] = [
  {
    schedule: {
      id: 'nightly',
      name: 'Nightly smoke',
      cron: '0 2 * * *',
      selector: ['tag:smoke'],
      profiles: ['prod'],
      notify: [{ kind: 'slack', settings: [{ key: 'channel', value: '#api-alerts' }] }],
      runner: 'vcServer',
      enabled: true,
    },
    nextMs: now + 9n * HOUR,
    last: run(17n, 15n * HOUR, 'tag:smoke', 0),
  },
  {
    schedule: {
      id: 'quickSmoke',
      cron: '*/15 * * * *',
      selector: ['users.crud'],
      profiles: ['staging'],
      runner: 'vcServer',
      enabled: false,
    },
  },
  {
    schedule: {
      id: 'weeklyFull',
      name: 'Weekly full suite',
      cron: '30 6 * * 1',
      selector: [],
      profiles: ['staging', 'prod'],
      runner: 'githubActions',
      enabled: true,
    },
    nextMs: now + 4n * 24n * HOUR,
    last: run(12n, 3n * 24n * HOUR, '', 2),
  },
];

const configBefore = `schedules:
  - id: nightly
    name: Nightly smoke
    cron: "0 2 * * *"
    selector: [tag:smoke]
    profiles: [prod]
`;

const configAfter = configBefore.replace('"0 2 * * *"', '"0 3 * * *"');

const workflow = `name: Vibranium nightly
on:
  schedule:
    - cron: "0 2 * * *"
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: vc run tag:smoke --profile prod
`;

export const fakes: Fakes = {
  listSchedules: () => ({ schedules: listings }),
  writeSchedule: () => ({
    files: [{ path: 'vibranium.config.yaml', before: configBefore, after: configAfter }],
    titles: ['Change schedule nightly'],
    digest: 'd1',
  }),
  exportSchedule: ({ id }) => ({
    files: [{ path: `.github/workflows/vibranium-${id}.yml`, after: workflow }],
    titles: [`Export schedule ${id}`],
    digest: 'd1',
    path: `.github/workflows/vibranium-${id}.yml`,
  }),
};

/** Opens the ⋯ of the first schedule and picks `item`. */
async function rowAction(page: Page, item: string): Promise<void> {
  await click(page, 'nightly actions');
  await page.getByRole('menuitem', { name: item }).click();
}

/** The schedules screens and overlays the audit checks. */
export const states: State[] = [
  { name: 'schedules', path: '/runs/schedules' },
  {
    name: 'schedules-empty',
    path: '/runs/schedules',
    fakes: { listSchedules: () => ({ schedules: [] }) },
  },
  {
    name: 'schedules-edit',
    path: '/runs/schedules',
    act: async (page) => {
      await rowAction(page, 'Edit…');
      await page.getByLabel('Cron').fill('0 3 * * *');
      await click(page, 'Preview');
      await page.getByRole('button', { name: 'Write', exact: true }).waitFor();
    },
  },
  {
    name: 'schedules-export',
    path: '/runs/schedules',
    act: async (page) => {
      await rowAction(page, 'Export to GitHub Actions…');
      await page.getByRole('button', { name: 'Write workflow' }).waitFor();
    },
  },
  {
    name: 'runs-by-schedule',
    path: '/runs?schedule=nightly',
    fakes: {
      listRuns: ({ kind, schedule }) => ({
        runs:
          kind === 'adhoc'
            ? adhocRuns
            : runs.filter((r) => !schedule || r.selector === 'tag:smoke'),
      }),
    },
  },
];
