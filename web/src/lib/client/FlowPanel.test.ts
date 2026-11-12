import { Code, ConnectError } from '@connectrpc/connect';
import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { MASKED } from '#lib/settings/port.ts';
import { FlowEditor } from './flow.svelte.ts';
import FlowPanel from './FlowPanel.svelte';
import type { ApiDraft, ApiInfo, ClientPort, WrittenDefinition } from './port.ts';

const WRITTEN: WrittenDefinition = {
  path: 'services/users/flow.test.yaml',
  pointer: '/tests/0',
  digest: 'd1',
  node: {
    id: 'createsThenGets',
    steps: [
      {
        call: 'users#create',
        vars: { email: 'a@example.com' },
        save: { userId: '${response.body.id}' },
      },
      {
        call: 'users#get',
        headers: { 'X-Key': MASKED },
        poll: { until: { body: { match: { state: 'done' } } }, every: '1s', timeout: '10s' },
      },
    ],
  },
};

const APIS: ApiInfo[] = ['users#create', 'users#get', 'users#delete'].map((id) => ({
  id,
  name: id,
  path: 'services/users/users.api.yaml',
  vars: [],
}));

type Port = Pick<ClientPort, 'readDefinition' | 'editFile'>;

async function rendered(overrides: Partial<Port> = {}) {
  const sent: { draft: ApiDraft; write: boolean }[] = [];
  const port: Port = {
    readDefinition: () => Promise.resolve(WRITTEN),
    editFile: (draft, write) => {
      sent.push({ draft, write });
      return Promise.resolve({
        diff: '+    repeat: 3\n',
        digest: 'd2',
        written: write,
        problems: 0,
      });
    },
    ...overrides,
  };
  const flow = new FlowEditor(port, 'users.flow#createsThenGets');
  await flow.load();
  const said: string[] = [];
  render(FlowPanel, { flow, apis: APIS, onsaved: (what: string) => said.push(what) });
  return { flow, sent, said, user: userEvent.setup() };
}

