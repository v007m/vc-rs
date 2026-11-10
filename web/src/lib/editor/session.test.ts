import { create } from '@bufbuild/protobuf';
import { Code, ConnectError } from '@connectrpc/connect';
import { FixSchema } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { describe, expect, it } from 'vitest';
import { fakeEngine } from '#lib/testing/engine.ts';
import { EditorSession } from './session.svelte.ts';

const path = 'services/users/crud.test.yaml';

function diagnostic(file: string, code: string) {
  return {
    code,
    message: `${code} here`,
    primary: { span: { file, startLine: 1, startColumn: 1 } },
  };
}

/** A one-file disk the fake engine reads and writes; `stale` makes the next write conflict. */
function disk(text = 'name: one\n') {
  const state = { text, digest: 'd1', stale: false, writes: 0 };
  const client = fakeEngine({
    listDefinitions: () => ({
      definitions: [{ path: 'b.test.yaml' }, { path: 'a.api.yaml' }],
    }),
    readFile: () => ({ text: state.text, digest: state.digest }),
    writeFile: (request) => {
      if (state.stale || request.expectedDigest !== state.digest) {
        state.stale = false;
        throw new ConnectError('changed on disk', Code.Aborted);
      }
      state.writes += 1;
      state.text = request.text;
      state.digest = `d${state.writes + 1}`;
      return { digest: state.digest, diagnostics: [diagnostic(path, 'V2001')] };
    },
    check: () => ({ diagnostics: [diagnostic(path, 'V4001'), diagnostic('other.yaml', 'V1001')] }),
  });
  return { state, editor: new EditorSession(client) };
}

describe('EditorSession', () => {
  it('lists the files sorted and opens one with its diagnostics', async () => {
    const { editor } = disk();
    await editor.loadFiles();
    expect(editor.files).toEqual(['a.api.yaml', 'b.test.yaml']);
    await editor.open(path);
    expect(editor.buffer?.text).toBe('name: one\n');
    expect(editor.fileDiagnostics.map((d) => d.code)).toEqual(['V4001']);
    expect(editor.dirty).toBe(false);
  });

  it('saves edits with the digest it read and takes the diagnostics of the write', async () => {
    const { state, editor } = disk();
    await editor.open(path);
    editor.edit('name: two\n');
    expect(editor.dirty).toBe(true);
    await editor.save();
    expect(state.text).toBe('name: two\n');
    expect(editor.dirty).toBe(false);
    expect(editor.buffer?.digest).toBe('d2');
    expect(editor.fileDiagnostics.map((d) => d.code)).toEqual(['V2001']);
  });

  it('reports a conflict when the file changed on disk, and keeps the edits on request', async () => {
    const { state, editor } = disk();
    await editor.open(path);
    editor.edit('mine\n');
    state.stale = true;
    await editor.save();
    expect(editor.state).toBe('conflict');
    expect(editor.dirty).toBe(true);
    await editor.keepMine();
    expect(editor.state).toBe('idle');
    expect(state.text).toBe('mine\n');
  });

  it('drops the edits and reads the disk again on reload', async () => {
    const { state, editor } = disk();
    await editor.open(path);
    editor.edit('mine\n');
    state.text = 'theirs\n';
    state.digest = 'd9';
    await editor.save();
    expect(editor.state).toBe('conflict');
    await editor.reload();
    expect(editor.buffer?.text).toBe('theirs\n');
    expect(editor.state).toBe('idle');
  });

  it('applies a fix to the buffer without saving it', async () => {
    const { state, editor } = disk('name: één\nid: x\n');
    await editor.open(path);
    editor.applyFix(
      create(FixSchema, {
        title: 'rename',
        edits: [
          { file: path, start: 16, end: 17, newText: 'getsUser' },
          { file: 'other.yaml', start: 0, end: 1, newText: 'no' },
        ],
      }),
    );
    expect(editor.buffer?.text).toBe('name: één\nid: getsUser\n');
    expect(state.writes).toBe(0);
  });

  it('shows a failed read as an error', async () => {
    const editor = new EditorSession(fakeEngine({}));
    await editor.open(path);
    expect(editor.error).toMatch(/unimplemented|not implemented/i);
    expect(editor.buffer).toBeUndefined();
  });
});
