import { Code, ConnectError } from '@connectrpc/connect';
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
    listDefinitions: () => ({
      definitions: [...disk.keys()].map((path) => ({ path, id: 'users.crud' })),
    }),
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

const moved = {
  path: 'services/users/cruds.test.yaml',
  movedFrom: crud,
  before: 'name: crud\n',
  after: 'name: cruds\n',
};

/** File operations that all work, recording what they were asked. */
function workingFiles(asked: string[]): FileOps {
  const said = <T>(what: string, value: T) => {
    asked.push(what);
    return Promise.resolve(available(value));
  };
  return {
    create: (template, name, service) => said(`create ${template} ${service}/${name}`, crud),
    previewRename: (from, to) => said(`preview ${from} ${to}`, { files: [moved], digest: 'p1' }),
    rename: (from, to, digest) => said(`rename ${from} ${to} ${digest}`, [moved]),
    format: (_, text) => said('format', `${text.trim()}\n# formatted\n`),
    references: (path) =>
      said(`references ${path}`, {
        uses: [`orders.flow#placesOrder · services/orders/flow.test.yaml:4`],
        unread: ['services/orders/broken.test.yaml'],
      }),
    trash: (path, digest) => said(`trash ${path} ${digest}`, '/Trash/crud.test.yaml'),
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

  it('previews a rename of the file ID, then renames with its digest and opens the new path', async () => {
    const asked: string[] = [];
    const { editor, user } = await setup({ ...noEditorSeams, files: workingFiles(asked) });
    await pick(user, 'Rename or move…');
    const dialog = screen.getByRole('dialog', { name: 'Rename or move' });
    const to = within(dialog).getByLabelText('New file ID');
    expect(to).toHaveProperty('value', 'users.crud');
    await user.clear(to);
    await user.type(to, 'users.cruds');
    await user.click(within(dialog).getByRole('button', { name: 'Preview' }));
    expect(await within(dialog).findByText('1 file changes.')).toBeTruthy();
    expect(within(dialog).getByText(`${crud} → services/users/cruds.test.yaml`)).toBeTruthy();
    await user.click(within(dialog).getByRole('button', { name: 'Rename' }));
    expect(asked).toEqual(['preview users.crud users.cruds', 'rename users.crud users.cruds p1']);
    await vi.waitFor(() => expect(editor.buffer?.path).toBe('services/users/cruds.test.yaml'));
  });

  it('lists what refers to a file before moving it to the Trash, then says where it went', async () => {
    const asked: string[] = [];
    const { editor, user } = await setup({ ...noEditorSeams, files: workingFiles(asked) });
    await pick(user, 'Move to Trash…');
    const dialog = screen.getByRole('dialog', { name: 'Move to Trash' });
    expect(within(dialog).getByText(/to the Trash\?$/)).toBeTruthy();
    const use = 'orders.flow#placesOrder · services/orders/flow.test.yaml:4';
    expect(await within(dialog).findByText(use)).toBeTruthy();
    expect(within(dialog).getByText('services/orders/broken.test.yaml')).toBeTruthy();
    await user.click(within(dialog).getByRole('button', { name: 'Move to Trash' }));
    await vi.waitFor(() => expect(editor.buffer).toBeUndefined());
    expect(asked).toEqual([`references ${crud}`, `trash ${crud} d`]);
    expect(await screen.findByText('Moved to Trash: /Trash/crud.test.yaml')).toBeTruthy();
  });

  it('says Moved to Trash when the engine names no place', async () => {
    const files = { ...workingFiles([]), trash: () => Promise.resolve(available('')) };
    const { user } = await setup({ ...noEditorSeams, files });
    await pick(user, 'Move to Trash…');
    const dialog = screen.getByRole('dialog', { name: 'Move to Trash' });
    await user.click(within(dialog).getByRole('button', { name: 'Move to Trash' }));
    expect(await screen.findByText('Moved to Trash')).toBeTruthy();
  });

  it('shows the engine’s message and keeps the file when the host has no trash', async () => {
    const refused = new ConnectError('this host has no trash', Code.FailedPrecondition);
    const files = { ...workingFiles([]), trash: () => Promise.reject(refused) };
    const { editor, user } = await setup({ ...noEditorSeams, files });
    await pick(user, 'Move to Trash…');
    const dialog = screen.getByRole('dialog', { name: 'Move to Trash' });
    await user.click(within(dialog).getByRole('button', { name: 'Move to Trash' }));
    expect(await within(dialog).findByText('this host has no trash')).toBeTruthy();
    expect(editor.buffer?.path).toBe(crud);
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

  it('offers a test file, a load file and a service', async () => {
    await setup({ ...noEditorSeams, files: workingFiles([]) });
    await userEvent.setup().click(screen.getByRole('button', { name: 'New file…' }));
    const kinds = within(screen.getByRole('dialog', { name: 'New file' })).getAllByRole('option');
    expect(kinds.map((o) => o.textContent)).toEqual(['Test file', 'Load file', 'Service']);
  });

  it('shows the plan of the file as vc plan prints it: line, tree, notices', async () => {
    const tree = 'users.crud\n└─ getsUser\n   expect status: 200';
    const plan = {
      plan: () =>
        Promise.resolve(
          available({
            line: '1 instance in 1 file',
            tree,
            instances: 1,
            notices: ['profile dev from the config'],
          }),
        ),
    };
    const { user } = await setup({ ...noEditorSeams, plan });
    await pick(user, 'Plan preview…');
    const dialog = await screen.findByRole('dialog', { name: `Plan of ${crud}` });
    expect(await within(dialog).findByText('1 instance in 1 file')).toBeTruthy();
    expect(dialog.querySelector('pre')?.textContent).toBe(tree);
    expect(within(dialog).getByText('profile dev from the config')).toBeTruthy();
  });

  it('keeps a failed file action quiet when the engine lacks it', async () => {
    const files = { ...workingFiles([]), references: () => Promise.resolve(notYet('nope')) };
    const { user } = await setup({ ...noEditorSeams, files });
    await pick(user, 'Move to Trash…');
    const dialog = screen.getByRole('dialog', { name: 'Move to Trash' });
    expect(await within(dialog).findByText('nope')).toBeTruthy();
    expect(within(dialog).getByRole('button', { name: 'Move to Trash' })).toHaveProperty(
      'disabled',
      true,
    );
  });
});
