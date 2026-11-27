import { Code, ConnectError } from '@connectrpc/connect';
import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { Severity } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { RunSession } from '#lib/run/session.svelte.ts';
import { codeView, findCodeView, selected, typeAtEnd } from '#lib/testing/codeView.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { available } from '#lib/api/maybe.ts';
import Editor from './Editor.svelte';
import { noEditorSeams, type EditorSeams } from './seams.ts';
import { EditorSession } from './session.svelte.ts';

const crud = 'services/users/crud.test.yaml';
const api = 'services/users/users.api.yaml';

const badId = {
  code: 'V4001',
  severity: Severity.ERROR,
  message: 'test IDs are camelCase',
  primary: { span: { file: crud, startLine: 2, startColumn: 5, endLine: 2, endColumn: 12 } },
  fixes: [
    {
      title: 'Rename to getsUser',
      certain: true,
      edits: [{ file: crud, start: 15, end: 22, newText: 'getsUser' }],
    },
  ],
};

async function setup(conflict = { next: false }, seams: EditorSeams = noEditorSeams) {
  const disk = new Map([
    [crud, 'name: crud\nid: GetUser\n'],
    [api, 'name: users\n'],
  ]);
  const runs: string[][] = [];
  const records: (boolean | undefined)[] = [];
  const engine = fakeEngine({
    listDefinitions: () => ({ definitions: [...disk.keys()].map((path) => ({ path })) }),
    readFile: ({ path }) => ({ text: disk.get(path) ?? '', digest: 'd' }),
    writeFile: ({ path, text }) => {
      if (conflict.next) {
        conflict.next = false;
        throw new ConnectError('changed', Code.Aborted);
      }
      disk.set(path, text);
      return { digest: 'd', diagnostics: [] };
    },
    // The unsaved text when the editor sends it, else the file on disk.
    check: ({ paths, overlay }) => {
      const text = overlay[crud] ?? (paths.includes(crud) ? disk.get(crud) : '');
      return { diagnostics: text?.includes('GetUser') ? [badId] : [] };
    },
    // eslint-disable-next-line require-yield -- the run ends without events
    async *run(request) {
      runs.push(request.selection?.paths ?? []);
      records.push(request.record);
    },
  });
  const editor = new EditorSession(engine, 0);
  await editor.loadFiles();
  render(Editor, { editor, session: new RunSession(engine), seams });
  const user = userEvent.setup();
  const open = async (name: string) => {
    await user.click(within(screen.getByRole('tree', { name: 'Files' })).getByText(name));
  };
  return { disk, runs, records, user, open };
}

