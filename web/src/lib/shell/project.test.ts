import { describe, expect, it } from 'vitest';
import { Code, ConnectError } from '@connectrpc/connect';
import { Severity } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { fakeEngine } from '#lib/testing/engine.ts';
import { checkLine, ProjectStatus } from './project.svelte.ts';

describe('ProjectStatus', () => {
  it('loads the version, the default profile and what vc check found', async () => {
    const project = new ProjectStatus(
      fakeEngine({
        getProject: () => ({
          name: 'shop',
          vcVersion: '2.0.0',
          profiles: ['dev', 'prod'],
          defaultProfile: 'prod',
        }),
        check: () => ({
          checked: 4,
          diagnostics: [
            { severity: Severity.ERROR },
            { severity: Severity.WARNING },
            { severity: Severity.ERROR },
          ],
        }),
      }),
    );
    await project.load();
    expect(project).toMatchObject({ name: 'shop', version: '2.0.0', profile: 'prod' });
    expect(project.profiles).toEqual(['dev', 'prod']);
    expect(project.check).toEqual({ errors: 2, warnings: 1, files: 4 });
  });

  it('falls back to the first profile with no default', async () => {
    const project = new ProjectStatus(fakeEngine({ getProject: () => ({ profiles: ['dev'] }) }));
    await project.load();
    expect(project.profile).toBe('dev');
  });

  it('keeps the reason when the engine fails, and leaves the rest blank', async () => {
    const project = new ProjectStatus(
      fakeEngine({
        getProject: () => {
          throw new ConnectError('no project here', Code.FailedPrecondition);
        },
      }),
    );
    await project.load();
    expect(project.version).toBeUndefined();
    expect(project.error).toBeTruthy();
  });

  it('switches to a known profile only', async () => {
    const project = new ProjectStatus(
      fakeEngine({ getProject: () => ({ profiles: ['dev', 'qa'] }) }),
    );
    await project.load();
    project.useProfile('qa');
    project.useProfile('nowhere');
    expect(project.profile).toBe('qa');
  });

  it('lists the project files in order for the palette, and keeps them on a failure', async () => {
    let fail = false;
    const project = new ProjectStatus(
      fakeEngine({
        listDefinitions: () => {
          if (fail) throw new ConnectError('gone', Code.Unavailable);
          return { definitions: [{ path: 'b.test.yaml' }, { path: 'a.api.yaml' }] };
        },
      }),
    );
    await project.listFiles();
    fail = true;
    await project.listFiles();
    expect(project.files).toEqual(['a.api.yaml', 'b.test.yaml']);
  });
});

describe('checkLine', () => {
  it('says what vc check found in words', () => {
    expect(checkLine(undefined)).toBe('vc check …');
    expect(checkLine({ errors: 0, warnings: 0, files: 3 })).toBe('vc check: clean');
    expect(checkLine({ errors: 1, warnings: 2, files: 3 })).toBe('vc check: 1 error, 2 warnings');
    expect(checkLine({ errors: 0, warnings: 1, files: 3 })).toBe('vc check: 1 warning');
  });
});
