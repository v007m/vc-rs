import { ConnectError, type Client } from '@connectrpc/connect';
import type { EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';

/** One profile's check: the names it declared, those that resolve, and why none could. */
export type ProfileCheck = Readonly<{
  declared: readonly string[];
  resolved: readonly string[];
  error?: string;
}>;

/** What the secrets section asks the engine to check; a value never comes back. */
export interface SecretCheckPort {
  check(profile: string): Promise<ProfileCheck>;
}

/** The port over the engine client's `CheckSecrets`. */
export function secretCheckPort(
  client: Pick<Client<typeof EngineService>, 'checkSecrets'>,
): SecretCheckPort {
  return {
    async check(profile) {
      const { secrets, error } = await client.checkSecrets({ profiles: [profile] });
      const declared = secrets.map((secret) => secret.name);
      const resolved = secrets.filter((secret) => secret.resolved).map((secret) => secret.name);
      return error === undefined ? { declared, resolved } : { declared, resolved, error };
    },
  };
}

export type SecretState = 'resolved' | 'not set' | 'not checked';

/** Whether each declared secret resolves, by profile, as last checked. */
export class SecretCheck {
  byProfile: Readonly<Record<string, ProfileCheck>> = $state({});
  checking = $state(false);
  error: string | undefined = $state();
  /** Each profile whose provider could not load, with why. */
  readonly failures = $derived(
    Object.entries(this.byProfile).flatMap(([profile, found]) =>
      found.error === undefined ? [] : [`${profile}: ${found.error}`],
    ),
  );
  readonly #port: SecretCheckPort;

  constructor(port: SecretCheckPort) {
    this.#port = port;
  }

  /** Checks every one of `profiles` again. */
  async check(profiles: readonly string[]): Promise<void> {
    this.checking = true;
    try {
      const found = await Promise.all(
        profiles.map(async (profile) => [profile, await this.#port.check(profile)] as const),
      );
      this.byProfile = Object.fromEntries(found);
      this.error = undefined;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    } finally {
      this.checking = false;
    }
  }

  /** `name` under `profile`: not checked when the last check did not declare it. */
  state(name: string, profile: string): SecretState {
    const found = this.byProfile[profile];
    if (found === undefined || !found.declared.includes(name)) return 'not checked';
    return found.resolved.includes(name) ? 'resolved' : 'not set';
  }
}
