import type { Maybe } from '#lib/api/maybe.ts';
import { gitError, type RepoStatus, type VcsPort } from './port.ts';

/** What the last git action said, for the pane's status line. */
export type GitNote = Readonly<{ tone: 'ok' | 'fail'; text: string; output?: string }>;

/**
 * The git pane's state (D518): the status, the files picked to commit, the message and what the
 * last action said. Every file is picked when it first shows. A commit, push or pull refreshes
 * the status; a pull also bumps `revision`, so screens that show the files reload them.
 */
export class VcsSession {
  status: Maybe<RepoStatus> | undefined = $state.raw();
  /** The paths picked to commit. */
  picked: readonly string[] = $state.raw([]);
  message = $state('');
  busy = $state(false);
  note: GitNote | undefined = $state.raw();
  /** Bumped when git changed the files on disk. */
  revision = $state(0);
  #seen: readonly string[] = [];
  readonly #port: VcsPort;

  constructor(port: VcsPort) {
    this.#port = port;
  }

  get files(): RepoStatus['files'] {
    return this.status?.available ? this.status.value.files : [];
  }

  get branch(): string | undefined {
    return this.status?.available ? this.status.value.branch || undefined : undefined;
  }

  get canCommit(): boolean {
    return !this.busy && this.picked.length > 0 && this.message.trim() !== '';
  }

  async refresh(): Promise<void> {
    try {
      this.status = await this.#port.status();
    } catch (error) {
      this.note = { tone: 'fail', text: gitError(error) };
      return;
    }
    const paths = this.files.map((file) => file.path);
    this.picked = paths.filter((path) => this.picked.includes(path) || !this.#seen.includes(path));
    this.#seen = paths;
  }

  toggle(path: string): void {
    const picked = this.picked.includes(path);
    this.picked = picked ? this.picked.filter((p) => p !== path) : [...this.picked, path];
  }

  /** Picks every file, or none when every file is picked. */
  toggleAll(): void {
    const all = this.picked.length === this.files.length;
    this.picked = all ? [] : this.files.map((file) => file.path);
  }

  diff(path: string): Promise<string> {
    return this.#port.diff(path);
  }

  /** Commits the picked files; resolves true when git made the commit. */
  async commit(): Promise<boolean> {
    if (!this.canCommit) return false;
    const paths = this.files.map((f) => f.path).filter((path) => this.picked.includes(path));
    return this.#act(async () => {
      const id = await this.#port.commit(paths, this.message.trim());
      this.message = '';
      const files = paths.length === 1 ? '1 file' : `${paths.length} files`;
      return { tone: 'ok', text: `Committed ${files} as ${id}` };
    });
  }

  push(): Promise<boolean> {
    return this.#act(async () => ({ tone: 'ok', text: 'Pushed', output: await this.#port.push() }));
  }

  async pull(): Promise<boolean> {
    const done = await this.#act(async () => ({
      tone: 'ok',
      text: 'Pulled',
      output: await this.#port.pull(),
    }));
    if (done) this.revision += 1;
    return done;
  }

  async commitAndPush(): Promise<boolean> {
    return (await this.commit()) && this.push();
  }

  async #act(work: () => Promise<GitNote>): Promise<boolean> {
    this.busy = true;
    try {
      this.note = await work();
      return true;
    } catch (error) {
      this.note = { tone: 'fail', text: gitError(error) };
      return false;
    } finally {
      this.busy = false;
      await this.refresh();
    }
  }
}
