import { ConnectError, type Client } from '@connectrpc/connect';
import { Severity, type EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';

/** The part of the engine the shell's status line and env pill read, so tests can pass a fake. */
export type ProjectClient = Pick<Client<typeof EngineService>, 'getProject' | 'check'>;

/** What `vc check` found over the whole project. */
export type CheckSummary = Readonly<{ errors: number; warnings: number; files: number }>;

/**
 * The project facts the shell shows on every screen: vc's version and the default profile (the
 * env pill), and the last whole-project `vc check` (the status line). Each loads once; a failure
 * leaves its part blank with the reason, and the screens keep working.
 */
export class ProjectStatus {
  name: string | undefined = $state();
  version: string | undefined = $state();
  profile: string | undefined = $state();
  check: CheckSummary | undefined = $state();
  error: string | undefined = $state();
  readonly #client: ProjectClient;

  constructor(client: ProjectClient) {
    this.#client = client;
  }

  /** Loads the project and checks it; resolves when both have settled. */
  async load(): Promise<void> {
    await Promise.all([this.#project(), this.#check()]);
  }

  async #project(): Promise<void> {
    try {
      const { name, vcVersion, defaultProfile, profiles } = await this.#client.getProject({});
      this.name = name;
      this.version = vcVersion;
      this.profile = defaultProfile ?? profiles[0];
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    }
  }

  async #check(): Promise<void> {
    try {
      const { diagnostics, checked } = await this.#client.check({});
      const count = (severity: Severity) =>
        diagnostics.filter((d) => d.severity === severity).length;
      this.check = {
        errors: count(Severity.ERROR),
        warnings: count(Severity.WARNING),
        files: checked,
      };
    } catch (error) {
      this.error = ConnectError.from(error).rawMessage;
    }
  }
}

/** The status line's words for `check`: what vc check found, never colour alone. */
export function checkLine(check: CheckSummary | undefined): string {
  if (!check) return 'vc check …';
  const { errors, warnings } = check;
  if (errors === 0 && warnings === 0) return 'vc check: clean';
  const part = (n: number, word: string) => (n ? [`${n} ${word}${n === 1 ? '' : 's'}`] : []);
  return `vc check: ${[...part(errors, 'error'), ...part(warnings, 'warning')].join(', ')}`;
}
