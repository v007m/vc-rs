import { Code, ConnectError } from '@connectrpc/connect';
import { describe, expect, it } from 'vitest';
import { VcsChange } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { enginePort, gitError, vcsNotYet } from './port.ts';

describe('the Vcs port', () => {
  it('maps the status, with no counts for a binary file', async () => {
    const port = enginePort(
      fakeEngine({
        vcsStatus: () => ({
          branch: 'main',
          upstream: 'origin/main',
          ahead: 2,
          behind: 1,
          files: [
            {
              path: 'services/users/crud.test.yaml',
              change: VcsChange.MODIFIED,
              added: 3,
              removed: 1,
            },
            { path: 'fixtures/logo.png', change: VcsChange.UNTRACKED, binary: true },
          ],
        }),
      }),
    );
    expect(await port.status()).toEqual({
      available: true,
      value: {
        branch: 'main',
        upstream: 'origin/main',
        ahead: 2,
        behind: 1,
        files: [
          {
            path: 'services/users/crud.test.yaml',
            change: 'modified',
            lines: { added: 3, removed: 1 },
          },
          { path: 'fixtures/logo.png', change: 'untracked' },
        ],
      },
    });
  });

  it('says the git pane needs a newer engine when Vcs is missing', async () => {
    expect(await enginePort(fakeEngine({})).status()).toEqual({
      available: false,
      reason: vcsNotYet,
    });
  });

  it('sends the diff, commit, push and pull and gives back what git said', async () => {
    const seen: unknown[] = [];
    const port = enginePort(
      fakeEngine({
        vcsDiff: ({ path }) => ({ diff: `diff of ${path}` }),
        vcsCommit: ({ paths, message }) => {
          seen.push({ paths, message });
          return { commit: 'a1b2c3d' };
        },
        vcsPush: () => ({ output: 'pushed' }),
        vcsPull: () => ({ output: 'Already up to date.' }),
      }),
    );
    expect(await port.diff('a.test.yaml')).toBe('diff of a.test.yaml');
    expect(await port.commit(['a.test.yaml'], 'test: add a')).toBe('a1b2c3d');
    expect(seen).toEqual([{ paths: ['a.test.yaml'], message: 'test: add a' }]);
    expect(await port.push()).toBe('pushed');
    expect(await port.pull()).toBe('Already up to date.');
  });

  it("shows a git failure in git's own words, without the code", () => {
    const said =
      'cannot pull: git pull timed out after 120 s; it may be waiting for an SSH passphrase';
    expect(gitError(new ConnectError(said, Code.FailedPrecondition))).toBe(said);
  });
});
