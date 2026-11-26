import { Code, ConnectError } from '@connectrpc/connect';
import { describe, expect, it } from 'vitest';
import { available } from '#lib/api/maybe.ts';
import type { ChangedFile, RepoStatus, VcsPort } from './port.ts';
import { VcsSession } from './session.svelte.ts';

const crud: ChangedFile = { path: 'services/users/crud.test.yaml', change: 'modified' };
const auth: ChangedFile = { path: 'services/users/auth.test.yaml', change: 'added' };

/** A repository whose files and commits the port's calls change, as git would. */
function repo(files: ChangedFile[], fail?: string) {
  const state = { files, ahead: 0, commits: [] as { paths: readonly string[]; message: string }[] };
  const status = (): RepoStatus => ({
    branch: 'main',
    upstream: 'origin/main',
    ahead: state.ahead,
    behind: 0,
    files: state.files,
  });
  const port: VcsPort = {
    status: () => Promise.resolve(available(status())),
    diff: (path) => Promise.resolve(`+${path}`),
    commit(paths, message) {
      if (fail) return Promise.reject(new ConnectError(fail, Code.FailedPrecondition));
      state.commits.push({ paths, message });
      state.files = state.files.filter((f) => !paths.includes(f.path));
      state.ahead += 1;
      return Promise.resolve('a1b2c3d');
    },
    push() {
      state.ahead = 0;
      return Promise.resolve('To origin\n   main -> main');
    },
    pull: () => Promise.resolve('Already up to date.'),
  };
  return { state, port };
}

describe('VcsSession', () => {
  it('picks every file when it first shows, and keeps what was unpicked', async () => {
    const { state, port } = repo([crud]);
    const vcs = new VcsSession(port);
    await vcs.refresh();
    expect(vcs.picked).toEqual([crud.path]);
    vcs.toggle(crud.path);
    state.files = [crud, auth];
    await vcs.refresh();
    expect(vcs.picked).toEqual([auth.path]);
    expect(vcs.branch).toBe('main');
  });

  it('commits only the picked files with the message, then refreshes', async () => {
    const { state, port } = repo([crud, auth]);
    const vcs = new VcsSession(port);
    await vcs.refresh();
    vcs.toggle(auth.path);
    expect(vcs.canCommit).toBe(false);
    vcs.message = '  test: cover user updates ';
    expect(await vcs.commit()).toBe(true);
    expect(state.commits).toEqual([{ paths: [crud.path], message: 'test: cover user updates' }]);
    expect(vcs.note).toEqual({ tone: 'ok', text: 'Committed 1 file as a1b2c3d' });
    expect(vcs.files).toEqual([auth]);
    expect(vcs.message).toBe('');
  });

  it('commits and pushes, and shows what git said', async () => {
    const { state, port } = repo([crud]);
    const vcs = new VcsSession(port);
    await vcs.refresh();
    vcs.message = 'test: crud';
    expect(await vcs.commitAndPush()).toBe(true);
    expect(state.ahead).toBe(0);
    expect(vcs.note?.output).toContain('main -> main');
  });

  it("keeps the message and shows git's words when the commit fails", async () => {
    const { port } = repo([crud], 'cannot commit: the pre-commit hook failed');
    const vcs = new VcsSession(port);
    await vcs.refresh();
    vcs.message = 'test: crud';
    expect(await vcs.commit()).toBe(false);
    expect(vcs.note).toEqual({ tone: 'fail', text: 'cannot commit: the pre-commit hook failed' });
    expect(vcs.message).toBe('test: crud');
  });

  it('bumps the revision after a pull, so the screens reload the files', async () => {
    const vcs = new VcsSession(repo([]).port);
    await vcs.pull();
    expect(vcs.revision).toBe(1);
    expect(vcs.note?.output).toBe('Already up to date.');
  });

  it('picks all, then none', async () => {
    const vcs = new VcsSession(repo([crud, auth]).port);
    await vcs.refresh();
    vcs.toggleAll();
    expect(vcs.picked.length).toBe(0);
    vcs.toggleAll();
    expect(vcs.picked.length).toBe(2);
  });
});
