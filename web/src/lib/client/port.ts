import type { Client } from '@connectrpc/connect';
import {
  DefinitionKind,
  type EngineService,
  type FileChange as GenFileChange,
} from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { checkLine, exchangeLine } from '#lib/api/results.ts';
import { wireEdits, type PointerEdit } from '#lib/settings/configEdits.ts';
import { editedOf, type Edited } from '#lib/settings/configForm.svelte.ts';
import type { ConfigValue } from '#lib/settings/port.ts';
import type { CheckLine, ExchangeLine } from '#lib/results/types.ts';

/** An api the client can call, with the names of the vars it declares. */
export type ApiInfo = Readonly<{ id: string; name: string; path: string; vars: readonly string[] }>;

/** A flow test: a test whose `steps` call apis in order (R43). */
export type FlowInfo = Readonly<{ id: string; name: string; steps: number }>;

/**
 * Pointer edits to an api's or test's file, made over it as it was opened; a call sends them
 * unsaved, and `editFile` previews or writes them.
 */
export type ApiDraft = Readonly<{ path: string; digest: string; edits: readonly PointerEdit[] }>;

/** One ad hoc call, as `vc call` takes it, with the api as the form edited it. */
export type CallInput = Readonly<{
  api: string;
  /** `name=value`, in order, like `--var`. */
  vars: readonly string[];
  keep: boolean;
  draft?: ApiDraft;
}>;

/** A header the api is sent with that it does not write itself; `value` masked. */
export type InheritedHeader = Readonly<{ name: string; value: string; from: string }>;

/** The auth an api is sent with, by name; never its token or secret values. */
export type ApiAuthInfo = Readonly<{
  id: string;
  type: string;
  tokenUrl: string;
  secrets: readonly string[];
}>;

/**
 * An api or test as its file writes it (`node`, secrets masked), where it sits (`pointer`,
 * `/apis/2` or `/tests/0`) and the digest its edits go over.
 */
export type WrittenDefinition = Readonly<{
  path: string;
  pointer: string;
  digest: string;
  node: Readonly<Record<string, unknown>>;
}>;

/**
 * What a call adds to an api: each var's layers, the inherited headers, the auth and the call
 * as curl, the token as `$TOKEN`.
 */
export type ExplainedApi = Readonly<{
  vars: readonly ConfigValue[];
  headers: readonly InheritedHeader[];
  auth?: ApiAuthInfo;
  curl: string;
  notices: readonly string[];
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
  readDefinition(id: string): Promise<WrittenDefinition>;
  explainApi(api: string, vars: readonly string[]): Promise<ExplainedApi>;
  editFile(draft: ApiDraft, write: boolean): Promise<Edited>;
  saveCallAsTest(input: SaveCallInput): Promise<SavedTest>;
  importHar(input: HarInput): Promise<ChangesOutput>;
}

type EngineClient = Pick<
  Client<typeof EngineService>,
  | 'listDefinitions'
  | 'call'
  | 'readDefinition'
  | 'explainApi'
  | 'editFile'
  | 'saveCallAsTest'
  | 'import'
>;

function parsed(json: string): Readonly<Record<string, unknown>> {
  const node: unknown = JSON.parse(json);
  return typeof node === 'object' && node !== null && !Array.isArray(node) ? { ...node } : {};
}

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
    async call({ api, vars, keep, draft }) {
      const sent = draft && { ...draft, edits: wireEdits(draft.edits) };
      const out = await client.call({ api, vars: [...vars], keep, draft: sent });
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
    async readDefinition(id) {
      const { path, pointer, digest, json } = await client.readDefinition({ id });
      return { path, pointer, digest, node: parsed(json) };
    },
    async explainApi(api, vars) {
      const out = await client.explainApi({ api, vars: [...vars] });
      const { curl, notices, auth } = out;
      const values = out.vars.map(({ key, layers }) => ({
        key,
        layers: layers.map(({ value, source }) => ({ value, source })),
      }));
      const headers = out.headers.map(({ name, value, from }) => ({ name, value, from }));
      const info = auth && {
        id: auth.id,
        type: auth.type,
        tokenUrl: auth.tokenUrl,
        secrets: auth.secrets,
      };
      const explained = { vars: values, headers, curl, notices };
      return info ? { ...explained, auth: info } : explained;
    },
    async editFile({ path, digest, edits }, write) {
      return editedOf(await client.editFile({ path, digest, edits: wireEdits(edits), write }));
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
