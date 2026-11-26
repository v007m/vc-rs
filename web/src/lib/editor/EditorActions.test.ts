import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { available, notYet } from '#lib/api/maybe.ts';
import { CancelLevel } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { RunSession } from '#lib/run/session.svelte.ts';
import { findCodeView, typeAtEnd } from '#lib/testing/codeView.ts';
import { events, fakeEngine } from '#lib/testing/engine.ts';
import Editor from './Editor.svelte';
import { noEditorSeams, type EditorSeams, type FileOps } from './seams.ts';
import { EditorSession } from './session.svelte.ts';

const crud = 'services/users/crud.test.yaml';

/** A project of one test file, whose run waits until it is cancelled. */
async function setup(seams: EditorSeams = noEditorSeams) {
  const disk = new Map([[crud, 'name: crud\n']]);
  const checks: string[] = [];
  const cancels: CancelLevel[] = [];
  const engine = fakeEngine({
    listDefinitions: () => ({ definitions: [...disk.keys()].map((path) => ({ path })) }),
    readFile: ({ path }) => ({ text: disk.get(path) ?? '', digest: 'd' }),
    check: ({ overlay }) => {
      checks.push(overlay[crud] ?? '');
      return { diagnostics: [] };
    },
    async *run(_, context) {
      yield { event: events({ case: 'runStarted', value: { instances: 1 } })[0] };
      await new Promise((resolve) => context.signal.addEventListener('abort', resolve));
    },
    cancel: ({ level }) => {
      cancels.push(level);
      return {};
    },
  });
  const editor = new EditorSession(engine, 60_000);
  await editor.loadFiles();
  await editor.open(crud);
  render(Editor, { editor, session: new RunSession(engine), seams });
  return { disk, checks, cancels, editor, user: userEvent.setup() };
}

/** File operations that all work, recording what they were asked. */
function workingFiles(asked: string[]): FileOps {
  const said = <T>(what: string, value: T) => {
    asked.push(what);
    return Promise.resolve(available(value));
  };
  return {
    create: (template, name, service) => said(`create ${template} ${service}/${name}`, crud),
    previewRename: (from, to) =>
      said(`preview ${from} ${to}`, [{ path: to, before: 'name: crud\n', after: 'name: cruds\n' }]),
    rename: (from, to) => said(`rename ${from} ${to}`, undefined),
    format: (_, text) => said('format', `${text.trim()}\n# formatted\n`),
    references: (path) => said(`references ${path}`, ['orders.flow#placesOrder']),
    trash: (path) => said(`trash ${path}`, undefined),
  };
}

async function pick(user: ReturnType<typeof userEvent.setup>, action: string) {
  await user.click(screen.getByRole('button', { name: 'File actions' }));
  await user.click(screen.getByRole('menuitem', { name: action }));
}

