import { ConnectError } from '@connectrpc/connect';
import type { ConfigValue, SettingsPort } from './port.ts';

/** The Settings screen's state: the profile picked and every value explained under it. */
export class EffectiveSettings {
  profiles: readonly string[] = $state([]);
  profile = $state('');
  values: readonly ConfigValue[] = $state([]);
  notices: readonly string[] = $state([]);
  loading = $state(false);
  error: string | undefined = $state();
  readonly #port: SettingsPort;

  constructor(port: SettingsPort) {
    this.#port = port;
  }

  /** Reads the profiles, then explains the settings under the default one. */
  async load(): Promise<void> {
    try {
      const { profiles, defaultProfile } = await this.#port.profiles();
      this.profiles = profiles;
      this.profile = defaultProfile ?? profiles[0] ?? '';
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
      return;
    }
    await this.explain();
  }

  async choose(profile: string): Promise<void> {
    this.profile = profile;
    await this.explain();
  }

  async explain(): Promise<void> {
    this.loading = true;
    try {
      const { values, notices } = await this.#port.explain(this.profile ? [this.profile] : []);
      this.values = values;
      this.notices = notices;
      this.error = undefined;
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    } finally {
      this.loading = false;
    }
  }
}