describe('Editor', () => {
  it('opens a file from the tree, edits it and saves with Ctrl+S', async () => {
    const { disk, user, open } = await setup();
    await open('users.api.yaml');
    typeAtEnd(await findCodeView(api), '# done');
    expect(await screen.findByText('Unsaved')).toBeTruthy();
    await user.keyboard('{Control>}s{/Control}');
    expect(await screen.findByText('Saved')).toBeTruthy();
    expect(disk.get(api)).toBe('name: users\n# done');
  });

  it('asks before saving text with errors, and saves it anyway on request', async () => {
    const { disk, user, open } = await setup();
    await open('crud.test.yaml');
    typeAtEnd(await findCodeView(crud), '# more');
    expect(await screen.findByText('Unsaved, 1 error')).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Save' }));
    const ask = await screen.findByRole('alert');
    expect(ask.textContent?.replace(/\s+/g, ' ')).toContain(
      'This file has 1 error. Save it anyway?',
    );
    expect(disk.get(crud)).toBe('name: crud\nid: GetUser\n');
    await user.click(within(ask).getByRole('button', { name: 'Save anyway' }));
    expect(await screen.findByText('Saved')).toBeTruthy();
    expect(disk.get(crud)).toBe('name: crud\nid: GetUser\n# more');
  });

  it('applies every certain fix with Fix all', async () => {
    const { user, open } = await setup();
    await open('crud.test.yaml');
    const view = await findCodeView(crud);
    await user.click(await screen.findByRole('button', { name: 'Fix all (1)' }));
    expect(view.state.doc.toString()).toBe('name: crud\nid: getsUser\n');
    expect(await screen.findByText('Unsaved')).toBeTruthy();
    expect(screen.queryByRole('button', { name: /^Fix all/ })).toBeNull();
  });

  it('fixes a saved file through the engine, unsaved, so Revert undoes it', async () => {
    const asked: string[][] = [];
    const fixes = {
      fix: (_: string, codes: readonly string[]) => {
        asked.push([...codes]);
        return Promise.resolve(available('name: crud\nid: getsUser # engine\n'));
      },
    };
    const { user, open } = await setup(undefined, { ...noEditorSeams, fixes });
    await open('crud.test.yaml');
    const view = await findCodeView(crud);
    const problems = screen.getByRole('region', { name: 'Problems' });
    await user.click(within(problems).getByRole('button', { name: 'Rename to getsUser' }));
    await vi.waitFor(() => expect(view.state.doc.toString()).toContain('# engine'));
    expect(asked).toEqual([['V4001']]);
    await user.click(screen.getByRole('button', { name: 'Revert' }));
    await user.click(await screen.findByRole('button', { name: 'Fix all (1)' }));
    await vi.waitFor(() => expect(asked).toEqual([['V4001'], []]));
  });

  it('says so when the engine finds nothing to fix', async () => {
    const fixes = { fix: () => Promise.resolve(available(undefined)) };
    const { user, open } = await setup(undefined, { ...noEditorSeams, fixes });
    await open('crud.test.yaml');
    await user.click(await screen.findByRole('button', { name: 'Fix all (1)' }));
    expect(await screen.findByText('Nothing to fix.')).toBeTruthy();
  });

  it('explains a problem’s code beside the code, and closes it', async () => {
    const explain = {
      explain: (target: string) =>
        Promise.resolve(
          available({ text: `# ${target}\n\nUse \`camelCase\`.`, markdown: true, notices: [] }),
        ),
    };
    const { user, open } = await setup(undefined, { ...noEditorSeams, explain });
    await open('crud.test.yaml');
    const problems = screen.getByRole('region', { name: 'Problems' });
    await user.click(within(problems).getByRole('button', { name: 'Explain V4001' }));
    const panel = await screen.findByRole('complementary', { name: 'Explain V4001' });
    expect(await within(panel).findByRole('heading', { name: 'V4001' })).toBeTruthy();
    expect(within(panel).getByText('camelCase').tagName).toBe('CODE');
    await user.click(within(panel).getByRole('button', { name: 'Close' }));
    expect(screen.queryByRole('complementary', { name: 'Explain V4001' })).toBeNull();
  });

  it('asks before dropping unsaved changes for another file', async () => {
    const { user, open } = await setup();
    await open('crud.test.yaml');
    typeAtEnd(await findCodeView(crud), 'x');
    await open('users.api.yaml');
    await user.click(screen.getByRole('button', { name: 'Keep editing' }));
    expect(codeView(crud)).toBeTruthy();
    await open('users.api.yaml');
    await user.click(screen.getByRole('button', { name: 'Discard and open' }));
    expect(await findCodeView(api)).toBeTruthy();
  });

  it('offers reload or keep-mine when the file changed on disk', async () => {
    const { disk, user, open } = await setup({ next: true });
    await open('users.api.yaml');
    typeAtEnd(await findCodeView(api), '!');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    const banner = await screen.findByRole('alert');
    expect(banner.textContent).toContain('changed on disk');
    await user.click(within(banner).getByRole('button', { name: 'Keep my version' }));
    expect(await screen.findByText('Saved')).toBeTruthy();
    expect(disk.get(api)).toContain('!');
  });

  it('lists problems, jumps to one and applies its fix', async () => {
    const { user, open } = await setup();
    await open('crud.test.yaml');
    const view = await findCodeView(crud);
    const problems = screen.getByRole('region', { name: 'Problems' });
    expect(within(problems).getByText('1 error')).toBeTruthy();
    await user.click(within(problems).getByRole('button', { name: /test IDs are camelCase/ }));
    await vi.waitFor(() => expect(selected(view)).toBe('GetUser'));
    expect(view.hasFocus).toBe(true);
    await user.click(within(problems).getByRole('button', { name: 'Rename to getsUser' }));
    expect(view.state.doc.toString()).toBe('name: crud\nid: getsUser\n');
  });

  it('shows the diff and reverts it', async () => {
    const { user, open } = await setup();
    await open('users.api.yaml');
    typeAtEnd(await findCodeView(api), 'tags: one');
    await user.click(screen.getByRole('button', { name: /^Diff/ }));
    const diff = screen.getByRole('region', { name: 'Changes since the last save' });
    expect(within(diff).getByText('tags: one')).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Revert' }));
    expect(within(diff).getByText('No changes since the last save.')).toBeTruthy();
  });

  it('runs a test file as a check run, and offers no run for an api file', async () => {
    const { runs, records, user, open } = await setup();
    await open('users.api.yaml');
    await findCodeView(api);
    expect(screen.queryByRole('button', { name: 'Run file' })).toBeNull();
    await open('crud.test.yaml');
    await findCodeView(crud);
    await user.click(screen.getByRole('button', { name: 'Run file' }));
    expect(runs).toEqual([[crud]]);
    expect(records).toEqual([false]);
  });

  it('moves through the file tree with the keyboard', async () => {
    const { user } = await setup();
    const items = within(screen.getByRole('tree', { name: 'Files' })).getAllByRole('treeitem');
    items[0]?.focus();
    await user.keyboard('{ArrowDown}{ArrowDown}{Enter}');
    expect(await findCodeView(api)).toBeTruthy();
  });
});
