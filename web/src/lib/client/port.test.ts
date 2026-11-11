import { describe, expect, it } from 'vitest';
import { DefinitionKind, EditOp } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { enginePort } from './port.ts';

describe('enginePort', () => {
  it('lists apis with their vars and tests with steps as flows', async () => {
    const port = enginePort(
      fakeEngine({
        listDefinitions: ({ kind }) => ({
          definitions:
            kind === DefinitionKind.APIS
              ? [{ id: 'users#createUser', path: 'u.api.yaml', vars: ['email'] }]
              : [
                  { id: 'users.crud#createsOne', name: 'Creates', path: 'c.test.yaml' },
                  { id: 'users.flow#replays', name: 'Replays', path: 'f.test.yaml', steps: 3 },
                ],
        }),
      }),
    );
    expect(await port.apis()).toEqual([
      { id: 'users#createUser', name: 'users#createUser', path: 'u.api.yaml', vars: ['email'] },
    ]);
    expect(await port.flows()).toEqual([{ id: 'users.flow#replays', name: 'Replays', steps: 3 }]);
  });

  it('sends a call, saves it as a test and previews a HAR import', async () => {
    const seen: unknown[] = [];
    const port = enginePort(
      fakeEngine({
        call: ({ api, vars, keep }) => {
          seen.push({ api, vars, keep });
          return { exit: 0, text: '200 OK', responseJson: '{"status":200}' };
        },
        saveCallAsTest: ({ id, responseJson, fields }) => {
          seen.push({ id, responseJson, fields });
          return { id, path: 'c.test.yaml', line: 4, titles: [`Add ${id}`] };
        },
        import: ({ kind, flow, write }) => {
          seen.push({ kind, flow, write });
          return { files: [{ path: 'n.test.yaml', after: 'id: n\n' }], titles: ['filtered 1'] };
        },
      }),
    );
    const out = await port.call({ api: 'users#listUsers', vars: ['a=1'], keep: true });
    expect(out).toEqual({
      exit: 0,
      text: '200 OK',
      notices: [],
      checks: [],
      saved: [],
      responseJson: '{"status":200}',
    });
    const saved = await port.saveCallAsTest({
      testId: 'users.crud#listsOne',
      api: 'users#listUsers',
      vars: [],
      responseJson: '{"status":200}',
      fields: ['body.id'],
    });
    expect(saved).toEqual({
      id: 'users.crud#listsOne',
      path: 'c.test.yaml',
      titles: ['Add users.crud#listsOne'],
    });
    const changes = await port.importHar({ source: '{}', flow: 'n#replays', write: false });
    expect(changes).toEqual({
      files: [{ path: 'n.test.yaml', before: '', after: 'id: n\n' }],
      titles: ['filtered 1'],
    });
    expect(seen).toEqual([
      { api: 'users#listUsers', vars: ['a=1'], keep: true },
      { id: 'users.crud#listsOne', responseJson: '{"status":200}', fields: ['body.id'] },
      { kind: 'har', flow: 'n#replays', write: false },
    ]);
  });

  it('reads and explains an api, sends a call with its draft, then saves it as EditFile', async () => {
    const seen: unknown[] = [];
    const port = enginePort(
      fakeEngine({
        readDefinition: ({ id }) => {
          seen.push({ id });
          return {
            path: 'services/users/users.api.yaml',
            pointer: '/apis/1',
            digest: 'd1',
            json: '{"id":"get","path":"/users/${var.userId}"}',
          };
        },
        explainApi: ({ api, vars }) => {
          seen.push({ api, vars });
          return {
            vars: [{ key: 'userId', layers: [{ value: '1', source: 'api' }] }],
            headers: [{ name: 'X-Key', value: '••••', from: 'users.api.yaml' }],
            auth: { id: 'shopOauth', type: 'oauth2', tokenUrl: '${var.authUrl}', secrets: ['ID'] },
            curl: 'curl http://x/users/1',
          };
        },
        call: ({ draft }) => {
          seen.push(draft?.edits.map(({ pointer, op, valueJson }) => ({ pointer, op, valueJson })));
          return { exit: 0, text: '200 OK' };
        },
        editFile: ({ path, digest, write }) => {
          seen.push({ path, digest, write });
          return { diff: '-a\n+b\n', digest: 'd2', written: true, diagnostics: [] };
        },
      }),
    );
    const written = await port.readDefinition('users#get');
    expect(written).toEqual({
      path: 'services/users/users.api.yaml',
      pointer: '/apis/1',
      digest: 'd1',
      node: { id: 'get', path: '/users/${var.userId}' },
    });
    expect(await port.explainApi('users#get', ['userId=2'])).toEqual({
      vars: [{ key: 'userId', layers: [{ value: '1', source: 'api' }] }],
      headers: [{ name: 'X-Key', value: '••••', from: 'users.api.yaml' }],
      auth: { id: 'shopOauth', type: 'oauth2', tokenUrl: '${var.authUrl}', secrets: ['ID'] },
      curl: 'curl http://x/users/1',
      notices: [],
    });
    const draft = {
      path: written.path,
      digest: 'd1',
      edits: [{ pointer: '/apis/1/method', op: 'set', value: 'POST' } as const],
    };
    await port.call({ api: 'users#get', vars: [], keep: false, draft });
    expect(await port.editFile(draft, true)).toEqual({
      diff: '-a\n+b\n',
      digest: 'd2',
      written: true,
      problems: 0,
    });
    expect(seen).toEqual([
      { id: 'users#get' },
      { api: 'users#get', vars: ['userId=2'] },
      [{ pointer: '/apis/1/method', op: EditOp.SET, valueJson: '"POST"' }],
      { path: 'services/users/users.api.yaml', digest: 'd1', write: true },
    ]);
  });
});
