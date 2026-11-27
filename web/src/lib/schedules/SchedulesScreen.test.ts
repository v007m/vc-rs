import type { MessageInitShape } from '@bufbuild/protobuf';
import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import type {
  ExportScheduleRequest,
  ScheduleListingSchema,
  WriteScheduleRequest,
} from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { RunStatus } from '#lib/gen/vibranium/results/v1/results_pb.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { Toasts } from '#lib/ui/toasts.svelte.ts';
import { engineSchedulesPort } from './port.ts';
import { ScheduleList } from './schedules.svelte.ts';
import SchedulesScreen from './SchedulesScreen.svelte';

const HOUR = 3_600_000n;
const now = BigInt(Date.now());

const LISTINGS: MessageInitShape<typeof ScheduleListingSchema>[] = [
  {
    schedule: {
      id: 'nightly',
      name: 'Nightly smoke',
      cron: '0 2 * * *',
      selector: ['tag:smoke'],
      profiles: ['prod'],
      notify: [{ kind: 'slack', settings: [{ key: 'channel', value: '#api' }] }],
      runner: 'vcServer',
      enabled: true,
    },
    nextMs: now + 9n * HOUR,
    last: {
      id: 17n,
      kind: 'run',
      startedMs: now - 15n * HOUR,
      finishedMs: now - 15n * HOUR + 4_200n,
      status: RunStatus.PASSED,
    },
  },
  {
    schedule: {
      id: 'quickSmoke',
      cron: '7 1-3 */2 * *',
      selector: [],
      profiles: [],
      runner: 'githubActions',
      enabled: false,
    },
  },
];

const diff = {
  files: [{ path: 'vibranium.config.yaml', before: 'schedules: []\n', after: 'schedules:\n' }],
  titles: ['Change schedules'],
  digest: 'd7',
};

async function rendered(listings = LISTINGS) {
  const writes: WriteScheduleRequest[] = [];
  const exports: ExportScheduleRequest[] = [];
  const engine = fakeEngine({
    listSchedules: () => ({ schedules: listings }),
    writeSchedule: (request) => {
      writes.push(request);
      return diff;
    },
    exportSchedule: (request) => {
      exports.push(request);
      return { ...diff, path: `.github/workflows/vibranium-${request.id}.yml` };
    },
  });
  const toasts = new Toasts(() => undefined);
  const list = new ScheduleList(engineSchedulesPort(engine), toasts);
  await list.load();
  render(SchedulesScreen, { list, profiles: ['dev', 'staging', 'prod'] });
  return { list, toasts, writes, exports, user: userEvent.setup() };
}

const row = (name: RegExp) => screen.getByRole('row', { name });
const said = (toasts: Toasts) => toasts.list.map(({ text }) => text);
const sent = (writes: WriteScheduleRequest[]) =>
  writes.map(({ write, expectedDigest, remove }) => [write, expectedDigest, remove]);

async function pick(user: ReturnType<typeof userEvent.setup>, id: string, item: string) {
  await user.click(screen.getByRole('button', { name: `${id} actions` }));
  await user.click(await screen.findByRole('menuitem', { name: item }));
}

