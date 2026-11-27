import { create } from '@bufbuild/protobuf';
import { DiagnosticSchema, Severity } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { describe, expect, it } from 'vitest';
import { marks } from './marks.ts';

const path = 'services/users/crud.test.yaml';
const text = 'name: one\nid: x\n';

function at(file: string, startLine: number, startColumn: number, endLine = 0, endColumn = 0) {
  return create(DiagnosticSchema, {
    code: 'V2001',
    severity: Severity.ERROR,
    message: 'unknown id',
    primary: { span: { file, startLine, startColumn, endLine, endColumn } },
    fixes: [
      { title: 'use getsUser', certain: true },
      { title: 'maybe', certain: false },
    ],
  });
}

describe('marks', () => {
  it('places the diagnostics of the file by line and column, with their certain fixes', () => {
    const [mark] = marks(text, path, [at(path, 2, 5, 2, 6), at('other.yaml', 1, 1)]);
    expect(mark).toMatchObject({ from: 14, to: 15, severity: 'error', code: 'V2001' });
    expect(mark?.fixes.map((f) => f.title)).toEqual(['use getsUser']);
  });

  it('marks the start when there is no end, and the first line when there is no line', () => {
    const got = marks(text, path, [at(path, 2, 1), at(path, 0, 0)]);
    expect(got.map(({ from, to }) => [from, to])).toEqual([
      [10, 10],
      [0, 0],
    ]);
  });

  it('reads warnings as warnings and anything else as info', () => {
    const warning = { ...at(path, 1, 1), severity: Severity.WARNING };
    const info = { ...at(path, 1, 1), severity: Severity.INFO };
    expect(marks(text, path, [warning, info]).map((m) => m.severity)).toEqual(['warning', 'info']);
  });
});
