import { Code, ConnectError } from '@connectrpc/connect';
import { describe, expect, it } from 'vitest';
import type { ClientPort, SaveCallInput } from './port.ts';
import { ClientSession } from './session.svelte.ts';

const apis = [
  { id: 'users#listUsers', name: 'List users', path: 'services/users/users.api.yaml', vars: [] },
  {
    id: 'users#createUser',
    name: 'Create a user',
    path: 'services/users/users.api.yaml',
    vars: ['name', 'email'],
  },
];

const created = JSON.stringify({ status: 201, headers: {}, body: { id: 'u_1', tags: [] } });

function fakePort(overrides: Partial<ClientPort> = {}): ClientPort {
  return {
    apis: () => Promise.resolve(apis),
    flows: () => Promise.resolve([]),
    call: () =>
      Promise.resolve({ exit: 0, text: '201 Created', notices: [], responseJson: created }),
    saveCallAsTest: () => Promise.resolve({ id: '', path: '', titles: [] }),
    importHar: () => Promise.resolve({ files: [], titles: [] }),
    ...overrides,
  };
}

async function opened(port: ClientPort, id = 'users#createUser'): Promise<ClientSession> {
  const session = new ClientSession(port);
  await session.load();
  session.open(id);
  return session;
}

describe('ClientSession', () => {
  it('opens an api with a row per declared var', async () => {
    const session = await opened(fakePort());
    expect(session.vars).toEqual([
      { name: 'name', value: '' },
      { name: 'email', value: '' },
    ]);
  });

  it('sends only the vars that have a value and reads the response', async () => {
    const sent: unknown[] = [];
    const session = await opened(
      fakePort({
        call: (input) => {
          sent.push(input);
          return Promise.resolve({ exit: 0, text: '201', notices: [], responseJson: created });
        },
      }),
    );
    session.vars[1] = { name: 'email', value: 'a@x' };
    session.addVar();
    session.vars[2] = { name: ' extra ', value: '1' };
    await session.send();
    expect(sent).toEqual([
      { api: 'users#createUser', vars: ['email=a@x', 'extra=1'], keep: false },
    ]);
    expect(session.result?.response?.status).toBe(201);
    expect(session.result?.fields).toEqual(['body.id']);
  });

  it('saves the last call with its response, and reports what was added', async () => {
    const saved: SaveCallInput[] = [];
    const session = await opened(
      fakePort({
        saveCallAsTest: (input) => {
          saved.push(input);
          return Promise.resolve({
            id: input.testId,
            path: 'c.test.yaml',
            titles: ['Add users.crud#createsOne'],
          });
        },
      }),
    );
    await session.send();
    expect(await session.saveAsTest('users.crud#createsOne', ['body.id'])).toBe(
      'Add users.crud#createsOne',
    );
    expect(saved[0]).toMatchObject({
      api: 'users#createUser',
      responseJson: created,
      fields: ['body.id'],
    });
  });

  it('shows the engine error and keeps the last result', async () => {
    const session = await opened(
      fakePort({ call: () => Promise.reject(new ConnectError('unknown api', Code.NotFound)) }),
    );
    await session.send();
    expect(session.error).toBe('unknown api');
    expect(session.result).toBeUndefined();
    expect(session.sending).toBe(false);
  });
});
