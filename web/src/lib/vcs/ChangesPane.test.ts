import { Code, ConnectError } from '@connectrpc/connect';
import { render, screen, within } from '@testing-library/svelte';
import { userEvent } from '@testing-library/user-event';
import { describe, expect, it } from 'vitest';
import { available, notYet } from '#lib/api/maybe.ts';
import ChangesPane from './ChangesPane.svelte';
import type { RepoStatus, VcsPort } from './port.ts';
import { VcsSession } from './session.svelte.ts';

const status: RepoStatus = {
  branch: 'feature/users',
  upstream: 'origin/feature/users',
  ahead: 1,
  behind: 2,
  files: [
    {
      path: 'services/users/crud.test.yaml',
      change: 'modified',
      lines: { added: 4, removed: 1 },
    },
    { path: 'fixtures/logo.png', change: 'added' },
  ],
};

function port(over: Partial<VcsPort> = {}): VcsPort {
  return {
    status: () => Promise.resolve(available(status)),
    diff: () => Promise.resolve('--- a/crud\n+++ b/crud\n@@ -1 +1 @@\n-old: 1\n+new: 2'),
    commit: () => Promise.resolve('a1b2c3d'),
    push: () => Promise.resolve('pushed'),
    pull: () => Promise.resolve('Fast-forward'),
    ...over,
  };
}

async function shown(over?: Partial<VcsPort>): Promise<VcsSession> {
  const vcs = new VcsSession(port(over));
  await vcs.refresh();
  render(ChangesPane, { vcs });
  return vcs;
}

describe('ChangesPane', () => {
  it('shows the branch, its commits ahead and behind, and each file with its letter and counts', async () => {
    await shown();
    expect(screen.getByText('feature/users')).toBeTruthy();
    expect(screen.getByLabelText('1 commit to push, 2 commits to pull')).toBeTruthy();
    const rows = within(screen.getByRole('list', { name: 'Changed files' })).getAllByRole(
      'listitem',
    );
    expect(rows.map((row) => row.textContent?.replace(/\s+/g, ' ').trim())).toEqual([
      'M crud.test.yaml +4 −1',
      'A logo.png',
    ]);
    expect(screen.getByLabelText('modified').textContent).toBe('M');
  });

  it('commits the picked files with a message', async () => {
    const commits: (readonly string[])[] = [];
    await shown({
      commit: (paths) => {
        commits.push(paths);
        return Promise.resolve('a1b2c3d');
      },
    });
    const commit = screen.getByRole('button', { name: 'Commit 2 files' });
    expect(commit).toHaveProperty('disabled', true);
    await userEvent.click(screen.getByRole('checkbox', { name: 'Pick fixtures/logo.png' }));
    await userEvent.type(screen.getByRole('textbox', { name: 'Commit message' }), 'test: crud');
    await userEvent.click(screen.getByRole('button', { name: 'Commit 1 file' }));
    expect(commits).toEqual([['services/users/crud.test.yaml']]);
    expect(screen.getByRole('status').textContent).toContain('Committed 1 file as a1b2c3d');
  });

  it('opens a file’s diff, keeping each line’s sign', async () => {
    await shown();
    await userEvent.click(screen.getByRole('button', { name: /crud\.test\.yaml/ }));
    const diff = await screen.findByLabelText('Diff of services/users/crud.test.yaml');
    expect(diff.textContent).toContain('-old: 1');
    expect(diff.textContent).toContain('+new: 2');
  });

  it("pulls from the menu and shows git's failure as git said it", async () => {
    const said = 'cannot pull: Not possible to fast-forward, aborting.';
    await shown({ pull: () => Promise.reject(new ConnectError(said, Code.FailedPrecondition)) });
    await userEvent.click(screen.getByRole('button', { name: 'More git actions' }));
    await userEvent.click(screen.getByRole('menuitem', { name: /Pull/ }));
    expect((await screen.findByRole('status')).textContent).toContain(said);
  });

  it('offers Push when nothing is left to commit', async () => {
    await shown({ status: () => Promise.resolve(available({ ...status, files: [] })) });
    expect(screen.getByText('No changes to commit.')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Push 1 commit' })).toBeTruthy();
  });

  it('says quietly when the engine has no git pane', async () => {
    await shown({ status: () => Promise.resolve(notYet('The git pane needs a newer engine.')) });
    expect(screen.getByText('The git pane needs a newer engine.')).toBeTruthy();
  });
});
