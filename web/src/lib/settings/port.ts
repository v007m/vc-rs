import { Code, ConnectError, type Client } from '@connectrpc/connect';
import type { EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';

/** What the redactor puts in place of a secret; the server never sends the value itself. */
export const MASKED = '••••';

/** One value a key was set to, and the layer that set it (`built-in`, `--set`, …). */
export type ConfigLayer = Readonly<{ value: string; source: string }>;

/** A setting or `vars.<name>`, with the winning layer first and then each value it replaced. */
export type ConfigValue = Readonly<{ key: string; layers: readonly ConfigLayer[] }>;

export type Explained = Readonly<{ values: readonly ConfigValue[]; notices: readonly string[] }>;

export type ProjectProfiles = Readonly<{ profiles: readonly string[]; defaultProfile?: string }>;

/**
 * What Settings asks of the engine: the profiles to pick from, and every setting and var
 * explained under them as `vc config explain` does (D129).
 */
export interface SettingsPort {
  profiles(): Promise<ProjectProfiles>;
  explain(profiles: readonly string[]): Promise<Explained>;
}

/** The port over the engine client; ExplainConfig is not served yet. */
export function enginePort(client: Pick<Client<typeof EngineService>, 'getProject'>): SettingsPort {
  return {
    async profiles() {
      const { profiles, defaultProfile } = await client.getProject({});
      return defaultProfile === undefined ? { profiles } : { profiles, defaultProfile };
    },
    explain: () => Promise.reject(new ConnectError('this needs a newer vc ui', Code.Unimplemented)),
  };
}
