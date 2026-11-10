import { create } from '@bufbuild/protobuf';
import { describe, expect, it } from 'vitest';
import { DiagnosticSchema, Severity } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { baseName, byImportance, dirName, folders, runnable } from './files.ts';
import { foldDiff, lineDiff } from './text.ts';

describe('files', () => {
  it('groups files by folder and filters them by path', () => {
    const paths = [
      'services/users/crud.test.yaml',
      'vibranium.config.yaml',
      'services/users/users.api.yaml',
    ];
    expect(folders(paths, '')).toEqual([
      { dir: '', files: ['vibranium.config.yaml'] },
      {
        dir: 'services/users',
        files: ['services/users/crud.test.yaml', 'services/users/users.api.yaml'],
      },
    ]);
    expect(folders(paths, 'API')).toEqual([
      { dir: 'services/users', files: ['services/users/users.api.yaml'] },
    ]);
    expect([dirName('a.yaml'), baseName('a/b/c.yaml')]).toEqual(['', 'c.yaml']);
  });

  it('runs only test and load files', () => {
    expect(['a.test.yaml', 'b.load.yml', 'c.api.yaml'].map(runnable)).toEqual([true, true, false]);
  });

  it('puts errors first, then by line', () => {
    const at = (code: string, severity: Severity, startLine?: number) =>
      create(DiagnosticSchema, {
        code,
        severity,
        primary: { span: startLine ? { startLine } : undefined },
      });
    const sorted = byImportance([
      at('W9', Severity.WARNING, 1),
      at('E5', Severity.ERROR, 5),
      at('E0', Severity.ERROR),
      at('E2', Severity.ERROR, 2),
    ]);
    expect(sorted.map((d) => d.code)).toEqual(['E2', 'E5', 'E0', 'W9']);
  });

  it('folds unchanged lines away from changes', () => {
    const before = Array.from({ length: 12 }, (_, i) => `l${i}`).join('\n');
    const after = before.replace('l6', 'six');
    const folded = foldDiff(lineDiff(before, after), 2);
    expect(folded[0]).toEqual({ kind: 'gap', count: 4 });
    expect(folded.filter((l) => l.kind !== 'gap')).toHaveLength(6);
    expect(folded.at(-1)).toEqual({ kind: 'gap', count: 3 });
  });
});
