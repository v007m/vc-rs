import { Code, ConnectError } from '@connectrpc/connect';
import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { MASKED } from '#lib/settings/port.ts';
import type { ApiDraft, CallInput, ClientPort, ExplainedApi, WrittenDefinition } from './port.ts';
import RequestPanel from './RequestPanel.svelte';
import { ClientSession } from './session.svelte.ts';

const WRITTEN: WrittenDefinition = {
  path: 'services/users/users.api.yaml',
  pointer: '/apis/1',
  digest: 'd1',
  node: {
    id: 'createUser',
    method: 'POST',
    path: '/users',
    headers: { 'X-Key': MASKED },
    body: { email: '${var.email}' },
    expect: { status: 201 },
  },
};

const EXPLAINED: ExplainedApi = {
  vars: [{ key: 'vars.email', layers: [{ value: 'a@example.com', source: 'profile dev' }] }],
  headers: [{ name: 'X-Trace', value: 't1', from: 'config headers' }],
  curl: '',
  notices: [],
};

function setup(overrides: Partial<ClientPort> = {}) {
  const sent = { calls: [] as CallInput[], edits: [] as { draft: ApiDraft; write: boolean }[] };
  const port: ClientPort = {
    apis: () =>
      Promise.resolve([
        { id: 'users#createUser', name: 'Create a user', path: WRITTEN.path, vars: ['email'] },
      ]),
    flows: () => Promise.resolve([]),
    call: (input) => {
      sent.calls.push(input);
      return Promise.resolve({ exit: 0, text: '201', notices: [], checks: [], saved: [] });
    },
    readDefinition: () => Promise.resolve(WRITTEN),
    explainApi: () => Promise.resolve(EXPLAINED),
    editFile: (draft, write) => {
      sent.edits.push({ draft, write });
      const diff = '-  method: POST\n+  method: PUT\n';
      return Promise.resolve({ diff, digest: 'd2', written: write, problems: 0 });
    },
    saveCallAsTest: () => Promise.reject(new Error('unused')),
    importHar: () => Promise.reject(new Error('unused')),
    ...overrides,
  };
  return { port, sent, user: userEvent.setup() };
}

async function rendered(overrides: Partial<ClientPort> = {}) {
  const s = setup(overrides);
  const client = new ClientSession(s.port);
  await client.load();
  await client.open('users#createUser');
  const said: string[] = [];
  render(RequestPanel, { client, onsaved: (what: string) => said.push(what) });
  return { ...s, client, said };
}

