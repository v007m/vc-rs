import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { RunSession } from '#lib/run/session.svelte.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import ClientScreen from './ClientScreen.svelte';
import type { CallInput, ClientPort, HarInput, SaveCallInput } from './port.ts';
import { ClientSession } from './session.svelte.ts';

const body = { id: 'u_1', email: 'grace@example.com' };
const changes = {
  files: [{ path: 'services/users/recorded.test.yaml', before: '', after: 'id: users.recorded\n' }],
  titles: ['Add users.recorded#replaysSession'],
  notices: ['read 3 entries · filtered 1 static → 2 requests'],
};

function setup() {
  const sent = {
    calls: [] as CallInput[],
    saves: [] as SaveCallInput[],
    imports: [] as HarInput[],
  };
  const runs: string[][] = [];
  const port: ClientPort = {
    apis: () =>
      Promise.resolve([
        { id: 'users#listUsers', name: 'List users', path: 'u.api.yaml', vars: [] },
        { id: 'users#createUser', name: 'Create a user', path: 'u.api.yaml', vars: ['email'] },
      ]),
    flows: () =>
      Promise.resolve([{ id: 'users.recorded#replaysSession', name: 'Replays', steps: 2 }]),
    call: (input) => {
      sent.calls.push(input);
      const responseJson = JSON.stringify({ status: 201, headers: { 'x-id': 'u_1' }, body });
      return Promise.resolve({
        exit: 0,
        text: '201 Created · 9 ms · api expect passed',
        notices: [],
        responseJson,
      });
    },
    saveCallAsTest: (input) => {
      sent.saves.push(input);
      return Promise.resolve({ files: [], titles: [`Add ${input.testId}`], notices: [] });
    },
    importHar: (input) => {
      sent.imports.push(input);
      return Promise.resolve(changes);
    },
  };
  const engine = fakeEngine({
    // eslint-disable-next-line require-yield -- the run ends without events
    async *run(request) {
      runs.push(request.selection?.ids ?? []);
    },
  });
  const client = new ClientSession(port);
  return { client, port, session: new RunSession(engine), sent, runs, user: userEvent.setup() };
}

async function rendered() {
  const s = setup();
  await s.client.load();
  render(ClientScreen, { client: s.client, port: s.port, session: s.session });
  return s;
}

describe('ClientScreen', () => {
  it('calls an api with a var and shows the status, body and headers', async () => {
    const { user, sent } = await rendered();
    await user.click(screen.getByRole('treeitem', { name: 'Create a user' }));
    await user.type(screen.getByRole('textbox', { name: 'email' }), 'grace@example.com');
    await user.click(screen.getByRole('button', { name: 'Send' }));
    expect(sent.calls).toEqual([
      { api: 'users#createUser', vars: ['email=grace@example.com'], keep: false },
    ]);
    const response = screen.getByRole('region', { name: 'Response' });
    expect(within(response).getByText('201 Created · 9 ms · api expect passed')).toBeTruthy();
    expect(within(response).getByText(/"u_1"/)).toBeTruthy();
    await user.click(within(response).getByRole('tab', { name: 'Headers' }));
    expect(within(response).getByText('x-id')).toBeTruthy();
  });

  it('saves the call as a test expecting a chosen body field', async () => {
    const { user, sent } = await rendered();
    await user.click(screen.getByRole('treeitem', { name: 'List users' }));
    await user.keyboard('{Control>}{Enter}{/Control}');
    await user.click(screen.getByRole('button', { name: 'Save as test' }));
    const dialog = screen.getByRole('dialog', { name: 'Save as test' });
    await user.type(
      within(dialog).getByRole('textbox', { name: 'Test ID' }),
      'users.crud#listsOne',
    );
    await user.click(within(dialog).getByRole('checkbox', { name: 'body.email' }));
    await user.click(within(dialog).getByRole('button', { name: 'Save' }));
    expect(sent.saves[0]).toMatchObject({ testId: 'users.crud#listsOne', fields: ['body.email'] });
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.getByRole('status').textContent).toBe('Add users.crud#listsOne');
  });

  it('previews a HAR import, shows a file diff and writes it', async () => {
    const { user, sent } = await rendered();
    await user.click(screen.getByRole('button', { name: 'Import HAR' }));
    const dialog = screen.getByRole('dialog', { name: 'Import a HAR file' });
    await user.upload(within(dialog).getByLabelText('HAR file'), new File(['{}'], 's.har'));
    await user.type(
      within(dialog).getByRole('textbox', { name: 'Flow test' }),
      'users.recorded#replaysSession',
    );
    await user.click(within(dialog).getByRole('button', { name: 'Preview' }));
    const file = within(within(dialog).getByRole('list', { name: 'Changes' })).getByRole('button');
    await user.click(file);
    expect(
      within(dialog).getByRole('region', { name: /recorded\.test\.yaml/ }).textContent,
    ).toContain('Added: id: users.recorded');
    expect(true).toBeTruthy();
    await user.click(within(dialog).getByRole('button', { name: 'Write files' }));
    expect(sent.imports.map((i) => i.write)).toEqual([false, true]);
    expect(screen.getByRole('status').textContent).toBe('Add users.recorded#replaysSession');
  });

  it('lists flow tests in Flow mode and runs one', async () => {
    const { user, runs } = await rendered();
    await user.click(
      within(screen.getByRole('group', { name: 'Mode' })).getByRole('button', { name: 'Flow' }),
    );
    const flows = screen.getByRole('list', { name: 'Flow tests' });
    await user.click(within(flows).getByRole('button', { name: 'Run' }));
    expect(runs).toEqual([['users.recorded#replaysSession']]);
  });
});
