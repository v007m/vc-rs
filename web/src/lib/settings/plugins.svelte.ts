import { ConnectError, type Client } from '@connectrpc/connect';
import { unlessMissing, type Maybe } from '#lib/api/maybe.ts';
import { fileChange } from '#lib/editor/port.ts';
import type { FileChange } from '#lib/editor/seams.ts';
import type {
  EngineService,
  PluginManifest,
  PluginPermissions,
} from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { preview, Staged, type Preview } from '#lib/writes/staged.svelte.ts';

/** What a plugin may reach: hosts, secret names and files. */
export type PluginReach = Readonly<{
  network: readonly string[];
  secrets: readonly string[];
  files: readonly string[];
}>;

/** What a plugin's manifest says it is. */
export type PluginAbout = Readonly<{ id: string; version: string; kind: string }>;

/** One plugin of the config or the lock (D715): what it asks for, what the lock grants it. */
export type PluginRow = Readonly<{
  id: string;
  source: string;
  about?: PluginAbout;
  asks: PluginReach;
  granted: PluginReach;
  /** A camelCase word: `ready`, `notInstalled`, `notApproved`, `asksForMore`, … */
  state: string;
  detail: string;
}>;

/** A plugin add's preview (D714): what it fetched, and the changes with their digest. */
export type PluginOffer = Readonly<{
  about?: PluginAbout;
  asks: PluginReach;
  resolved: string;
  integrity: string;
  warnings: readonly string[];
  files: readonly FileChange[];
  digest: string;
}>;

/** What the Plugins page asks of the engine; each change is a preview, then a write. */
export interface PluginsPort {
  list(): Promise<Maybe<readonly PluginRow[]>>;
  offer(source: string): Promise<Maybe<PluginOffer>>;
  add(source: string, digest: string): Promise<Maybe<unknown>>;
  removal(id: string): Promise<Maybe<Preview>>;
  remove(id: string, digest: string): Promise<Maybe<unknown>>;
}

const missing = 'Plugins need a newer vc: update it to list and add them here.';

const none: PluginReach = { network: [], secrets: [], files: [] };

function reach(found: PluginPermissions | undefined): PluginReach {
  return found ? { network: found.network, secrets: found.secrets, files: found.files } : none;
}

function about(manifest: PluginManifest | undefined): { about?: PluginAbout } {
  return manifest
    ? { about: { id: manifest.id, version: manifest.version, kind: manifest.kind } }
    : {};
}

type PluginsClient = Pick<
  Client<typeof EngineService>,
  'pluginList' | 'pluginAdd' | 'pluginRemove'
>;

/** The port over PluginList, PluginAdd and PluginRemove. */
export function pluginsPort(client: PluginsClient): PluginsPort {
  return {
    list: () =>
      unlessMissing(missing, async () =>
        (await client.pluginList({})).plugins.map((plugin) => ({
          id: plugin.id,
          source: plugin.source,
          ...about(plugin.manifest),
          asks: reach(plugin.manifest?.permissions),
          granted: reach(plugin.granted),
          state: plugin.state,
          detail: plugin.detail,
        })),
      ),
    offer: (source) =>
      unlessMissing(missing, async () => {
        const got = await client.pluginAdd({ source, write: false });
        const { resolved, integrity, warnings, digest } = got;
        const files = got.files.map(fileChange);
        const asks = reach(got.manifest?.permissions);
        return { ...about(got.manifest), asks, resolved, integrity, warnings, files, digest };
      }),
    add: (source, digest) =>
      unlessMissing(missing, () =>
        client.pluginAdd({ source, write: true, expectedDigest: digest }),
      ),
    removal: (id) =>
      unlessMissing(missing, async () => preview(await client.pluginRemove({ id, write: false }))),
    remove: (id, digest) =>
      unlessMissing(missing, () =>
        client.pluginRemove({ id, write: true, expectedDigest: digest }),
      ),
  };
}

/** `found` as one tag per thing it reaches: `network api.example.com`, `secrets SHOP_TOKEN`. */
export function reachTags(found: PluginReach): string[] {
  return (['network', 'secrets', 'files'] as const).flatMap((kind) =>
    found[kind].map((one) => `${kind} ${one}`),
  );
}

/**
 * The Plugins page (D714, D715): the plugins listed, one add previewed then written with its
 * digest, and one removal previewed then written with its own.
 */
export class Plugins {
  list: readonly PluginRow[] = $state.raw([]);
  loaded = $state(false);
  /** Why the engine has no plugin RPCs, when it has none. */
  missing: string | undefined = $state();
  error: string | undefined = $state();
  readonly adding = new Staged();
  offer: PluginOffer | undefined = $state.raw();
  readonly removing = new Staged();
  /** The plugin whose removal is open. */
  removal: string | undefined = $state();
  readonly #port: PluginsPort;

  constructor(port: PluginsPort) {
    this.#port = port;
  }

  async load(): Promise<void> {
    try {
      const got = await this.#port.list();
      if (got.available) this.list = got.value;
      else this.missing = got.reason;
      this.error = undefined;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    } finally {
      this.loaded = true;
    }
  }

  /** Forgets the add's preview, as when the source is edited or the dialog closes. */
  resetAdd(): void {
    this.offer = undefined;
    this.adding.reset();
  }

  previewAdd(source: string): Promise<void> {
    this.offer = undefined;
    return this.adding.review(async () => {
      const got = await this.#port.offer(source);
      if (got.available) this.offer = got.value;
      return got;
    });
  }

  /** Adds the plugin the preview showed, with its digest, then lists the plugins again. */
  async add(source: string): Promise<boolean> {
    const wrote = await this.adding.write((digest) => this.#port.add(source, digest));
    if (wrote) await this.#done(() => (this.offer = undefined));
    return wrote;
  }

  /** Opens `id`'s removal and previews it. */
  previewRemove(id: string): Promise<void> {
    this.removal = id;
    this.removing.reset();
    return this.removing.review(() => this.#port.removal(id));
  }

  closeRemove(): void {
    this.removal = undefined;
    this.removing.reset();
  }

  async remove(): Promise<boolean> {
    const id = this.removal;
    if (id === undefined) return false;
    const wrote = await this.removing.write((digest) => this.#port.remove(id, digest));
    if (wrote) await this.#done(() => (this.removal = undefined));
    return wrote;
  }

  async #done(close: () => void): Promise<void> {
    close();
    await this.load();
  }
}
