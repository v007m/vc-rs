import { ConnectError, type Client } from '@connectrpc/connect';
import { DefinitionKind, type EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { toRow, type TestRow } from './catalog.ts';

/** The part of the engine client the catalog needs, so tests can pass a fake. */
export type CatalogClient = Pick<Client<typeof EngineService>, 'listDefinitions'>;

/** The project's tests, loaded from the engine. */
export class Catalog {
  rows: readonly TestRow[] = $state([]);
  loading = $state(false);
  error: string | undefined = $state();
  readonly #client: CatalogClient;

  constructor(client: CatalogClient) {
    this.#client = client;
  }

  /**
   * Loads every test, replacing what was loaded before. The engine lists skipped tests too;
   * `skipped: true` would list only those (`vc list tests --skipped`).
   */
  async load(): Promise<void> {
    this.loading = true;
    try {
      const response = await this.#client.listDefinitions({ kind: DefinitionKind.TESTS });
      this.rows = response.definitions.map(toRow);
      this.error = undefined;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    } finally {
      this.loading = false;
    }
  }
}