describe('SchedulesScreen', () => {
  it('lists each schedule with its cron in words, what runs, when and how it last went', async () => {
    await rendered();
    const nightly = row(/Nightly smoke/);
    for (const text of [
      'Every day at 02:00 UTC',
      '0 2 * * *',
      'tag:smoke',
      'prod',
      'vc ui server',
      'in 9 h',
      '#17',
      'Passed',
    ]) {
      expect(nightly.textContent).toContain(text);
    }
    expect(within(nightly).getByRole('link', { name: 'Nightly smoke' }).getAttribute('href')).toBe(
      '/runs?schedule=nightly',
    );
    expect(within(nightly).getByRole('link', { name: /#17/ }).getAttribute('href')).toBe(
      '/runs/17',
    );
    const next = nightly.querySelector('time');
    expect(next?.getAttribute('title')).toBeTruthy();
    expect(screen.getByText('2 schedules')).toBeTruthy();
  });

  it('shows a cron it cannot word as written, and a schedule that has not run', async () => {
    await rendered();
    const quick = row(/quickSmoke/);
    for (const text of ['7 1-3 */2 * *', 'all tests', 'default', 'GitHub Actions', 'Not run yet']) {
      expect(quick.textContent).toContain(text);
    }
    expect(
      screen.getByRole<HTMLInputElement>('switch', { name: 'Schedule quickSmoke' }).checked,
    ).toBe(false);
  });

  it('offers one New schedule button when there are none', async () => {
    await rendered([]);
    expect(screen.getByText('No schedules yet')).toBeTruthy();
    expect(screen.getAllByRole('button', { name: 'New schedule' })).toHaveLength(1);
    expect(screen.queryByRole('table')).toBeNull();
  });

  it('says why on an engine without schedules', async () => {
    const list = new ScheduleList(engineSchedulesPort(fakeEngine({})));
    await list.load();
    render(SchedulesScreen, { list, profiles: [] });
    expect(screen.getByText('Schedules are not available')).toBeTruthy();
    expect(screen.getByText('Schedules need a newer engine.')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'New schedule' })).toBeNull();
  });

  it('turns a schedule on with a write of the dry run digest', async () => {
    const { toasts, writes, user } = await rendered();
    await user.click(screen.getByRole('switch', { name: 'Schedule quickSmoke' }));
    expect(sent(writes)).toEqual([
      [false, undefined, undefined],
      [true, 'd7', undefined],
    ]);
    expect(writes[1]?.schedule).toMatchObject({ id: 'quickSmoke', enabled: true });
    expect(said(toasts)).toEqual(['Turned on schedule quickSmoke.']);
  });

  it('previews a new schedule, then writes it with the preview digest', async () => {
    const { toasts, writes, user } = await rendered();
    await user.click(screen.getByRole('button', { name: 'New schedule' }));
    const dialog = screen.getByRole('dialog', { name: 'New schedule' });
    const field = (name: string) => within(dialog).getByLabelText(name);
    await user.click(within(dialog).getByRole('button', { name: 'Preview' }));
    expect(within(dialog).getByText('Give the schedule an ID.')).toBeTruthy();
    expect(writes).toEqual([]);

    await user.type(field('ID'), 'weeklyFull');
    await user.type(field('Name'), 'Weekly full');
    await user.clear(field('Cron'));
    await user.type(field('Cron'), '30 6 * * 1');
    expect(within(dialog).getByText('Every Monday at 06:30 UTC')).toBeTruthy();
    await user.type(field('Selector'), 'tag:full users.crud');
    await user.click(within(dialog).getByRole('checkbox', { name: 'staging' }));
    await user.click(within(dialog).getByRole('button', { name: 'GitHub Actions' }));
    await user.click(within(dialog).getByRole('button', { name: 'Preview' }));
    expect(within(dialog).getByText('vibranium.config.yaml')).toBeTruthy();

    await user.click(within(dialog).getByRole('button', { name: 'Write' }));
    expect(sent(writes)).toEqual([
      [false, undefined, undefined],
      [true, 'd7', undefined],
    ]);
    expect(writes[1]?.schedule).toMatchObject({
      id: 'weeklyFull',
      name: 'Weekly full',
      cron: '30 6 * * 1',
      selector: ['tag:full', 'users.crud'],
      profiles: ['staging'],
      runner: 'githubActions',
      enabled: true,
    });
    expect(said(toasts)).toEqual(['Saved schedule weeklyFull.']);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('drops the preview when a field changes after it', async () => {
    const { user } = await rendered();
    await pick(user, 'nightly', 'Edit…');
    const dialog = screen.getByRole('dialog', { name: 'Edit Nightly smoke' });
    await user.click(within(dialog).getByRole('button', { name: 'Preview' }));
    expect(within(dialog).getByRole('button', { name: 'Write' })).toBeTruthy();
    await user.type(within(dialog).getByLabelText('Name'), ' 2');
    expect(within(dialog).getByRole('button', { name: 'Preview' })).toBeTruthy();
  });

  it('edits a schedule under its ID, keeping its notify channels', async () => {
    const { writes, user } = await rendered();
    await pick(user, 'nightly', 'Edit…');
    const dialog = screen.getByRole('dialog', { name: 'Edit Nightly smoke' });
    const id = within(dialog).getByLabelText<HTMLInputElement>('ID');
    expect(id.value).toBe('nightly');
    expect(id.disabled).toBe(true);
    const cron = within(dialog).getByLabelText('Cron');
    await user.clear(cron);
    await user.type(cron, '0 3 * * *');
    await user.click(within(dialog).getByRole('button', { name: 'Preview' }));
    await user.click(within(dialog).getByRole('button', { name: 'Write' }));
    expect(writes[1]?.schedule).toMatchObject({
      id: 'nightly',
      cron: '0 3 * * *',
      notify: [{ kind: 'slack', settings: [{ key: 'channel', value: '#api' }] }],
    });
  });

  it('removes a schedule after showing the diff', async () => {
    const { toasts, writes, user } = await rendered();
    await pick(user, 'nightly', 'Remove…');
    const dialog = screen.getByRole('dialog', { name: 'Remove Nightly smoke' });
    expect(await within(dialog).findByText('vibranium.config.yaml')).toBeTruthy();
    await user.click(within(dialog).getByRole('button', { name: 'Remove' }));
    expect(sent(writes)).toEqual([
      [false, undefined, 'nightly'],
      [true, 'd7', 'nightly'],
    ]);
    expect(said(toasts)).toEqual(['Removed schedule nightly.']);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('exports a schedule to GitHub Actions and says where it wrote the workflow', async () => {
    const { toasts, exports, user } = await rendered();
    await pick(user, 'nightly', 'Export to GitHub Actions…');
    const dialog = screen.getByRole('dialog', { name: 'Export Nightly smoke to GitHub Actions' });
    await user.click(await within(dialog).findByRole('button', { name: 'Write workflow' }));
    expect(exports.map(({ id, write, expectedDigest }) => [id, write, expectedDigest])).toEqual([
      ['nightly', false, undefined],
      ['nightly', true, 'd7'],
    ]);
    const done = 'Wrote .github/workflows/vibranium-nightly.yml.';
    expect(within(dialog).getByText(done)).toBeTruthy();
    expect(said(toasts)).toEqual([done]);
    await user.click(within(dialog).getByRole('button', { name: 'Done' }));
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});
