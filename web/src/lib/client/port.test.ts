import { describe, expect, it } from 'vitest';
import { DefinitionKind } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
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
    expect(out).toEqual({ exit: 0, text: '200 OK', notices: [], responseJson: '{"status":200}' });
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
});