describe('RequestPanel', () => {
  it('sends the api as edited, with a masked header read-only', async () => {
    const { user, sent } = await rendered();
    await user.selectOptions(screen.getByRole('combobox', { name: 'Method' }), 'PUT');
    await user.clear(screen.getByRole('textbox', { name: 'Path' }));
    await user.type(screen.getByRole('textbox', { name: 'Path' }), '/people');
    expect(screen.getByRole('textbox', { name: 'Header name' }).hasAttribute('readonly')).toBe(
      true,
    );
    expect(screen.queryByRole('textbox', { name: 'X-Key value' })).toBeNull();
    const inherited = screen.getByRole('table', { name: 'Inherited headers' });
    expect(within(inherited).getByRole('rowheader', { name: 'X-Trace' })).toBeTruthy();
    await user.click(screen.getByRole('button', { name: 'Send' }));
    expect(sent.calls[0]?.draft).toEqual({
      path: WRITTEN.path,
      digest: 'd1',
      edits: [
        { pointer: '/apis/1/method', op: 'set', value: 'PUT' },
        { pointer: '/apis/1/path', op: 'set', value: '/people' },
      ],
    });
  });

  it('will not send a JSON body that does not parse', async () => {
    const { user, sent } = await rendered();
    const body = screen.getByRole('textbox', { name: 'JSON body' });
    await user.type(body, '{{');
    expect(screen.getByText('The body is not JSON. Fix it, or send it as text.')).toBeTruthy();
    expect(body.getAttribute('aria-invalid')).toBe('true');
    await user.click(screen.getByRole('button', { name: 'Send' }));
    expect(sent.calls).toEqual([]);
    expect(
      (screen.getByRole('button', { name: 'Save changes to api' }) as HTMLButtonElement).disabled,
    ).toBe(true);
  });

  it('adds a check for this request alone and lists the api’s own', async () => {
    const { user, sent } = await rendered();
    await user.click(screen.getByRole('tab', { name: 'Checks' }));
    expect(screen.getByRole('list', { name: 'Api checks' }).textContent).toContain('status: 201');
    await user.type(screen.getByRole('textbox', { name: 'Expected value' }), 'ok');
    await user.click(screen.getByRole('button', { name: 'Add check' }));
    expect(screen.getByText(/A status is a code such as 201/)).toBeTruthy();
    await user.selectOptions(screen.getByRole('combobox', { name: 'Check' }), 'timing');
    await user.selectOptions(screen.getByRole('combobox', { name: 'Matcher' }), 'lt');
    await user.clear(screen.getByRole('textbox', { name: 'Expected value' }));
    await user.type(screen.getByRole('textbox', { name: 'Expected value' }), '500ms');
    await user.click(screen.getByRole('button', { name: 'Add check' }));
    const added = screen.getByRole('list', { name: 'Added checks' });
    expect(added.textContent).toContain('timing total lt 500ms');
    await user.click(screen.getByRole('button', { name: 'Send' }));
    expect(sent.calls[0]?.draft?.edits).toEqual([
      { pointer: '/apis/1/expect/timing/total', op: 'set', value: { lt: '500ms' } },
    ]);
  });

  it('reviews the edits as a diff, then saves them', async () => {
    const { user, sent, said } = await rendered();
    expect(
      (screen.getByRole('button', { name: 'Save changes to api' }) as HTMLButtonElement).disabled,
    ).toBe(true);
    await user.selectOptions(screen.getByRole('combobox', { name: 'Method' }), 'PUT');
    await user.click(screen.getByRole('button', { name: 'Save changes to api' }));
    const dialog = screen.getByRole('dialog', { name: 'Save changes to users#createUser' });
    const diff = await within(dialog).findByLabelText(`Changes to ${WRITTEN.path}`);
    expect(diff.textContent).toContain('+  method: PUT');
    await user.click(within(dialog).getByRole('button', { name: 'Save' }));
    expect(sent.edits.map((e) => e.write)).toEqual([false, true]);
    expect(sent.edits[1]?.draft.edits).toEqual([
      { pointer: '/apis/1/method', op: 'set', value: 'PUT' },
    ]);
    expect(said).toEqual(['Saved users#createUser.']);
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
    await user.selectOptions(screen.getByRole('combobox', { name: 'Method' }), 'PUT');
    await user.click(screen.getByRole('button', { name: 'Save changes to api' }));
    const dialog = screen.getByRole('dialog');
    await within(dialog).findByLabelText(`Changes to ${WRITTEN.path}`);
    await user.click(within(dialog).getByRole('button', { name: 'Save' }));
    const banner = within(dialog).getByRole('alert');
    expect(banner.textContent).toContain('changed on disk since you opened it');
    await user.click(within(banner).getByRole('button', { name: 'Reload from disk' }));
    expect(reads).toBe(2);
    expect(within(dialog).queryByRole('alert')).toBeNull();
  });

  it('shows the vars the project sets and the curl command when there is one', async () => {
    const { user } = await rendered({
      explainApi: () => Promise.resolve({ ...EXPLAINED, curl: "curl -X POST 'http://x/users'" }),
    });
    await user.click(screen.getByRole('tab', { name: 'Vars' }));
    const layers = screen.getByRole('table', { name: 'Vars from the project' });
    expect(layers.textContent).toContain('profile dev');
    await user.click(screen.getByRole('button', { name: 'Copy as cURL' }));
    const dialog = screen.getByRole('dialog', { name: 'Copy as cURL' });
    expect(within(dialog).getByLabelText('cURL command').textContent).toContain('curl -X POST');
    expect(within(dialog).getByText('$TOKEN')).toBeTruthy();
  });
});
