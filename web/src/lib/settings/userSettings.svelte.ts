import { ConnectError, type Client } from '@connectrpc/connect';
import { unlessMissing, type Maybe } from '#lib/api/maybe.ts';
import type { EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { Staged, type Preview } from '#lib/writes/staged.svelte.ts';
import {
  userKeyProblem,
  userRows,
  withEdit,
  type UserEdit,
  type UserRow,
  type UserSetting,
} from './userKeys.ts';

/** The user config as the engine reads it: its fixed path (`~` for home), digest and settings. */
export type UserListing = Readonly<{
  path: string;
  digest: string;
  settings: readonly UserSetting[];
}>;

/** What Your settings asks of the engine; no request names a path (D711). */
export interface UserSettingsPort {
  read(): Promise<Maybe<UserListing>>;
  /** The file before and after `edits`, masked, with the digest a write sends back. */
  preview(edits: readonly UserEdit[]): Promise<Maybe<Preview>>;
  write(edits: readonly UserEdit[], digest: string): Promise<Maybe<unknown>>;
}

const missing = 'Your settings need a newer vc: update it to read and change them here.';

type UserClient = Pick<Client<typeof EngineService>, 'readUserSettings' | 'writeUserSettings'>;

const wire = (edits: readonly UserEdit[]) =>
  edits.map((edit) => (edit.value === undefined ? { key: edit.key } : { ...edit }));

/** The port over ReadUserSettings and WriteUserSettings. */
export function userSettingsPort(client: UserClient): UserSettingsPort {
  return {
    read: () =>
      unlessMissing(missing, async () => {
        const { path, digest, settings } = await client.readUserSettings({});
        return {
          path,
          digest,
          settings: settings.map(({ key, value, shadowed }) => ({ key, value, shadowed })),
        };
      }),
    preview: (edits) =>
      unlessMissing(missing, async () => {
        const got = await client.writeUserSettings({ edits: wire(edits), write: false });
        const change = { path: got.path, before: got.before ?? '', after: got.after };
        return { files: [change], digest: got.digest };
      }),
    write: (edits, digest) =>
      unlessMissing(missing, () =>
        client.writeUserSettings({ edits: wire(edits), write: true, expectedDigest: digest }),
      ),
  };
}

/**
 * Your settings (D711, D712): the user config's settings, edits staged over them, then a
 * preview of the file and a write with the preview's digest.
 */
export class UserSettings {
  listing: UserListing | undefined = $state.raw();
  /** Why the engine has no user settings RPCs, when it has none. */
  missing: string | undefined = $state();
  error: string | undefined = $state();
  edits: readonly UserEdit[] = $state.raw([]);
  readonly staged = new Staged();
  readonly rows: readonly UserRow[] = $derived(userRows(this.listing?.settings ?? [], this.edits));
  readonly #port: UserSettingsPort;

  constructor(port: UserSettingsPort) {
    this.#port = port;
  }

  async load(): Promise<void> {
    try {
      const got = await this.#port.read();
      if (got.available) this.listing = got.value;
      else this.missing = got.reason;
      this.error = undefined;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    }
  }

  /** Stages `key` set to `value`; the reason when the key is not one a client may set. */
  set(key: string, value: string): string | undefined {
    const problem = userKeyProblem(key);
    if (problem === undefined) this.#stage(withEdit(this.edits, { key, value }));
    return problem;
  }

  /** Stages `key`'s removal, or drops the edit that would add it. */
  remove(key: string): void {
    const known = this.listing?.settings.some((setting) => setting.key === key) ?? false;
    this.#stage(known ? withEdit(this.edits, { key }) : this.#without(key));
  }

  undo(key: string): void {
    this.#stage(this.#without(key));
  }

  discard(): void {
    this.#stage([]);
  }

  preview(): Promise<void> {
    const edits = this.edits;
    return this.staged.review(() => this.#port.preview(edits));
  }

  /** Writes the previewed edits with the preview's digest, then reads the file again. */
  async write(): Promise<boolean> {
    const edits = this.edits;
    const wrote = await this.staged.write((digest) => this.#port.write(edits, digest));
    if (wrote) {
      this.edits = [];
      await this.load();
    }
    return wrote;
  }

  #without(key: string): UserEdit[] {
    return this.edits.filter((edit) => edit.key !== key);
  }

  #stage(edits: readonly UserEdit[]): void {
    this.edits = edits;
    this.staged.reset();
  }
}
