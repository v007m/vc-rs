import { describe, expect, it } from 'vitest';
import { CompletionKind } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { assistNotYet, wordStart } from './assist.ts';
import { engineEditorSeams, newFile } from './port.ts';
import { explainNotYet, filesNotYet, fixesNotYet, movedTo, planNotYet } from './seams.ts';

const crud = 'services/users/crud.test.yaml';

describe('the editor seams over the engine', () => {
  it('completes from the start of the word before the cursor, with each kind and doc', async () => {
    const seams = engineEditorSeams(
      fakeEngine({
        complete: () => ({
          items: [
            { label: 'users.crud#getsUser', kind: CompletionKind.REFERENCE, detail: 'test' },
            { label: 'POST', kind: CompletionKind.VALUE, doc: 'The method.' },
            { label: 'userId', kind: CompletionKind.VAR },
            { label: 'eq', kind: CompletionKind.MATCHER },
            { label: 'name', kind: CompletionKind.KEY },
          ],
        }),
      }),
    );
    const text = 'use: ${users.cr';
    const got = await seams.assist.complete(crud, text, text.length);
    expect(got).toEqual({
      available: true,
      value: {
        from: 5,
        items: [
          { label: 'users.crud#getsUser', kind: 'id', detail: 'test' },
          { label: 'POST', kind: 'value', doc: 'The method.' },
          { label: 'userId', kind: 'var' },
          { label: 'eq', kind: 'matcher' },
          { label: 'name', kind: 'key' },
        ],
      },
    });
  });

  it('finds where the word before the cursor starts', () => {
    expect(wordStart('a: b', 4)).toBe(3);
    expect(wordStart('ref: users.crud#gets', 20)).toBe(5);
    expect(wordStart('a: ', 3)).toBe(3);
  });

  it('hovers with the engine’s Markdown, and with nothing when it has none', async () => {
    let markdown: string | undefined = '`name`: the test’s name';
    const seams = engineEditorSeams(fakeEngine({ hover: () => (markdown ? { markdown } : {}) }));
    expect(await seams.assist.hover(crud, 'name: x', 1)).toEqual({
      available: true,
      value: { markdown },
    });
    markdown = undefined;
    expect(await seams.assist.hover(crud, 'name: x', 1)).toEqual({
      available: true,
      value: undefined,
    });
  });

  it('explains a target and fixes one file as a preview', async () => {
    const fixes: unknown[] = [];
    const seams = engineEditorSeams(
      fakeEngine({
        explain: ({ target }) => ({ text: `# ${target}`, markdown: true, notices: ['masked'] }),
        applyFixes: ({ codes, paths, write }) => {
          fixes.push({ codes, paths, write });
          return {
            files: [
              { path: 'other.yaml', after: 'no' },
              { path: crud, after: 'fixed' },
            ],
          };
        },
      }),
    );
    expect(await seams.explain.explain('V2003')).toEqual({
      available: true,
      value: { text: '# V2003', markdown: true, notices: ['masked'] },
    });
    expect(await seams.fixes.fix(crud, ['V4001'])).toEqual({ available: true, value: 'fixed' });
    expect(fixes).toEqual([{ codes: ['V4001'], paths: [crud], write: false }]);
  });

  it('creates a file with a preview, then the write with its digest', async () => {
    const asked: unknown[] = [];
    const seams = engineEditorSeams(
      fakeEngine({
        createFile: ({ kind, id, write, expectedDigest }) => {
          asked.push({ kind, id, write, expectedDigest });
          return { digest: 'p1', path: `services/${id}.yaml` };
        },
      }),
    );
    const made = await seams.files.create('load', 'soak', 'users');
    expect(made).toEqual({ available: true, value: 'services/users.soak.yaml' });
    expect(asked).toEqual([
      { kind: 'load', id: 'users.soak', write: false, expectedDigest: undefined },
      { kind: 'load', id: 'users.soak', write: true, expectedDigest: 'p1' },
    ]);
    expect(newFile('test', 'crud', 'users')).toEqual(['test', 'users.crud']);
    expect(newFile('service', 'orders', 'users')).toEqual(['api', 'orders']);
  });

  it('renames by file ID, a new file’s before empty, and finds the moved file', async () => {
    const files = [
      { path: 'services/orders/flow.test.yaml', before: 'a', after: 'b' },
      { path: 'services/users/cruds.test.yaml', after: 'name: crud\n', movedFrom: crud },
    ];
    const asked: unknown[] = [];
    const seams = engineEditorSeams(
      fakeEngine({
        rename: ({ from, to, write, expectedDigest }) => {
          asked.push({ from, to, write, expectedDigest });
          return { files, digest: 'r1' };
        },
      }),
    );
    const preview = await seams.files.previewRename('users.crud', 'users.cruds');
    expect(preview.available && preview.value.digest).toBe('r1');
    expect(preview.available && preview.value.files[1]).toEqual({
      path: 'services/users/cruds.test.yaml',
      before: '',
      after: 'name: crud\n',
      movedFrom: crud,
    });
    const done = await seams.files.rename('users.crud', 'users.cruds', 'r1');
    expect(done.available && movedTo(done.value, crud)).toBe('services/users/cruds.test.yaml');
    expect(asked.at(-1)).toEqual({
      from: 'users.crud',
      to: 'users.cruds',
      write: true,
      expectedDigest: 'r1',
    });
    expect(movedTo([{ path: 'x', before: '', after: '' }], crud)).toBe('x');
  });

  it('formats, lists references and moves a file to the Trash with its digest', async () => {
    const trashed: unknown[] = [];
    const seams = engineEditorSeams(
      fakeEngine({
        format: ({ text }) => ({ text: `${text}\n`, changed: true }),
        fileReferences: () => ({
          references: [{ from: 'orders.flow#placesOrder', path: 'flow.test.yaml', line: 4 }],
          unread: ['broken.test.yaml'],
        }),
        trashFile: ({ path, expectedDigest }) => {
          trashed.push({ path, expectedDigest });
          return { trashedTo: '/Trash/crud.test.yaml' };
        },
      }),
    );
    expect(await seams.files.format(crud, 'a: 1')).toEqual({ available: true, value: 'a: 1\n' });
    expect(await seams.files.references(crud)).toEqual({
      available: true,
      value: { uses: ['orders.flow#placesOrder · flow.test.yaml:4'], unread: ['broken.test.yaml'] },
    });
    const done = await seams.files.trash(crud, 'd1');
    expect(done).toEqual({ available: true, value: '/Trash/crud.test.yaml' });
    expect(trashed).toEqual([{ path: crud, expectedDigest: 'd1' }]);
  });

  it('plans the file as vc plan prints it', async () => {
    const selections: unknown[] = [];
    const seams = engineEditorSeams(
      fakeEngine({
        plan: ({ selection }) => {
          selections.push(selection?.paths);
          return { line: '1 instance', tree: 'users.crud', instances: 1, notices: ['n'] };
        },
      }),
    );
    expect(await seams.plan.plan([crud])).toEqual({
      available: true,
      value: { line: '1 instance', tree: 'users.crud', instances: 1, notices: ['n'] },
    });
    expect(selections).toEqual([[crud]]);
  });

  it('says each needs a newer engine on an engine without them', async () => {
    const seams = engineEditorSeams(fakeEngine({}));
    const reasons = await Promise.all([
      seams.assist.complete(crud, '', 0),
      seams.files.trash(crud, 'd'),
      seams.plan.plan([crud]),
      seams.explain.explain('V2003'),
      seams.fixes.fix(crud, []),
    ]);
    expect(reasons.map((r) => (r.available ? 'available' : r.reason))).toEqual([
      assistNotYet,
      filesNotYet,
      planNotYet,
      explainNotYet,
      fixesNotYet,
    ]);
  });
});
