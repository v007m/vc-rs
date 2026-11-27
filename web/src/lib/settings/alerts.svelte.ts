import { ConnectError, type Client } from '@connectrpc/connect';
import { unlessMissing, type Maybe } from '#lib/api/maybe.ts';
import type { EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { alertRules } from './alertsRules.ts';
import { CONFIG_PATH } from './configEdits.ts';

/** One channel a test alert tried: its rule's number (from 1), its kind and why it failed. */
export type AlertSent = Readonly<{ rule: number; channel: string; error?: string }>;

/** What the Alerts page asks of the engine. */
export interface AlertsPort {
  /** The config's alert rules, one text each, as it writes them. */
  rules(): Promise<Maybe<readonly string[]>>;
  /** Sends a test alert on every channel the rules name under `profile` (D713). */
  test(profile: string): Promise<Maybe<readonly AlertSent[]>>;
}

const missing = 'Test alerts need a newer vc: update it to send one from here.';
const noConfig = 'The config could not be read.';

type AlertsClient = Pick<Client<typeof EngineService>, 'readFile' | 'testAlert'>;

/** The port over ReadFile for the config's rules and TestAlert to send. */
export function alertsPort(client: AlertsClient): AlertsPort {
  return {
    rules: () =>
      unlessMissing(noConfig, async () =>
        alertRules((await client.readFile({ path: CONFIG_PATH })).text),
      ),
    test: (profile) =>
      unlessMissing(missing, async () => {
        const { sent } = await client.testAlert({ profiles: profile ? [profile] : [], sets: [] });
        return sent.map(({ rule, channel, error }) =>
          error === undefined ? { rule, channel } : { rule, channel, error },
        );
      }),
  };
}

/** The toast after a test: how many channels took it, and how many did not. */
export function sentText(sent: readonly AlertSent[]): string {
  const failed = sent.filter((one) => one.error !== undefined).length;
  const ok = sent.length - failed;
  const told = `Sent ${ok} test ${ok === 1 ? 'alert' : 'alerts'}.`;
  return failed === 0 ? told : `${told} ${failed} failed.`;
}

/** The Alerts page: the config's rules, read only, and the last test alert's channels. */
export class Alerts {
  rules: readonly string[] = $state.raw([]);
  loaded = $state(false);
  /** The channels the last test tried, or undefined before one. */
  sent: readonly AlertSent[] | undefined = $state.raw();
  sending = $state(false);
  /** Why the engine cannot send a test alert, when it cannot. */
  missing: string | undefined = $state();
  error: string | undefined = $state();
  readonly #port: AlertsPort;

  constructor(port: AlertsPort) {
    this.#port = port;
  }

  async load(): Promise<void> {
    try {
      const got = await this.#port.rules();
      this.rules = got.available ? got.value : [];
      this.error = got.available ? undefined : got.reason;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    } finally {
      this.loaded = true;
    }
  }

  /** Sends a test alert under `profile`; the channels it tried, or undefined when it failed. */
  async test(profile: string): Promise<readonly AlertSent[] | undefined> {
    this.sending = true;
    try {
      const got = await this.#port.test(profile);
      if (!got.available) this.missing = got.reason;
      else this.sent = got.value;
      this.error = undefined;
      return got.available ? got.value : undefined;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
      throw error;
    } finally {
      this.sending = false;
    }
  }
}