describe('Editor actions', () => {
  it('checks the typed text at once on Check', async () => {
    const { checks, user } = await setup();
    typeAtEnd(await findCodeView(crud), 'id: x\n');
    await user.click(screen.getByRole('button', { name: 'Check' }));
    expect(checks.at(-1)).toBe('name: crud\nid: x\n');
  });

  it('runs the file and stops it from the bar, then skips the cleanups', async () => {
    const { cancels, user } = await setup();
    await user.click(screen.getByRole('button', { name: 'Run file' }));
    await user.click(await screen.findByRole('button', { name: 'Stop' }));
    expect(await screen.findByText('Stopping: running the cleanups')).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Skip cleanups' }));
    expect(cancels).toEqual([CancelLevel.STOP, CancelLevel.ABANDON]);
  });

  it('says why each file action is missing on an engine without them', async () => {
    const { user } = await setup();
    await pick(user, 'Format');
    expect(await screen.findByText('File operations need a newer engine.')).toBeTruthy();
    await pick(user, 'Plan preview…');
    const plan = await screen.findByRole('dialog', { name: `Plan of ${crud}` });
    expect(await within(plan).findByText('The plan preview needs a newer engine.')).toBeTruthy();
  });

  it('formats the unsaved text through vc fmt', async () => {
    const asked: string[] = [];
    const { editor, user } = await setup({ ...noEditorSeams, files: workingFiles(asked) });
    await pick(user, 'Format');
    await vi.waitFor(() => expect(editor.buffer?.text).toBe('name: crud\n# formatted\n'));
    expect(editor.dirty).toBe(true);
  });

  it('previews a rename before making it, then opens the file at its new path', async () => {
    const asked: string[] = [];
    const { user } = await setup({ ...noEditorSeams, files: workingFiles(asked) });
    await pick(user, 'Rename or move…');
    const dialog = screen.getByRole('dialog', { name: 'Rename or move' });
    const to = within(dialog).getByLabelText('New path');
    await user.clear(to);
    await user.type(to, 'services/users/cruds.test.yaml');
    await user.click(within(dialog).getByRole('button', { name: 'Preview' }));
    expect(await within(dialog).findByText('1 file changes.')).toBeTruthy();
    await user.click(within(dialog).getByRole('button', { name: 'Rename' }));
    expect(asked).toEqual([
      `preview ${crud} services/users/cruds.test.yaml`,
      `rename ${crud} services/users/cruds.test.yaml`,
    ]);
  });

  it('lists the tests that use a file before moving it to the trash', async () => {
    const asked: string[] = [];
    const { editor, user } = await setup({ ...noEditorSeams, files: workingFiles(asked) });
    await pick(user, 'Move to trash…');
    const dialog = screen.getByRole('dialog', { name: 'Move to trash' });
    expect(await within(dialog).findByText('orders.flow#placesOrder')).toBeTruthy();
    await user.click(within(dialog).getByRole('button', { name: 'Move to trash' }));
    await vi.waitFor(() => expect(editor.buffer).toBeUndefined());
    expect(asked).toEqual([`references ${crud}`, `trash ${crud}`]);
  });

  it('makes a new test file in the open file’s service', async () => {
    const asked: string[] = [];
    const { user } = await setup({ ...noEditorSeams, files: workingFiles(asked) });
    await user.click(screen.getByRole('button', { name: 'New file…' }));
    const dialog = screen.getByRole('dialog', { name: 'New file' });
    await user.type(within(dialog).getByLabelText('Area'), 'search');
    await user.click(within(dialog).getByRole('button', { name: 'Create' }));
    await vi.waitFor(() => expect(asked).toEqual(['create test users/search']));
  });

  it('shows the plan of the file: instances, merged expects and the lifecycle order', async () => {
    const plan = {
      plan: () =>
        Promise.resolve(
          available({
            instances: [
              { id: 'users.crud#getsUser', name: 'gets a user', expects: ['status: 200'] },
            ],
            order: [
              { phase: 'beforeAll', target: 'users.crud' },
              { phase: 'test', target: 'users.crud#getsUser' },
            ],
          }),
        ),
    };
    const { user } = await setup({ ...noEditorSeams, plan });
    await pick(user, 'Plan preview…');
    const dialog = await screen.findByRole('dialog', { name: `Plan of ${crud}` });
    expect(await within(dialog).findByText('1 instance')).toBeTruthy();
    const expects = within(dialog).getByRole('list', { name: 'Expects of users.crud#getsUser' });
    expect(expects.textContent).toBe('status: 200');
    expect(within(dialog).getByText('beforeAll')).toBeTruthy();
  });

  it('keeps a failed file action quiet when the engine lacks it', async () => {
    const files = { ...workingFiles([]), references: () => Promise.resolve(notYet('nope')) };
    const { user } = await setup({ ...noEditorSeams, files });
    await pick(user, 'Move to trash…');
    const dialog = screen.getByRole('dialog', { name: 'Move to trash' });
    expect(await within(dialog).findByText('nope')).toBeTruthy();
    expect(within(dialog).getByRole('button', { name: 'Move to trash' })).toHaveProperty(
      'disabled',
      true,
    );
  });
});
