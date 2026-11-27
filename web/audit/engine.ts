import {
  create,
  fromBinary,
  toBinary,
  type DescMessage,
  type MessageInitShape,
  type MessageShape,
} from '@bufbuild/protobuf';
import type { Page } from '@playwright/test';
import * as E from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import * as R from '#lib/gen/vibranium/results/v1/results_pb.ts';
import * as F from './fixtures.ts';
import * as onboarding from './areas/onboarding.ts';
import * as schedules from './areas/schedules.ts';
import * as settings from './areas/settings.ts';
import * as usage from './areas/usage.ts';
import { compareRuns, diagnosticsOf, loadTicks, loadReport, testHistory, vcs } from './scenes.ts';

type Methods = (typeof E.EngineService)['method'];
type Answer<M extends Methods[keyof Methods]> = MessageInitShape<M['output']>;
/** One RPC's fake: the request in, one answer (unary) or the answers in order (a stream). */
type Fake<M extends Methods[keyof Methods]> = (
  request: MessageShape<M['input']>,
) => Answer<M> | readonly Answer<M>[];
export type Fakes = { [K in keyof Methods]?: Fake<Methods[K]> };

const callJson = JSON.stringify({
  status: 201,
  headers: { 'content-type': 'application/json', 'x-request-id': 'r_5521' },
  body: F.userBody,
});

const fakes: Fakes = {
  getProject: () => ({
    name: 'shop',
    profiles: ['dev', 'staging', 'prod'],
    defaultProfile: 'staging',
    vcVersion: '2.0.0-dev',
    auths: ['bearer'],
    configured: true,
  }),
  listDefinitions: ({ kind }) => ({
    definitions:
      kind === E.DefinitionKind.APIS
        ? F.apiDefinitions
        : kind === E.DefinitionKind.FILES
          ? F.fileDefinitions
          : F.testDefinitions,
  }),
  check: ({ paths, overlay }) => {
    const file = 'services/users/crud.test.yaml';
    if (!paths.includes(file) && !(file in overlay)) return { checked: 9, diagnostics: [] };
    return { checked: 1, diagnostics: diagnosticsOf(overlay[file] ?? F.crudYaml) };
  },
  listRuns: ({ kind }) => ({ runs: kind === 'adhoc' ? F.adhocRuns : F.runs }),
  getRun: ({ id }) => F.storedRun(id ?? 0n),
  readFile: ({ path }) => ({
    text: path === F.configPath ? F.configYaml : F.crudYaml,
    digest: 'd1',
  }),
  explainConfig: () => ({ values: F.configValues, notices: [] }),
  apiCoverage: () => ({ apis: F.coverage, notes: [] }),
  trends: () => ({ trends: F.trends }),
  apiTrends: () => ({ trends: F.apiTrends }),
  readDefinition: () => ({
    path: 'services/users/users.api.yaml',
    pointer: '/apis/1',
    digest: 'd1',
    json: JSON.stringify({ id: 'createsUser', method: 'POST', path: '/users', body: F.userBody }),
  }),
  explainApi: () => ({
    vars: [
      { key: 'vars.email', layers: [{ value: 'grace@example.com', source: 'profile staging' }] },
    ],
    headers: [{ name: 'Accept', value: 'application/json', from: 'config headers' }],
    curl: "curl -X POST 'https://staging.shop.dev/users' -H 'Accept: application/json'",
  }),
  call: () => ({
    exit: 0,
    text: '201 Created · 48 ms',
    responseJson: callJson,
    exchange: { ...F.exchange(201, 48), url: 'https://staging.shop.dev/users' },
    checks: [
      {
        kind: R.CheckKind.STATUS,
        origin: 'api expect',
        passed: true,
        expected: '201',
        actual: '201',
      },
    ],
    saved: [{ name: 'userId', from: '$.id', value: '"u_1"' }],
  }),
  compareRuns,
  testHistory,
  runLoad: () => [...loadTicks(), loadReport()],
  listApprovals: () => ({
    approvals: [
      {
        id: 3n,
        agent: 'claude',
        tool: 'vc_run',
        profile: 'prod',
        methods: ['POST'],
        summary: 'Run payments.charges against prod',
        arguments: '{"selector":"payments.charges"}',
        createdMs: F.now,
        status: E.ApprovalStatus.PENDING,
      },
    ],
  }),
  listAgentActions: () => ({
    actions: [
      {
        agent: 'claude',
        tool: 'vc_run',
        profile: 'staging',
        arguments: '{}',
        result: 'ok',
        atMs: F.now,
      },
    ],
  }),
  vcsStatus: vcs.status,
  vcsDiff: vcs.diff,
  ...schedules.fakes,
  ...usage.fakes,
  ...onboarding.fakes,
  ...settings.fakes,
};

const bin = <D extends DescMessage>(schema: D, value: MessageInitShape<D>): Uint8Array =>
  toBinary(schema, create(schema, value));

/** Connect's stream framing: each message with flag 0, then the end-of-stream trailer. */
function framed(messages: readonly Uint8Array[]): Buffer {
  const frames = [...messages.map((m) => [0, m] as const), [2, F.enc.encode('{}')] as const];
  return Buffer.concat(
    frames.map(([flag, bytes]) => {
      const head = Buffer.alloc(5);
      head.writeUInt8(flag, 0);
      head.writeUInt32BE(bytes.length, 1);
      return Buffer.concat([head, bytes]);
    }),
  );
}

function answer(
  name: string,
  body: Buffer,
  extra: Fakes,
): { type: string; body: Buffer } | undefined {
  const method = E.EngineService.methods.find((m) => m.name === name);
  const known = { ...fakes, ...extra } as Record<string, Fake<Methods[keyof Methods]>>;
  const fake = method && known[method.localName];
  if (!method || !fake) return undefined;
  const streamed = method.methodKind === 'server_streaming';
  const input = streamed ? body.subarray(5) : body;
  const out = fake(fromBinary(method.input, input) as never);
  const all = (Array.isArray(out) ? out : [out]).map((m) => bin(method.output, m as never));
  return streamed
    ? { type: 'application/connect+proto', body: framed(all) }
    : { type: 'application/proto', body: Buffer.from(all[0] ?? []) };
}

/**
 * Serves the engine API from the fakes, with a state's own `extra` answers over them; an RPC
 * with no fake answers Unimplemented (a seam).
 */
export async function routeEngine(page: Page, extra: Fakes = {}): Promise<void> {
  await page.route('**/api/**', (route) => {
    const name = new URL(route.request().url()).pathname.split('/').pop() ?? '';
    const reply = answer(name, route.request().postDataBuffer() ?? Buffer.alloc(0), extra);
    if (reply) return route.fulfill({ status: 200, contentType: reply.type, body: reply.body });
    return route.fulfill({
      status: 501,
      contentType: 'application/json',
      body: JSON.stringify({ code: 'unimplemented', message: `${name} is not faked` }),
    });
  });
}
