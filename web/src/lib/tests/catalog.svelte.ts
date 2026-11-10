import { ConnectError, type Client } from '@connectrpc/connect';
import { SvelteMap } from 'svelte/reactivity';
import { DefinitionKind, type EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { toRow, type TestRow } from './catalog.ts';

/** The part of the engine client the catalog needs, so tests can pass a fake. */
export type CatalogClient = Pick<Client<typeof EngineService>, 'listDefinitions'>;

/** The project's tests, loaded from the engine. */
export class Catalog {
  rows: readonly TestRow[] = $state([]);
  loading = $state(false);
  error: string | undefined = $state();
  /** The IDs of the tests in files changed since each git ref asked for since the last load. */
  readonly changed = new SvelteMap<string, ReadonlySet<string>>();
  /** Why the engine could not answer for a git ref, by ref. */
  readonly changedErrors = new SvelteMap<string, string>();
  readonly #client: CatalogClient;
  /** The refs asked for or being asked for; only read here, so not reactive. */
  // eslint-disable-next-line svelte/prefer-svelte-reactivity -- never read by a view
  readonly #asked = new Set<string>();

  constructor(client: CatalogClient) {
    this.#client = client;
  }

  /**
   * Loads every test, replacing what was loaded before, and forgets which tests changed since
   * each ref, since a save or a write may have changed them. The engine lists skipped tests too;
   * `skipped: true` would list only those (`vc list tests --skipped`).
   */
  async load(): Promise<void> {
    this.loading = true;
    this.#asked.clear();
    this.changed.clear();
    this.changedErrors.clear();
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

  /**
   * Asks the engine once for the tests in files changed since the git ref `since`, as
   * `vc run --changed` picks them. Until it answers, `changed` has no entry for it.
   */
  async loadChanged(since: string): Promise<void> {
    if (this.#asked.has(since)) return;
    this.#asked.add(since);
    this.changedErrors.delete(since);
    try {
      const response = await this.#client.listDefinitions({
        kind: DefinitionKind.TESTS,
        selection: { changed: since },
      });
      const ids = response.definitions.map((d) => d.id);
      // eslint-disable-next-line svelte/prefer-svelte-reactivity -- never changed once stored
      if (this.#asked.has(since)) this.changed.set(since, new Set(ids));
    } catch (error) {
      if (!this.#asked.delete(since)) return;
      this.changedErrors.set(since, ConnectError.from(error).rawMessage);
    }
  }
}
