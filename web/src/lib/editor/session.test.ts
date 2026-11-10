import { create } from '@bufbuild/protobuf';
import { Code, ConnectError } from '@connectrpc/connect';
import { FixSchema, Severity } from '#lib/gen/vibranium/engine/v1/engine_pb.ts';
import { describe, expect, it, vi } from 'vitest';
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

/** An engine whose check reports an error while the unsaved text has `bad` in it. */
function overlayDisk(checkDelayMs = 0) {
  const overlays: string[] = [];
  const writes: string[] = [];
  const client = fakeEngine({
    readFile: () => ({ text: 'name: one\n', digest: 'd1' }),
    writeFile: ({ text }) => {
      writes.push(text);
      return { digest: 'd2', diagnostics: [] };
    },
    check: (request) => {
      const text = request.overlay[path];
      if (text !== undefined) overlays.push(text);
      const error = { ...diagnostic(path, 'V1001'), severity: Severity.ERROR };
      return { diagnostics: text?.includes('bad') ? [error] : [] };
    },
  });
  return { overlays, writes, editor: new EditorSession(client, checkDelayMs) };
}

/** A promise and the function that settles it, for answering a request later. */
function later<T>() {
  let settle: (value: T) => void = () => {};
  const promise = new Promise<T>((resolve) => (settle = resolve));
  return { promise, settle };
}