describe('FlowPanel', () => {
  it('shows each stage with its api, the vars it uses and saves, and a loop it cannot show locked', async () => {
    await rendered();
    const stages = screen.getByRole('table', { name: 'Stages' });
    const [, made, polled] = within(stages).getAllByRole('row');
    if (!made || !polled) throw new Error('no rows');
    expect(
      (within(made).getByRole('combobox', { name: 'Stage 1 api' }) as HTMLSelectElement).value,
    ).toBe('users#create');
    expect(made.textContent).toContain('email');
    expect(made.textContent).toContain('userId');
    expect(polled.textContent).toContain('As written in the file');
    expect(within(polled).queryByRole('combobox', { name: 'Stage 2 runs' })).toBeNull();
    expect(stages.textContent).not.toContain(MASKED);
    expect(screen.getByRole('button', { name: 'Save changes to flow' })).toHaveProperty(
      'disabled',
      true,
    );
  });

  it('reviews and saves a stage that now repeats, a removed stage and an added one', async () => {
    const { user, sent, said } = await rendered();
    await user.selectOptions(screen.getByRole('combobox', { name: 'Stage 1 runs' }), 'repeat');
    await user.clear(screen.getByRole('textbox', { name: 'Stage 1 times' }));
    await user.type(screen.getByRole('textbox', { name: 'Stage 1 times' }), '3');
    await user.click(screen.getByRole('button', { name: 'Remove stage 2' }));
    await user.selectOptions(screen.getByRole('combobox', { name: 'Api to add' }), 'users#delete');
    await user.click(screen.getByRole('button', { name: 'Add stage' }));
    expect(screen.getByText('Unsaved: a run runs the flow as saved.')).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Save changes to flow' }));
    const dialog = screen.getByRole('dialog', {
      name: 'Save changes to users.flow#createsThenGets',
    });
    const diff = await within(dialog).findByLabelText(`Changes to ${WRITTEN.path}`);
    expect(diff.textContent).toContain('repeat: 3');
    await user.click(within(dialog).getByRole('button', { name: 'Save' }));
    expect(sent.map((e) => e.write)).toEqual([false, true]);
    expect(sent[1]?.draft).toEqual({
      path: WRITTEN.path,
      digest: 'd1',
      edits: [
        { pointer: '/tests/0/steps/1', op: 'remove' },
        { pointer: '/tests/0/steps/-', op: 'insert', value: { call: 'users#delete' } },
        { pointer: '/tests/0/steps/0/repeat', op: 'set', value: 3 },
      ],
    });
    expect(said).toEqual(['Saved users.flow#createsThenGets.']);
  });

  it('moves a stage up and down, says where it went, keeps focus on it and saves the move', async () => {
    const { user, flow } = await rendered();
    const button = (name: string) => screen.getByRole('button', { name });
    expect(button('Move up stage 1')).toHaveProperty('disabled', true);
    expect(button('Move down stage 2')).toHaveProperty('disabled', true);
    await user.click(button('Move down stage 1'));
    expect(screen.getByRole('status').textContent).toBe('Stage 1 moved down to place 2.');
    expect(document.activeElement).toBe(button('Move up stage 2'));
    expect(flow.stages.map((stage) => stage.target)).toEqual(['users#get', 'users#create']);
    expect(flow.edits).toEqual([{ pointer: '/tests/0/steps/1', op: 'move', value: 0 }]);
    await user.click(button('Move up stage 2'));
    expect(screen.getByRole('status').textContent).toBe('Stage 2 moved up to place 1.');
    expect(document.activeElement).toBe(button('Move down stage 1'));
    expect(flow.edits).toEqual([]);
    expect(flow.dirty).toBe(false);
  });

  it('keeps the moved stage on the button the keyboard pressed while it is not at an end', async () => {
    const { user, flow } = await rendered();
    flow.addStage('users#delete');
    await user.click(await screen.findByRole('button', { name: 'Move up stage 3' }));
    expect(document.activeElement).toBe(screen.getByRole('button', { name: 'Move up stage 2' }));
    expect(flow.edits).toEqual([
      { pointer: '/tests/0/steps/-', op: 'insert', value: { call: 'users#delete' } },
      { pointer: '/tests/0/steps/2', op: 'move', value: 1 },
    ]);
  });

  it('says why the flow cannot be saved', async () => {
    const { user } = await rendered();
    await user.selectOptions(screen.getByRole('combobox', { name: 'Stage 1 runs' }), 'poll');
    await user.clear(screen.getByRole('textbox', { name: 'Stage 1 until status' }));
    await user.type(screen.getByRole('textbox', { name: 'Stage 1 until status' }), 'ok');
    expect(screen.getByText('Stage 1: poll until needs a status such as 200.')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Save changes to flow' })).toHaveProperty(
      'disabled',
      true,
    );
    await user.click(screen.getByRole('button', { name: 'Discard' }));
    expect(screen.queryByText(/poll until needs/)).toBeNull();
  });

  it('says the file changed on disk when a save races it, and reloads', async () => {
    let reads = 0;
    const { user } = await rendered({
      readDefinition: () => Promise.resolve({ ...WRITTEN, digest: `d${++reads}` }),
      editFile: (_draft, write) =>
        write
          ? Promise.reject(new ConnectError('changed', Code.Aborted))
          : Promise.resolve({ diff: '+x', digest: 'd2', written: false, problems: 0 }),
    });
    await user.click(screen.getByRole('button', { name: 'Remove stage 2' }));
    await user.click(screen.getByRole('button', { name: 'Save changes to flow' }));
    const dialog = screen.getByRole('dialog');
    await within(dialog).findByLabelText(`Changes to ${WRITTEN.path}`);
    await user.click(within(dialog).getByRole('button', { name: 'Save' }));
    const banner = within(dialog).getByRole('alert');
    await user.click(within(banner).getByRole('button', { name: 'Reload from disk' }));
    expect(reads).toBe(2);
    expect(within(screen.getByRole('table', { name: 'Stages' })).getAllByRole('row')).toHaveLength(
      3,
    );
  });
});
