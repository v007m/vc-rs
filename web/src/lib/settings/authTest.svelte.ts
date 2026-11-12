import { ConnectError, type Client } from '@connectrpc/connect';
import type { EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';

/** What one auth gave for an api: what its credentials are called and how long they hold. */
export type AuthResult = Readonly<{
  api: string;
  headers: readonly string[];
  query: readonly string[];
  cookies: readonly string[];
  expiresInMs?: number;
  error?: string;
}>;

/** What the auth section asks of the engine; a credential's value never comes back. */
export interface AuthTestPort {
  auths(): Promise<readonly string[]>;
  /** Tests `auth` under `profile`, or under the default profile when it is empty. */
  test(auth: string, profile: string): Promise<AuthResult>;
}

/** The port over the engine client: GetProject for the auth IDs, TestAuth for each test. */
export function authTestPort(
  client: Pick<Client<typeof EngineService>, 'getProject' | 'testAuth'>,
): AuthTestPort {
  return {
    async auths() {
      return (await client.getProject({})).auths;
    },
    async test(auth, profile) {
      const found = await client.testAuth({ auth, profiles: profile ? [profile] : [] });
      const { api, headers, query, cookies } = found;
      const expiry =
        found.expiresInMs === undefined ? {} : { expiresInMs: Number(found.expiresInMs) };
      const error = found.error === undefined ? {} : { error: found.error };
      return { api, headers, query, cookies, ...expiry, ...error };
    },
  };
}

/** `ms` as the largest whole unit: `59 min`, `2 h`, `30 s`. */
export function lifetime(ms: number): string {
  const units: readonly [number, string][] = [
    [86_400_000, 'd'],
    [3_600_000, 'h'],
    [60_000, 'min'],
    [1000, 's'],
  ];
  const [size, unit] = units.find(([size]) => ms >= size) ?? [1, 'ms'];
  return `${Math.floor(ms / size)} ${unit}`;
}

/** Each auth the config defines, and what testing it under a profile last gave. */
export class AuthTest {
  auths: readonly string[] = $state([]);
  /** The last result by auth, with the profile it was tested under. */
  results: Readonly<Record<string, AuthResult & { profile: string }>> = $state({});
  /** The auth being tested now. */
  testing: string | undefined = $state();
  error: string | undefined = $state();
  readonly #port: AuthTestPort;

  constructor(port: AuthTestPort) {
    this.#port = port;
  }

  async load(): Promise<void> {
    try {
      this.auths = await this.#port.auths();
      this.error = undefined;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    }
  }

  /** Obtains `auth`'s credentials under `profile`, without sending a call. */
  async test(auth: string, profile: string): Promise<void> {
    if (this.testing !== undefined) return;
    this.testing = auth;
    try {
      const found = await this.#port.test(auth, profile);
      this.results = { ...this.results, [auth]: { ...found, profile } };
      this.error = undefined;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    } finally {
      this.testing = undefined;
    }
  }
}
