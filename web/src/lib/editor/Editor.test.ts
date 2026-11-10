import { Code, ConnectError } from '@connectrpc/connect';
import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { Severity } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { RunSession } from '#lib/run/session.svelte.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import Editor from './Editor.svelte';
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
      edits: [{ file: crud, start: 15, end: 22, newText: 'getsUser' }],
    },
  ],
};

async function setup(conflict = { next: false }) {
  const disk = new Map([
    [crud, 'name: crud\nid: GetUser\n'],
    [api, 'name: users\n'],
  ]);
  const runs: string[][] = [];
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
    },
  });
  const editor = new EditorSession(engine, 0);
  await editor.loadFiles();
  render(Editor, { editor, session: new RunSession(engine) });
  const user = userEvent.setup();
  const open = async (name: string) => {
    await user.click(within(screen.getByRole('tree', { name: 'Files' })).getByText(name));
  };
  return { disk, runs, user, open };
}

describe('Editor', () => {
  it('opens a file from the tree, edits it and saves with Ctrl+S', async () => {
    const { disk, user, open } = await setup();
    await open('users.api.yaml');
    const text = await screen.findByRole('textbox', { name: api });
    await user.type(text, '# done');
    expect(screen.getByText('Unsaved changes')).toBeTruthy();
    await user.keyboard('{Control>}s{/Control}');
    expect(await screen.findByText('Saved')).toBeTruthy();
    expect(disk.get(api)).toBe('name: users\n# done');
  });

  it('blocks saving while the unsaved text has errors', async () => {
    const { disk, user, open } = await setup();
    await open('crud.test.yaml');
    await user.type(await screen.findByRole('textbox', { name: crud }), '# more');
    expect(await screen.findByText('Fix 1 error to save')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Save' })).toHaveProperty('disabled', true);
    await user.keyboard('{Control>}s{/Control}');
    expect(disk.get(crud)).toBe('name: crud\nid: GetUser\n');
    const problems = screen.getByRole('region', { name: 'Problems' });
    await user.click(within(problems).getByRole('button', { name: 'Rename to getsUser' }));
    expect(await screen.findByText('Unsaved changes')).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByText('Saved')).toBeTruthy();
    expect(disk.get(crud)).toBe('name: crud\nid: getsUser\n# more');
  });

  it('asks before dropping unsaved changes for another file', async () => {
    const { user, open } = await setup();
    await open('crud.test.yaml');
    await user.type(await screen.findByRole('textbox', { name: crud }), 'x');
    await open('users.api.yaml');
    await user.click(screen.getByRole('button', { name: 'Keep editing' }));
    expect(screen.getByRole('textbox', { name: crud })).toBeTruthy();
    await open('users.api.yaml');
    await user.click(screen.getByRole('button', { name: 'Discard and open' }));
    expect(await screen.findByRole('textbox', { name: api })).toBeTruthy();
  });

  it('offers reload or keep-mine when the file changed on disk', async () => {
    const { disk, user, open } = await setup({ next: true });
    await open('users.api.yaml');
    await user.type(await screen.findByRole('textbox', { name: api }), '!');
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
    const text = (await screen.findByRole('textbox', { name: crud })) as HTMLTextAreaElement;
    const problems = screen.getByRole('region', { name: 'Problems' });
    expect(within(problems).getByText('1 error')).toBeTruthy();
    await user.click(within(problems).getByRole('button', { name: /test IDs are camelCase/ }));
    expect(document.activeElement).toBe(text);
    expect(text.value.slice(text.selectionStart, text.selectionEnd)).toBe('GetUser');
    await user.click(within(problems).getByRole('button', { name: 'Rename to getsUser' }));
    expect(text.value).toBe('name: crud\nid: getsUser\n');
  });

  it('shows the diff and reverts it', async () => {
    const { user, open } = await setup();
    await open('users.api.yaml');
    await user.type(await screen.findByRole('textbox', { name: api }), 'tags: one');
    await user.click(screen.getByRole('button', { name: /^Diff/ }));
    const diff = screen.getByRole('region', { name: 'Changes since the last save' });
    expect(within(diff).getByText('tags: one')).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Revert' }));
    expect(within(diff).getByText('No changes since the last save.')).toBeTruthy();
  });

  it('runs a test file, and offers no run for an api file', async () => {
    const { runs, user, open } = await setup();
    await open('users.api.yaml');
    await screen.findByRole('textbox', { name: api });
    expect(screen.queryByRole('button', { name: 'Run file' })).toBeNull();
    await open('crud.test.yaml');
    await screen.findByRole('textbox', { name: crud });
    await user.click(screen.getByRole('button', { name: 'Run file' }));
    expect(runs).toEqual([[crud]]);
  });

  it('moves through the file tree with the keyboard', async () => {
    const { user } = await setup();
    const items = within(screen.getByRole('tree', { name: 'Files' })).getAllByRole('treeitem');
    items[0]?.focus();
    await user.keyboard('{ArrowDown}{ArrowDown}{Enter}');
    expect(await screen.findByRole('textbox', { name: api })).toBeTruthy();
  });
});
