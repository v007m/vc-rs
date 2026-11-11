import type { Client } from '@connectrpc/connect';
import {
  DefinitionKind,
  type EngineService,
  type FileChange as GenFileChange,
} from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { checkLine, exchangeLine } from '#lib/api/results.ts';
import type { CheckLine, ExchangeLine } from '#lib/results/types.ts';

/** An api the client can call, with the names of the vars it declares. */
export type ApiInfo = Readonly<{ id: string; name: string; path: string; vars: readonly string[] }>;

/** A flow test: a test whose `steps` call apis in order (R43). */
export type FlowInfo = Readonly<{ id: string; name: string; steps: number }>;

/** One ad hoc call, as `vc call` takes it. */
export type CallInput = Readonly<{
  api: string;
  /** `name=value`, in order, like `--var`. */
  vars: readonly string[];
  keep: boolean;
}>;

/** A value the api's `save` took from the response: `from` as written, `value` masked. */
export type SavedLine = Readonly<{ name: string; from: string; value: string }>;

/**
 * What a call gave back; `responseJson` is the masked `status`/`headers`/`body` value, and
 * `exchange` the request as sent and the response with its timing, absent when none came.
 */
export type CallOutput = Readonly<{
  exit: number;
  text: string;
  notices: readonly string[];
  responseJson?: string;
  exchange?: ExchangeLine;
  checks: readonly CheckLine[];
  saved: readonly SavedLine[];
}>;

/**
 * A call saved as a test expecting its status and the named body fields at the values its
 * response held (D176); the engine reads them from `responseJson`, it does not call again.
 */
export type SaveCallInput = Readonly<{
  testId: string;
  api: string;
  vars: readonly string[];
  responseJson?: string;
  fields: readonly string[];
}>;

/** One file an engine change touches; `before` is empty for a new file. */
export type FileChange = Readonly<{
  path: string;
  before: string;
  after: string;
  movedFrom?: string;
}>;

/** What an import did or would do; `titles` also name what it could not carry over. */
export type ChangesOutput = Readonly<{ files: readonly FileChange[]; titles: readonly string[] }>;

/** The test a call was saved as. */
export type SavedTest = Readonly<{ id: string; path: string; titles: readonly string[] }>;

/** A HAR import; `write: false` previews it, `true` writes it. */
export type HarInput = Readonly<{ source: string; flow?: string; write: boolean }>;

/** What the API client asks of the engine; `enginePort` serves it from the engine client. */
export interface ClientPort {
  apis(): Promise<ApiInfo[]>;
  flows(): Promise<FlowInfo[]>;
  call(input: CallInput): Promise<CallOutput>;
  saveCallAsTest(input: SaveCallInput): Promise<SavedTest>;
  importHar(input: HarInput): Promise<ChangesOutput>;
}

type EngineClient = Pick<
  Client<typeof EngineService>,
  'listDefinitions' | 'call' | 'saveCallAsTest' | 'import'
>;

function fileChange(file: GenFileChange): FileChange {
  const { path, before = '', after, movedFrom } = file;
  return movedFrom === undefined ? { path, before, after } : { path, before, after, movedFrom };
}

/** The port over the engine client: a flow is a test with steps (R43). */
export function enginePort(client: EngineClient): ClientPort {
  return {
    async apis() {
      const { definitions } = await client.listDefinitions({ kind: DefinitionKind.APIS });
      return definitions.map((d) => ({
        id: d.id,
        name: d.name ?? d.id,
        path: d.path,
        vars: d.vars,
      }));
    },
    async flows() {
      const { definitions } = await client.listDefinitions({ kind: DefinitionKind.TESTS });
      return definitions
        .filter((d) => d.steps > 0)
        .map((d) => ({ id: d.id, name: d.name ?? d.id, steps: d.steps }));
    },
    async call({ api, vars, keep }) {
      const out = await client.call({ api, vars: [...vars], keep });
      const { exit, text, notices, responseJson, exchange } = out;
      const checks = out.checks.map(checkLine);
      const saved = out.saved.map(({ name, from, value }) => ({ name, from, value }));
      return {
        exit,
        text,
        notices,
        checks,
        saved,
        ...(responseJson === undefined ? {} : { responseJson }),
        ...(exchange === undefined ? {} : { exchange: exchangeLine(exchange) }),
      };
    },
    async saveCallAsTest({ testId, api, vars, responseJson, fields }) {
      const { id, path, titles } = await client.saveCallAsTest({
        id: testId,
        api,
        vars: [...vars],
        responseJson,
        fields: [...fields],
      });
      return { id, path, titles };
    },
    async importHar({ source, flow, write }) {
      const { files, titles } = await client.import({ kind: 'har', source, flow, write });
      return { files: files.map(fileChange), titles };
    },
  };
}
