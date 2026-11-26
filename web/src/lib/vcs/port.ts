import { ConnectError, type Client } from '@connectrpc/connect';
import { VcsChange, type EngineService } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { unlessMissing, type Maybe } from '#lib/api/maybe.ts';

/** How a file changed since the last commit, as git's one letter says it (never colour alone). */
export type Change = 'added' | 'modified' | 'deleted' | 'untracked' | 'conflicted';

export type ChangedFile = Readonly<{
  path: string;
  change: Change;
  /** Lines added and removed since the last commit; a binary file has none. */
  lines?: Readonly<{ added: number; removed: number }>;
}>;

/** The git pane's view of the repository (D546). */
export type RepoStatus = Readonly<{
  /** Empty when HEAD is detached. */
  branch: string;
  /** Empty when the branch tracks no upstream. */
  upstream: string;
  ahead: number;
  behind: number;
  files: readonly ChangedFile[];
}>;

/** The git pane's port over the engine's Vcs RPCs (D546-D550), so tests can pass a fake. */
export interface VcsPort {
  status(): Promise<Maybe<RepoStatus>>;
  /** The file's masked unified diff against the last commit; empty when it did not change. */
  diff(path: string): Promise<string>;
  /** Commits `paths` as they are now and gives the short commit ID. */
  commit(paths: readonly string[], message: string): Promise<string>;
  /** What git said. */
  push(): Promise<string>;
  pull(): Promise<string>;
}

export type VcsClient = Pick<
  Client<typeof EngineService>,
  'vcsStatus' | 'vcsDiff' | 'vcsCommit' | 'vcsPush' | 'vcsPull'
>;

export const vcsNotYet = 'The git pane needs a newer engine.';

const changes: Partial<Record<VcsChange, Change>> = {
  [VcsChange.ADDED]: 'added',
  [VcsChange.MODIFIED]: 'modified',
  [VcsChange.DELETED]: 'deleted',
  [VcsChange.UNTRACKED]: 'untracked',
  [VcsChange.CONFLICTED]: 'conflicted',
};

/** git's one letter for `change`, as `git status --short` shows it. */
export const letters: Record<Change, string> = {
  added: 'A',
  modified: 'M',
  deleted: 'D',
  untracked: 'U',
  conflicted: 'C',
};

/**
 * What a failed git action says. A git failure is `failed_precondition` with git's own words
 * (D549), the 120 s timeout's hint among them (D554), so it is shown as it is, without the code.
 */
export function gitError(error: unknown): string {
  return ConnectError.from(error).rawMessage;
}

export function enginePort(client: VcsClient): VcsPort {
  return {
    status: () =>
      unlessMissing(vcsNotYet, async () => {
        const { branch, upstream, ahead, behind, files } = await client.vcsStatus({});
        return {
          branch,
          upstream,
          ahead,
          behind,
          files: files.map(({ path, change, added, removed, binary }) => ({
            path,
            change: changes[change] ?? 'modified',
            ...(binary ? {} : { lines: { added, removed } }),
          })),
        };
      }),
    diff: async (path) => (await client.vcsDiff({ path })).diff,
    commit: async (paths, message) =>
      (await client.vcsCommit({ paths: [...paths], message })).commit,
    push: async () => (await client.vcsPush({})).output,
    pull: async () => (await client.vcsPull({})).output,
  };
}
