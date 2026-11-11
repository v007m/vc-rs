import { Code, ConnectError } from '@connectrpc/connect';
import { describe, expect, it } from 'vitest';
import type { ApiDraft, CallInput, ClientPort, SaveCallInput } from './port.ts';
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

const WRITTEN = {
  path: 'services/users/users.api.yaml',
  pointer: '/apis/1',
  digest: 'd1',
  node: { id: 'createUser', method: 'POST', path: '/users', expect: { status: 201 } },
};

function fakePort(overrides: Partial<ClientPort> = {}): ClientPort {
  return {
    apis: () => Promise.resolve(apis),
    flows: () => Promise.resolve([]),
    call: () =>
      Promise.resolve({
        exit: 0,
        text: '201 Created',
        notices: [],
        checks: [],
        saved: [],
        responseJson: created,
      }),
    saveCallAsTest: () => Promise.resolve({ id: '', path: '', titles: [] }),
    importHar: () => Promise.resolve({ files: [], titles: [] }),
    readDefinition: () => Promise.resolve(WRITTEN),
    explainApi: () => Promise.resolve({ vars: [], headers: [], curl: '', notices: [] }),
    editFile: () => Promise.resolve({ diff: '', digest: 'd2', written: true, problems: 0 }),
    ...overrides,
  };
}

async function opened(port: ClientPort, id = 'users#createUser'): Promise<ClientSession> {
  const session = new ClientSession(port);
  await session.load();
  await session.open(id);
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
          return Promise.resolve({
            exit: 0,
            text: '201',
            notices: [],
            checks: [],
            saved: [],
            responseJson: created,
          });
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

  it('sends the request as edited, with the added checks, and saves only the edits', async () => {
    const sent: CallInput[] = [];
    const saves: [ApiDraft, boolean][] = [];
    const session = await opened(
      fakePort({
        call: (input) => {
          sent.push(input);
          return Promise.resolve({ exit: 0, text: '', notices: [], checks: [], saved: [] });
        },
        editFile: (draft, write) => {
          saves.push([draft, write]);
          return Promise.resolve({ diff: '+x', digest: 'd2', written: write, problems: 0 });
        },
      }),
    );
    const request = session.request;
    if (!request) throw new Error('open made no request');
    await session.send();
    expect(sent[0]?.draft).toBeUndefined();
    request.form.path = '/people';
    expect(request.addCheck({ kind: 'status', path: '', op: 'equals', value: '202' })).toBe(
      undefined,
    );
    await session.send();
    const path = { pointer: '/apis/1/path', op: 'set', value: '/people' };
    expect(sent[1]?.draft).toEqual({
      path: WRITTEN.path,
      digest: 'd1',
      edits: [
        path,
        { pointer: '/apis/1/expect/status', op: 'remove' },
        { pointer: '/apis/1/expect/status', op: 'set', value: 202 },
      ],
    });
    await request.review();
    expect(request.diff).toBe('+x');
    await request.save();
    expect(saves.map(([draft, write]) => [draft.edits, write])).toEqual([
      [[path], false],
      [[path], true],
    ]);
    expect(request.checks).toHaveLength(1);
  });

  it('marks the request stale when its file changed, and sends no body that is not JSON', async () => {
    let calls = 0;
    const session = await opened(
      fakePort({
        call: () => {
          calls += 1;
          return Promise.reject(new ConnectError('changed since it was opened', Code.Aborted));
        },
      }),
    );
    const request = session.request;
    if (!request) throw new Error('open made no request');
    request.form.path = '/people';
    await session.send();
    expect([request.state, calls]).toEqual(['conflict', 1]);
    request.form.bodyMode = 'json';
    request.form.body = '{';
    await session.send();
    expect(calls).toBe(1);
  });
});