describe('EditorSession', () => {
  it('checks the unsaved text once typing pauses and blocks saving while it has errors', async () => {
    const { overlays, editor } = overlayDisk();
    await editor.open(path);
    editor.edit('name: b');
    editor.edit('name: bad');
    await vi.waitFor(() => expect(editor.errorCount).toBe(1));
    expect(overlays).toEqual(['name: bad']);
    expect(editor.canSave).toBe(false);
    editor.edit('name: good');
    await vi.waitFor(() => expect(editor.errorCount).toBe(0));
    expect(editor.canSave).toBe(true);
  });

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

  it('checks the text again on save and refuses it when the last check is out of date', async () => {
    const { writes, editor } = overlayDisk(60_000);
    await editor.open(path);
    editor.edit('name: bad');
    expect(editor.canSave).toBe(true);
    await editor.save();
    expect(writes).toEqual([]);
    expect(editor.errorCount).toBe(1);
    expect(editor.state).toBe('idle');
    expect(editor.dirty).toBe(true);
  });

  it('keeps keys typed while a save is in flight, unsaved', async () => {
    const written = later<{ digest: string; diagnostics: [] }>();
    const editor = new EditorSession(
      fakeEngine({
        readFile: () => ({ text: 'name: one\n', digest: 'd1' }),
        check: () => ({ diagnostics: [] }),
        writeFile: () => written.promise,
      }),
      60_000,
    );
    await editor.open(path);
    editor.edit('name: two\n');
    const saving = editor.save();
    await vi.waitFor(() => expect(editor.state).toBe('saving'));
    editor.edit('name: three\n');
    written.settle({ digest: 'd2', diagnostics: [] });
    await saving;
    expect(editor.buffer).toMatchObject({
      text: 'name: three\n',
      original: 'name: two\n',
      digest: 'd2',
    });
    expect(editor.dirty).toBe(true);
  });

  it('leaves a file opened during a save or keep-mine alone when that answers late', async () => {
    const other = 'services/users/other.test.yaml';
    const held = new Set<string>();
    const reads = new Map<string, (file: { text: string; digest: string }) => void>();
    const answer = (file: string, text: string, digest: string) => {
      held.delete(file);
      reads.get(file)?.({ text, digest });
    };
    let refuse: (error: ConnectError) => void = () => {};
    const writes: string[] = [];
    const editor = new EditorSession(
      fakeEngine({
        readFile: ({ path: file }) =>
          held.has(file)
            ? new Promise((resolve) => reads.set(file, resolve))
            : { text: `${file}\n`, digest: 'd1' },
        check: () => ({ diagnostics: [] }),
        writeFile: ({ path: file }) => {
          writes.push(file);
          return new Promise<never>((_, reject) => (refuse = reject));
        },
      }),
      60_000,
    );
    await editor.open(path);
    editor.edit('mine\n');
    const saving = editor.save();
    await vi.waitFor(() => expect(writes).toEqual([path]));
    held.add(other);
    const opening = editor.open(other);
    refuse(new ConnectError('changed on disk', Code.Aborted));
    await saving;
    expect(editor.state).toBe('loading');
    answer(other, 'theirs\n', 'd7');
    await opening;
    expect(editor.state).toBe('idle');
    expect(editor.buffer).toMatchObject({ path: other, text: 'theirs\n', digest: 'd7' });

    await editor.open(path);
    editor.edit('mine again\n');
    const conflicting = editor.save();
    await vi.waitFor(() => expect(writes).toHaveLength(2));
    refuse(new ConnectError('changed on disk', Code.Aborted));
    await conflicting;
    expect(editor.state).toBe('conflict');
    held.add(path);
    const keeping = editor.keepMine();
    await editor.open(other);
    editor.edit('edited\n');
    answer(path, 'disk\n', 'd9');
    await keeping;
    expect(editor.buffer).toMatchObject({ path: other, text: 'edited\n', digest: 'd1' });
    expect(writes).toHaveLength(2);
    expect(editor.state).toBe('idle');
  });

  it('shows the file opened last when an earlier read answers after it', async () => {
    const reads = new Map<string, (file: { text: string; digest: string }) => void>();
    const editor = new EditorSession(
      fakeEngine({
        readFile: ({ path: file }) => {
          const read = later<{ text: string; digest: string }>();
          reads.set(file, read.settle);
          return read.promise;
        },
        check: () => ({ diagnostics: [] }),
      }),
    );
    const first = editor.open('a.test.yaml');
    const second = editor.open('b.test.yaml');
    await vi.waitFor(() => expect(reads.size).toBe(2));
    reads.get('b.test.yaml')?.({ text: 'b\n', digest: 'db' });
    await second;
    reads.get('a.test.yaml')?.({ text: 'a\n', digest: 'da' });
    await first;
    expect(editor.buffer).toMatchObject({ path: 'b.test.yaml', text: 'b\n', digest: 'db' });
    expect(editor.state).toBe('idle');
  });

  it('takes the new digest when the check after a save fails, and says so until the next check', async () => {
    const editor = new EditorSession(
      fakeEngine({
        readFile: () => ({ text: 'name: one\n', digest: 'd1' }),
        check: () => ({ diagnostics: [] }),
        writeFile: () => ({ digest: 'd2', diagnostics: [], checkError: 'hook timed out' }),
      }),
      0,
    );
    await editor.open(path);
    editor.edit('name: two\n');
    await editor.save();
    expect(editor.buffer).toMatchObject({ original: 'name: two\n', digest: 'd2' });
    expect(editor.dirty).toBe(false);
    expect(editor.checkError).toBe('hook timed out');
    editor.edit('name: three\n');
    await vi.waitFor(() => expect(editor.checkError).toBeUndefined());
  });

  it('keeps a save conflict or failure shown while later edits check clean', async () => {
    const checked: string[] = [];
    let refusal = new ConnectError('changed on disk', Code.Aborted);
    const client = fakeEngine({
      readFile: () => ({ text: 'name: one\n', digest: 'd1' }),
      writeFile: () => {
        throw refusal;
      },
      check: (request) => {
        const text = request.overlay[path];
        if (text === 'name: down\n') throw new ConnectError('engine went away', Code.Unavailable);
        if (text !== undefined) checked.push(text);
        return { diagnostics: [] };
      },
    });
    const editor = new EditorSession(client, 0);
    await editor.open(path);
    editor.edit('mine\n');
    await editor.save();
    editor.edit('mine, more\n');
    await vi.waitFor(() => expect(checked.at(-1)).toBe('mine, more\n'));
    expect(editor.state).toBe('conflict');

    refusal = new ConnectError('disk full', Code.ResourceExhausted);
    await editor.keepMine();
    expect(editor.error).toBe('disk full');
    editor.edit('mine, still more\n');
    await vi.waitFor(() => expect(checked.at(-1)).toBe('mine, still more\n'));
    expect(editor.error).toBe('disk full');

    await editor.open(path);
    editor.edit('name: down\n');
    await vi.waitFor(() => expect(editor.error).toBe('engine went away'));
    editor.edit('name: up\n');
    await vi.waitFor(() => expect(editor.error).toBeUndefined());
  });

  it('shows a failed read as an error', async () => {
    const editor = new EditorSession(fakeEngine({}));
    await editor.open(path);
    expect(editor.error).toMatch(/unimplemented|not implemented/i);
    expect(editor.buffer).toBeUndefined();
  });
});
