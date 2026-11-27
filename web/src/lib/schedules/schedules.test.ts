import { ConnectError } from '@connectrpc/connect';
import { describe, expect, it } from 'vitest';
import { available, notYet, type Maybe } from '#lib/api/maybe.ts';
import { Toasts } from '#lib/ui/toasts.svelte.ts';
import type { Preview } from '#lib/writes/staged.svelte.ts';
import { Staged } from '#lib/writes/staged.svelte.ts';
import type { ScheduleChange, ScheduleLine, SchedulesPort } from './port.ts';
import { doneText, ScheduleList, specOf } from './schedules.svelte.ts';

const nightly: ScheduleLine = {
  id: 'nightly',
  name: 'Nightly smoke',
  cron: '0 2 * * *',
  selector: ['tag:smoke'],
  profiles: ['prod'],
  notify: [],
  runner: 'vcServer',
  enabled: true,
  nextMs: 1_795_732_400_000,
};

const dry = (digest: string): Maybe<Preview> => available({ files: [], digest });

type Call = [string, ScheduleChange | string, string?];

function setup(over: Partial<SchedulesPort> = {}) {
  const calls: Call[] = [];
  const port: SchedulesPort = {
    list: () => Promise.resolve(available([nightly])),
    preview: (change) => {
      calls.push(['preview', change]);
      return Promise.resolve(dry('d1'));
    },
    write: (change, digest) => {
      calls.push(['write', change, digest]);
      return Promise.resolve(dry('d2'));
    },
    previewExport: (id) => {
      calls.push(['previewExport', id]);
      return Promise.resolve(dry('e1'));
    },
    export: (id, digest) => {
      calls.push(['export', id, digest]);
      return Promise.resolve(available(`.github/workflows/vibranium-${id}.yml`));
    },
    ...over,
  };
  const toasts = new Toasts(() => undefined);
  return { list: new ScheduleList(port, toasts), toasts, calls };
}

const said = (toasts: Toasts) => toasts.list.map(({ text, tone }) => [tone, text]);

describe('ScheduleList', () => {
  it('lists the schedules', async () => {
    const { list } = setup();
    await list.load();
    expect(list.loaded).toBe(true);
    expect(list.lines.map((line) => line.id)).toEqual(['nightly']);
    expect(list.reason).toBeUndefined();
  });

  it('says why on an engine without schedules', async () => {
    const { list } = setup({ list: () => Promise.resolve(notYet('Too old.')) });
    await list.load();
    expect(list.reason).toBe('Too old.');
    expect(list.lines).toEqual([]);
  });

  it('writes a previewed change with its digest, then toasts and lists again', async () => {
    const { list, toasts, calls } = setup();
    const staged = new Staged();
    const change = { put: specOf(nightly) };
    await list.review(staged, change);
    expect(await list.write(staged, change)).toBe(true);
    expect(calls).toEqual([
      ['preview', change],
      ['write', change, 'd1'],
    ]);
    expect(said(toasts)).toEqual([['done', 'Saved schedule nightly.']]);
    expect(list.loaded).toBe(true);
  });

  it('toasts a failed write and keeps the reason inline', async () => {
    const { list, toasts } = setup({
      write: () => Promise.reject(new ConnectError('the config changed since the preview')),
    });
    const staged = new Staged();
    await list.review(staged, { remove: 'nightly' });
    expect(await list.write(staged, { remove: 'nightly' })).toBe(false);
    expect(staged.note).toBe('the config changed since the preview');
    expect(said(toasts)).toEqual([['error', 'the config changed since the preview']]);
  });

  it('turns a schedule off at once with a dry run and a write of its digest', async () => {
    const { list, toasts, calls } = setup();
    await list.load();
    await list.toggle(nightly, false);
    const off = { put: { ...specOf(nightly), enabled: false } };
    expect(calls).toEqual([
      ['preview', off],
      ['write', off, 'd1'],
    ]);
    expect(said(toasts)).toEqual([['done', 'Turned off schedule nightly.']]);
    expect(list.toggling).toBeUndefined();
  });

  it('moves the switch back when turning a schedule off fails', async () => {
    const { list, toasts } = setup({
      list: () => Promise.resolve(available([nightly])),
      write: () => Promise.reject(new ConnectError('config.yaml is read-only')),
    });
    await list.load();
    await list.toggle(nightly, false);
    expect(list.lines[0]?.enabled).toBe(true);
    expect(list.error).toBe('config.yaml is read-only');
    expect(said(toasts)).toEqual([['error', 'config.yaml is read-only']]);
  });

  it('exports a workflow with the preview digest and gives its path', async () => {
    const { list, toasts, calls } = setup();
    const staged = new Staged();
    await list.reviewExport(staged, 'nightly');
    expect(await list.writeExport(staged, 'nightly')).toBe(
      '.github/workflows/vibranium-nightly.yml',
    );
    expect(calls).toEqual([
      ['previewExport', 'nightly'],
      ['export', 'nightly', 'e1'],
    ]);
    expect(said(toasts)).toEqual([['done', 'Wrote .github/workflows/vibranium-nightly.yml.']]);
  });

  it('keeps only the spec of a listed line, and words each write', () => {
    expect(specOf(nightly)).not.toHaveProperty('nextMs');
    expect(doneText({ remove: 'nightly' })).toBe('Removed schedule nightly.');
    expect(doneText({ put: specOf(nightly) })).toBe('Saved schedule nightly.');
  });
});
