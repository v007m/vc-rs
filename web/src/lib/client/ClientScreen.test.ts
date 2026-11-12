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
  titles: ['Add users.recorded#replaysSession', 'read 3 entries · filtered 1 static → 2 requests'],
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
        exchange: {
          method: 'POST',
          url: 'http://localhost:7421/users',
          status: 201,
          ms: 9,
          timing: { dnsMs: 1, connectMs: 2, firstByteMs: 5, downloadMs: 1, totalMs: 9 },
          requestHeaders: [],
          responseHeaders: [['x-id', 'u_1']],
          truncated: false,
        },
        checks: [
          {
            label: 'status',
            passed: true,
            expected: '201',
            actual: '201',
            location: 'services/users/users.api.yaml:20',
          },
        ],
        saved: [{ name: 'userId', from: '${response.body.id}', value: '"u_1"' }],
      });
    },
    readDefinition: (id) =>
      Promise.resolve({
        path: 'u.api.yaml',
        pointer: '/apis/0',
        digest: 'd1',
        node: { id: id.split('#')[1], method: 'POST', path: '/users' },
      }),
    explainApi: () => Promise.resolve({ vars: [], headers: [], curl: '', notices: [] }),
    editFile: () => Promise.reject(new Error('unused')),
    saveCallAsTest: (input) => {
      sent.saves.push(input);
      return Promise.resolve({
        id: input.testId,
        path: 'u.test.yaml',
        titles: [`Add ${input.testId}`],
      });
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
    await user.click(screen.getByRole('tab', { name: 'Vars' }));
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

  it('shows the timing waterfall, the checks that ran and the values saved', async () => {
    const { user } = await rendered();
    await user.click(screen.getByRole('treeitem', { name: 'Create a user' }));
    await user.click(screen.getByRole('button', { name: 'Send' }));
    const response = screen.getByRole('region', { name: 'Response' });
    await user.click(within(response).getByRole('tab', { name: 'Timing' }));
    const waterfall = within(response).getByRole('table', { name: 'Timing waterfall' });
    const phases = within(waterfall)
      .getAllByRole('rowheader')
      .map((th) => th.textContent);
    expect(phases).toEqual(['dns', 'connect', 'firstByte', 'download', 'total']);
    await user.click(within(response).getByRole('tab', { name: 'Checks' }));
    const checks = within(response).getByRole('list', { name: 'Checks' });
    expect(checks.textContent?.replace(/\s+/g, ' ')).toContain('status held');
    await user.click(within(response).getByRole('tab', { name: 'Saved' }));
    const saved = within(response).getByRole('table', { name: 'Saved values' });
    expect(within(saved).getByRole('rowheader', { name: 'userId' })).toBeTruthy();
    expect(saved.textContent).toContain('${response.body.id}');
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
    expect(within(dialog).getByText(/filtered 1 static/)).toBeTruthy();
    await user.click(within(dialog).getByRole('button', { name: 'Write files' }));
    expect(sent.imports.map((i) => i.write)).toEqual([false, true]);
    expect(screen.getByRole('status').textContent).toContain('Add users.recorded#replaysSession');
  });

  it('lists flow tests in Flow mode and runs one', async () => {
    const { user, runs } = await rendered();
    await user.click(
      within(screen.getByRole('group', { name: 'Mode' })).getByRole('button', { name: 'Flow' }),
    );
    const flows = screen.getByRole('list', { name: 'Flow tests' });
    await user.click(
      within(flows).getByRole('button', { name: 'Run users.recorded#replaysSession' }),
    );
    expect(runs).toEqual([['users.recorded#replaysSession']]);
  });

  it('edits a flow test below the list, and runs it only as saved', async () => {
    const s = setup();
    s.port.readDefinition = () =>
      Promise.resolve({
        path: 'services/users/recorded.test.yaml',
        pointer: '/tests/0',
        digest: 'd1',
        node: { id: 'replaysSession', steps: [{ call: 'users#listUsers' }] },
      });
    await s.client.load();
    render(ClientScreen, { client: s.client, port: s.port, session: s.session });
    await s.user.click(
      within(screen.getByRole('group', { name: 'Mode' })).getByRole('button', { name: 'Flow' }),
    );
    const flows = screen.getByRole('list', { name: 'Flow tests' });
    await s.user.click(
      within(flows).getByRole('button', { name: 'Edit users.recorded#replaysSession' }),
    );
    const api = await screen.findByRole('combobox', { name: 'Stage 1 api' });
    await s.user.selectOptions(api, 'users#createUser');
    const run = within(flows).getByRole('button', { name: 'Run users.recorded#replaysSession' });
    expect(run).toHaveProperty('disabled', true);
    await s.user.click(screen.getByRole('button', { name: 'Discard' }));
    expect(run).toHaveProperty('disabled', false);
  });
});
