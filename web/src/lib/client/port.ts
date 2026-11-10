import { Code, ConnectError, type Client } from '@connectrpc/connect';
import { DefinitionKind, type EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';

/** An api the client can call, with the names of the vars it declares. */
export type ApiInfo = Readonly<{ id: string; name: string; path: string; vars: readonly string[] }>;

/** One ad hoc call, as `vc call` takes it. */
export type CallInput = Readonly<{
  api: string;
  /** `name=value`, in order, like `--var`. */
  vars: readonly string[];
  keep: boolean;
}>;

/** What a call gave back; `responseJson` is the masked `status`/`headers`/`body` value. */
export type CallOutput = Readonly<{
  exit: number;
  text: string;
  notices: readonly string[];
  responseJson?: string;
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

/** One file an engine change touches, before and after. */
export type FileChange = Readonly<{
  path: string;
  before: string;
  after: string;
  movedFrom?: string;
}>;

/** What an engine change did or would do. */
export type ChangesOutput = Readonly<{
  files: readonly FileChange[];
  titles: readonly string[];
  notices: readonly string[];
}>;

/** A HAR import; `write: false` previews it, `true` writes it. */
export type HarInput = Readonly<{ source: string; flow?: string; write: boolean }>;

/**
 * What the API client asks of the engine. The M9b protos (Call, SaveCallAsTest, Import) are
 * not generated yet; `enginePort` maps this onto the generated client once they are.
 */
export interface ClientPort {
  apis(): Promise<ApiInfo[]>;
  call(input: CallInput): Promise<CallOutput>;
  saveCallAsTest(input: SaveCallInput): Promise<ChangesOutput>;
  importHar(input: HarInput): Promise<ChangesOutput>;
}

function notYet(): Promise<never> {
  return Promise.reject(new ConnectError('this needs a newer vc ui', Code.Unimplemented));
}

/** The port over the engine client: apis come from ListDefinitions, the rest is not served yet. */
export function enginePort(
  client: Pick<Client<typeof EngineService>, 'listDefinitions'>,
): ClientPort {
  return {
    async apis() {
      const { definitions } = await client.listDefinitions({ kind: DefinitionKind.APIS });
      return definitions.map((d) => ({ id: d.id, name: d.name ?? d.id, path: d.path, vars: [] }));
    },
    call: notYet,
    saveCallAsTest: notYet,
    importHar: notYet,
  };
}
